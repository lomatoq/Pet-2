use std::{
    ffi::c_void,
    mem::size_of,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use directories::ProjectDirs;
use glam::Vec2;
use lifecore::AppCategory;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT},
    Graphics::{
        Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
        Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HALFTONE, HBITMAP, HDC, HGDIOBJ,
            NOMIRRORBITMAP, ReleaseDC, SRCCOPY, SelectObject, SetStretchBltMode, StretchBlt,
        },
    },
    System::{
        SystemInformation::GetTickCount64,
        Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
    },
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO, VK_LBUTTON},
        WindowsAndMessaging::{
            EnumWindows, GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
            GetWindowRect, GetWindowThreadProcessId, HWND_TOPMOST, IsWindowVisible,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW,
            SetWindowPos, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        },
    },
};
use winit::platform::windows::WindowAttributesExtWindows;
use winit::window::{Window, WindowAttributes};

use crate::{
    ApplicationInfo, DesktopBackgroundFrame, DesktopSnapshot, DesktopSurface, DesktopVisualFrame,
    DesktopVisualSample, DisplayTopology, HostError, PhysicalDesktopPoint, PlatformBackend,
    PlatformCapabilities, PlatformKind, RectI, VISUAL_GRID_CELLS, VISUAL_GRID_HEIGHT,
    VISUAL_GRID_WIDTH, VisualCell,
};

const VISUAL_CAPTURE_SAMPLES_PER_AXIS: usize = 4;
const VISUAL_CAPTURE_WIDTH: usize = VISUAL_GRID_WIDTH * VISUAL_CAPTURE_SAMPLES_PER_AXIS;
const VISUAL_CAPTURE_HEIGHT: usize = VISUAL_GRID_HEIGHT * VISUAL_CAPTURE_SAMPLES_PER_AXIS;

pub struct WindowsBackend {
    started: Instant,
    overlay_hwnd: Option<HWND>,
    cursor_hittest_enabled: Option<bool>,
    visual_worker: Option<VisualSampleWorker>,
    background_worker: Option<GdiBackgroundWorker>,
    cached_surfaces: Vec<DesktopSurface>,
    last_window_enumeration: Instant,
    cached_foreground_hwnd: HWND,
    cached_active_application: Option<ApplicationInfo>,
}

pub(crate) fn prepare_overlay_window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    // DirectComposition owns presentation for this window, so the legacy DWM
    // redirection bitmap must not sit behind the transparent swapchain.
    attributes.with_no_redirection_bitmap(true)
}

impl Default for WindowsBackend {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            overlay_hwnd: None,
            cursor_hittest_enabled: None,
            visual_worker: None,
            background_worker: None,
            cached_surfaces: Vec::with_capacity(32),
            last_window_enumeration: Instant::now() - Duration::from_secs(1),
            cached_foreground_hwnd: std::ptr::null_mut(),
            cached_active_application: None,
        }
    }
}

impl PlatformBackend for WindowsBackend {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Windows
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            global_cursor: true,
            user_idle_time: true,
            active_application: true,
            visible_window_geometry: true,
            transparent_overlay: true,
            dynamic_cursor_hittest: true,
            nonactivating_overlay: true,
            multiple_monitors: true,
            microphone: false,
            camera: false,
            screen_capture: true,
        }
    }

    fn initialize(&mut self, window: &Window) -> Result<(), HostError> {
        let handle = window
            .window_handle()
            .map_err(|error| HostError::WindowHandle(error.to_string()))?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err(HostError::WindowHandle("expected a Win32 HWND".into()));
        };
        self.overlay_hwnd = Some(handle.hwnd.get() as HWND);
        // A virtual-desktop-sized transparent host must start click-through. The
        // liquid hit-test enables input only while the pointer is over the Pet or
        // an already captured drag is active.
        window.set_cursor_hittest(false).map_err(|error| {
            HostError::Platform(format!("could not initialize cursor hit testing: {error}"))
        })?;
        self.cursor_hittest_enabled = Some(false);
        self.visual_worker = Some(VisualSampleWorker::new());
        self.background_worker = Some(GdiBackgroundWorker::new(self.started));
        Ok(())
    }

    fn poll_desktop(&mut self, _topology: &DisplayTopology) -> DesktopSnapshot {
        let foreground = unsafe { GetForegroundWindow() };
        let cursor = cursor_position();
        let idle_seconds = idle_seconds();
        let active_window = window_bounds(foreground);
        let active_application = if foreground == self.cached_foreground_hwnd {
            self.cached_active_application.clone()
        } else {
            self.cached_foreground_hwnd = foreground;
            self.cached_active_application = application_for_window(foreground);
            self.cached_active_application.clone()
        };
        if self.last_window_enumeration.elapsed() >= Duration::from_millis(100) {
            let mut context = EnumerationContext {
                overlay: self.overlay_hwnd,
                surfaces: Vec::with_capacity(32),
            };
            unsafe {
                EnumWindows(
                    Some(enum_window),
                    &mut context as *mut EnumerationContext as LPARAM,
                );
            }
            self.cached_surfaces = context.surfaces;
            self.last_window_enumeration = Instant::now();
        }
        DesktopSnapshot {
            timestamp: self.started.elapsed().as_secs_f64(),
            topology_revision: _topology.revision,
            cursor,
            primary_button_down: Some(unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } < 0),
            idle_seconds,
            active_application,
            active_window,
            visible_surfaces: self.cached_surfaces.clone(),
        }
    }

    fn poll_visual_features(
        &mut self,
        topology: &DisplayTopology,
        pet_position: Vec2,
    ) -> Option<DesktopVisualFrame> {
        self.visual_worker
            .as_mut()?
            .submit_and_poll(topology, pet_position)
    }

    fn capture_overlay_background(&mut self, window: &Window) -> Option<DesktopBackgroundFrame> {
        self.background_worker.as_mut()?.submit_and_poll(window)
    }

    fn apply_overlay_policy(&mut self, _window: &Window) -> Result<(), HostError> {
        let hwnd = self
            .overlay_hwnd
            .ok_or_else(|| HostError::WindowHandle("backend is not initialized".into()))?;
        unsafe {
            let mut style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            style |= WS_EX_NOACTIVATE;
            style |= WS_EX_TOOLWINDOW;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
            let success = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            if success == 0 {
                return Err(HostError::Platform("SetWindowPos failed".into()));
            }
        }
        Ok(())
    }

    fn set_cursor_hittest(&mut self, window: &Window, enabled: bool) -> Result<(), HostError> {
        // `set_cursor_hittest` changes native window state. Reapplying it at the
        // 60 Hz sensor cadence on a full-desktop HWND causes an avoidable DWM/input
        // hitch exactly when a press begins. Only native state transitions belong
        // here; steady hover/drag samples are free.
        if self.cursor_hittest_enabled == Some(enabled) {
            return Ok(());
        }
        window.set_cursor_hittest(enabled).map_err(|error| {
            HostError::Platform(format!("could not update cursor hit testing: {error}"))
        })?;
        self.cursor_hittest_enabled = Some(enabled);
        Ok(())
    }

    fn app_data_directory(&self) -> Result<PathBuf, HostError> {
        ProjectDirs::from("io", "lomatoq", "Pet 2")
            .map(|dirs| dirs.data_local_dir().to_path_buf())
            .ok_or(HostError::AppDataUnavailable)
    }

    fn shutdown(&mut self) {
        if let Some(mut worker) = self.visual_worker.take() {
            worker.shutdown();
        }
        if let Some(mut worker) = self.background_worker.take() {
            worker.shutdown();
        }
    }
}

#[derive(Debug, Clone)]
struct VisualSampleRequest {
    topology: DisplayTopology,
    pet_position: Vec2,
}

struct VisualSampleWorker {
    request_tx: Option<Sender<VisualSampleRequest>>,
    sample_rx: Receiver<DesktopVisualFrame>,
    join: Option<JoinHandle<()>>,
    latest: Option<DesktopVisualFrame>,
}

impl VisualSampleWorker {
    fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<VisualSampleRequest>();
        let (sample_tx, sample_rx) = mpsc::channel::<DesktopVisualFrame>();
        let join = thread::Builder::new()
            .name("pet2-visual-sample".into())
            .spawn(move || visual_sample_loop(&request_rx, &sample_tx))
            .ok();
        Self {
            request_tx: Some(request_tx),
            sample_rx,
            join,
            latest: None,
        }
    }

    fn submit_and_poll(
        &mut self,
        topology: &DisplayTopology,
        pet_position: Vec2,
    ) -> Option<DesktopVisualFrame> {
        if let Some(sample) = self.sample_rx.try_iter().last() {
            self.latest = Some(sample);
        }
        let _ = self.request_tx.as_ref().map(|sender| {
            sender.send(VisualSampleRequest {
                topology: topology.clone(),
                pet_position,
            })
        });
        if let Some(sample) = self.sample_rx.try_iter().last() {
            self.latest = Some(sample);
        }
        self.latest
    }

    fn shutdown(&mut self) {
        self.request_tx.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for VisualSampleWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn visual_sample_loop(
    requests: &Receiver<VisualSampleRequest>,
    samples: &Sender<DesktopVisualFrame>,
) {
    let mut previous = Vec::with_capacity(64);
    let mut capture =
        GdiBackgroundCapture::new(VISUAL_CAPTURE_WIDTH as u32, VISUAL_CAPTURE_HEIGHT as u32);
    let started = Instant::now();
    let mut sequence = 0_u64;
    while let Ok(mut request) = requests.recv() {
        for newer in requests.try_iter() {
            request = newer;
        }
        sequence = sequence.saturating_add(1);
        let Some(capture) = capture.as_mut() else {
            continue;
        };
        let Some(sample) = sample_desktop_visual(
            &request.topology,
            request.pet_position,
            &mut previous,
            capture,
            sequence,
            started.elapsed().as_secs_f64(),
        ) else {
            continue;
        };
        if samples.send(sample).is_err() {
            break;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CaptureRequest {
    physical_rect: RectI,
    width: u32,
    height: u32,
}

struct GdiBackgroundWorker {
    request_tx: Option<Sender<CaptureRequest>>,
    frame_rx: Receiver<DesktopBackgroundFrame>,
    join: Option<JoinHandle<()>>,
    last_submit: Instant,
}

impl GdiBackgroundWorker {
    fn new(started: Instant) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<CaptureRequest>();
        let (frame_tx, frame_rx) = mpsc::channel::<DesktopBackgroundFrame>();
        let join = thread::Builder::new()
            .name("pet2-desktop-capture".into())
            .spawn(move || background_capture_loop(started, &request_rx, &frame_tx))
            .ok();
        Self {
            request_tx: Some(request_tx),
            frame_rx,
            join,
            last_submit: Instant::now() - Duration::from_secs(1),
        }
    }

    fn submit_and_poll(&mut self, window: &Window) -> Option<DesktopBackgroundFrame> {
        let mut latest = self.frame_rx.try_iter().last();
        if self.last_submit.elapsed() >= Duration::from_secs_f64(1.0 / 30.0) {
            let position = window.outer_position().ok()?;
            let size = window.inner_size();
            if size.width > 0 && size.height > 0 {
                let request = CaptureRequest {
                    physical_rect: RectI {
                        minimum: PhysicalDesktopPoint {
                            x: position.x,
                            y: position.y,
                        },
                        maximum: PhysicalDesktopPoint {
                            x: position.x.saturating_add(size.width as i32),
                            y: position.y.saturating_add(size.height as i32),
                        },
                    },
                    width: size.width.div_ceil(2),
                    height: size.height.div_ceil(2),
                };
                if self
                    .request_tx
                    .as_ref()
                    .is_some_and(|sender| sender.send(request).is_ok())
                {
                    self.last_submit = Instant::now();
                }
            }
            latest = self.frame_rx.try_iter().last().or(latest);
        }
        latest
    }

    fn shutdown(&mut self) {
        self.request_tx.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for GdiBackgroundWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn background_capture_loop(
    started: Instant,
    requests: &Receiver<CaptureRequest>,
    frames: &Sender<DesktopBackgroundFrame>,
) {
    let mut capture: Option<GdiBackgroundCapture> = None;
    let mut sequence = 0_u64;
    while let Ok(mut request) = requests.recv() {
        for newer in requests.try_iter() {
            request = newer;
        }
        let resize = capture.as_ref().is_none_or(|capture| {
            capture.width != request.width || capture.height != request.height
        });
        if resize {
            capture = GdiBackgroundCapture::new(request.width, request.height);
        }
        let Some(capture) = capture.as_mut() else {
            continue;
        };
        let Some(tight) = capture.capture(
            request.physical_rect.minimum.x,
            request.physical_rect.minimum.y,
            request.physical_rect.width().max(1) as u32,
            request.physical_rect.height().max(1) as u32,
        ) else {
            continue;
        };
        let tight_row = request.width as usize * 4;
        let bytes_per_row = request.width.saturating_mul(4).div_ceil(256) * 256;
        let mut bgra8 = vec![0_u8; bytes_per_row as usize * request.height as usize];
        for row in 0..request.height as usize {
            let source = &tight[row * tight_row..(row + 1) * tight_row];
            let target =
                &mut bgra8[row * bytes_per_row as usize..row * bytes_per_row as usize + tight_row];
            target.copy_from_slice(source);
        }
        let (mean_luminance, contrast) =
            background_luminance_stats(&bgra8, request.width, request.height, bytes_per_row);
        sequence = sequence.wrapping_add(1);
        if frames
            .send(DesktopBackgroundFrame {
                width: request.width,
                height: request.height,
                bytes_per_row,
                bgra8,
                physical_rect: request.physical_rect,
                sequence,
                timestamp: started.elapsed().as_secs_f64(),
                mean_luminance,
                contrast,
            })
            .is_err()
        {
            break;
        }
    }
}

fn background_luminance_stats(
    bgra8: &[u8],
    width: u32,
    height: u32,
    bytes_per_row: u32,
) -> (f32, f32) {
    let stride = (width.min(height) / 24).max(1) as usize;
    let mut sum = 0.0_f32;
    let mut sum_squared = 0.0_f32;
    let mut count = 0.0_f32;
    for y in (0..height as usize).step_by(stride) {
        for x in (0..width as usize).step_by(stride) {
            let index = y * bytes_per_row as usize + x * 4;
            let blue = bgra8[index] as f32 / 255.0;
            let green = bgra8[index + 1] as f32 / 255.0;
            let red = bgra8[index + 2] as f32 / 255.0;
            let value = red * 0.2126 + green * 0.7152 + blue * 0.0722;
            sum += value;
            sum_squared += value * value;
            count += 1.0;
        }
    }
    let mean = sum / count.max(1.0);
    let contrast = (sum_squared / count.max(1.0) - mean * mean).max(0.0).sqrt();
    (mean, contrast)
}

struct GdiBackgroundCapture {
    screen_dc: HDC,
    memory_dc: HDC,
    bitmap: HBITMAP,
    previous_bitmap: HGDIOBJ,
    bits: *mut u8,
    width: u32,
    height: u32,
}

impl GdiBackgroundCapture {
    fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return None;
        }
        let screen_dc = unsafe { GetDC(std::ptr::null_mut()) };
        if screen_dc.is_null() {
            return None;
        }
        let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
        if memory_dc.is_null() {
            unsafe { ReleaseDC(std::ptr::null_mut(), screen_dc) };
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // A negative height keeps the DIB top-down and avoids a CPU row flip.
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: width.saturating_mul(height).saturating_mul(4),
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = unsafe {
            CreateDIBSection(
                screen_dc,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            )
        };
        if bitmap.is_null() || bits.is_null() {
            unsafe {
                DeleteDC(memory_dc);
                ReleaseDC(std::ptr::null_mut(), screen_dc);
            }
            return None;
        }
        let previous_bitmap = unsafe { SelectObject(memory_dc, bitmap) };
        if previous_bitmap.is_null() || previous_bitmap.addr() == usize::MAX {
            unsafe {
                DeleteObject(bitmap);
                DeleteDC(memory_dc);
                ReleaseDC(std::ptr::null_mut(), screen_dc);
            }
            return None;
        }
        Some(Self {
            screen_dc,
            memory_dc,
            bitmap,
            previous_bitmap,
            bits: bits.cast(),
            width,
            height,
        })
    }

    fn capture(&mut self, x: i32, y: i32, source_width: u32, source_height: u32) -> Option<&[u8]> {
        unsafe { SetStretchBltMode(self.memory_dc, HALFTONE) };
        let copied = unsafe {
            StretchBlt(
                self.memory_dc,
                0,
                0,
                self.width as i32,
                self.height as i32,
                self.screen_dc,
                x,
                y,
                source_width as i32,
                source_height as i32,
                SRCCOPY | NOMIRRORBITMAP,
            )
        };
        if copied == 0 {
            return None;
        }
        let length = self.width as usize * self.height as usize * 4;
        Some(unsafe { std::slice::from_raw_parts(self.bits, length) })
    }
}

impl Drop for GdiBackgroundCapture {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory_dc, self.previous_bitmap);
            DeleteObject(self.bitmap);
            DeleteDC(self.memory_dc);
            ReleaseDC(std::ptr::null_mut(), self.screen_dc);
        }
    }
}

fn sample_desktop_visual(
    topology: &DisplayTopology,
    pet_position: Vec2,
    previous: &mut Vec<[f32; 3]>,
    capture: &mut GdiBackgroundCapture,
    sequence: u64,
    timestamp: f64,
) -> Option<DesktopVisualFrame> {
    let virtual_bounds = topology.virtual_physical_bounds;
    if !virtual_bounds.is_valid() {
        return None;
    }
    let pixels = capture.capture(
        virtual_bounds.minimum.x,
        virtual_bounds.minimum.y,
        virtual_bounds.width().max(1) as u32,
        virtual_bounds.height().max(1) as u32,
    )?;
    let sampled = sample_visual_cells(pixels, previous);
    let (mut cells, mut means) = sampled?;
    // GetDC(NULL) observes the composed desktop, including our own topmost
    // organism. Without a self-mask the bright liquid body becomes the most
    // salient "external" color/shape, so attention locks to its own position
    // and locomotion appears dead. Replace the 3x3 footprint around the body
    // with a neutral estimate before perception or temporal differencing sees
    // it. The worker stores these scrubbed means for the next frame as well.
    let self_mask = visual_self_mask(pet_position);
    let neutral_rgb = mean_unmasked_rgb(&means, &self_mask);
    for (index, masked) in self_mask.iter().copied().enumerate() {
        if !masked {
            continue;
        }
        means[index] = neutral_rgb;
        cells[index].luminance = luminance(neutral_rgb);
        cells[index].contrast = 0.0;
        cells[index].colorfulness = 0.0;
        cells[index].motion = 0.0;
        cells[index].edge_density = 0.0;
        cells[index].sudden_change = 0.0;
    }
    let unmasked_means = means
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, sample)| (!self_mask[index]).then_some(sample))
        .collect::<Vec<_>>();
    let (mean_luminance, contrast, colorfulness, warmth, dominant_hue, edge_density) =
        summarize_samples(&unmasked_means);
    let motion_energy = cells.iter().map(|cell| cell.motion).sum::<f32>() / cells.len() as f32;
    let sudden_change = cells
        .iter()
        .map(|cell| cell.sudden_change)
        .fold(0.0_f32, f32::max);
    let column = (pet_position.x.clamp(0.0, 0.999_999) * VISUAL_GRID_WIDTH as f32) as usize;
    let row = (pet_position.y.clamp(0.0, 0.999_999) * VISUAL_GRID_HEIGHT as f32) as usize;
    let summary = DesktopVisualSample {
        mean_luminance,
        local_luminance: cells[row * VISUAL_GRID_WIDTH + column].luminance,
        contrast,
        colorfulness,
        warmth,
        dominant_hue,
        motion_energy,
        edge_density,
        sudden_change,
    };
    *previous = means;
    let frame = DesktopVisualFrame {
        summary,
        cells,
        sequence,
        timestamp,
    };
    frame.is_finite().then_some(frame)
}

fn sample_visual_cells(
    pixels: &[u8],
    previous: &[[f32; 3]],
) -> Option<([VisualCell; VISUAL_GRID_CELLS], Vec<[f32; 3]>)> {
    if pixels.len() < VISUAL_CAPTURE_WIDTH * VISUAL_CAPTURE_HEIGHT * 4 {
        return None;
    }
    let mut cells = [VisualCell::default(); VISUAL_GRID_CELLS];
    let mut means = Vec::with_capacity(VISUAL_GRID_CELLS);
    for row in 0..VISUAL_GRID_HEIGHT {
        for column in 0..VISUAL_GRID_WIDTH {
            let mut samples = [[0.0_f32; 3]; 16];
            let mut count = 0_usize;
            for sample_row in 0..VISUAL_CAPTURE_SAMPLES_PER_AXIS {
                for sample_column in 0..VISUAL_CAPTURE_SAMPLES_PER_AXIS {
                    let pixel_x = column * VISUAL_CAPTURE_SAMPLES_PER_AXIS + sample_column;
                    let pixel_y = row * VISUAL_CAPTURE_SAMPLES_PER_AXIS + sample_row;
                    let pixel = (pixel_y * VISUAL_CAPTURE_WIDTH + pixel_x) * 4;
                    samples[count] = [
                        pixels[pixel + 2] as f32 / 255.0,
                        pixels[pixel + 1] as f32 / 255.0,
                        pixels[pixel] as f32 / 255.0,
                    ];
                    count += 1;
                }
            }
            let valid_samples = &samples[..count];
            let mut mean_rgb = [0.0_f32; 3];
            for sample in valid_samples {
                for channel in 0..3 {
                    mean_rgb[channel] += sample[channel] / count as f32;
                }
            }
            let (luma, contrast, colorfulness, warmth, hue, fallback_edge_density) =
                summarize_samples(valid_samples);
            let edge_density = if count == 16 {
                grid_edge_density_4x4(valid_samples)
            } else {
                fallback_edge_density
            };
            let index = row * VISUAL_GRID_WIDTH + column;
            let motion = previous.get(index).map_or(0.0, |old| {
                (((mean_rgb[0] - old[0]).abs()
                    + (mean_rgb[1] - old[1]).abs()
                    + (mean_rgb[2] - old[2]).abs())
                    / 3.0
                    * 2.6)
                    .clamp(0.0, 1.0)
            });
            let previous_luma = previous.get(index).map_or(luma, |old| luminance(*old));
            cells[index] = VisualCell {
                luminance: luma,
                contrast,
                colorfulness,
                warmth,
                hue,
                motion,
                edge_density,
                sudden_change: ((luma - previous_luma).abs() * 1.8 + motion * 1.25).clamp(0.0, 1.0),
            };
            means.push(mean_rgb);
        }
    }
    Some((cells, means))
}

fn visual_self_mask(pet_position: Vec2) -> [bool; VISUAL_GRID_CELLS] {
    let column = (pet_position.x.clamp(0.0, 0.999_999) * VISUAL_GRID_WIDTH as f32) as isize;
    let row = (pet_position.y.clamp(0.0, 0.999_999) * VISUAL_GRID_HEIGHT as f32) as isize;
    std::array::from_fn(|index| {
        let candidate_row = (index / VISUAL_GRID_WIDTH) as isize;
        let candidate_column = (index % VISUAL_GRID_WIDTH) as isize;
        (candidate_row - row).abs() <= 1 && (candidate_column - column).abs() <= 1
    })
}

fn mean_unmasked_rgb(means: &[[f32; 3]], mask: &[bool; VISUAL_GRID_CELLS]) -> [f32; 3] {
    let mut sum = [0.0_f32; 3];
    let mut count = 0_u32;
    for (index, sample) in means.iter().copied().enumerate() {
        if mask.get(index).copied().unwrap_or(true) {
            continue;
        }
        for channel in 0..3 {
            sum[channel] += sample[channel];
        }
        count += 1;
    }
    if count == 0 {
        return [0.5; 3];
    }
    for channel in &mut sum {
        *channel /= count as f32;
    }
    sum
}

fn grid_edge_density_4x4(samples: &[[f32; 3]]) -> f32 {
    if samples.len() != 16 {
        return 0.0;
    }
    let luminances = samples.iter().copied().map(luminance).collect::<Vec<_>>();
    let mut gradient = 0.0_f32;
    let mut pairs = 0_u32;
    for row in 0..4 {
        for column in 0..4 {
            let index = row * 4 + column;
            if column + 1 < 4 {
                gradient += (luminances[index + 1] - luminances[index]).abs();
                pairs += 1;
            }
            if row + 1 < 4 {
                gradient += (luminances[index + 4] - luminances[index]).abs();
                pairs += 1;
            }
        }
    }
    (gradient / pairs.max(1) as f32 * 3.2).clamp(0.0, 1.0)
}

fn summarize_samples(samples: &[[f32; 3]]) -> (f32, f32, f32, f32, f32, f32) {
    if samples.is_empty() {
        return (0.5, 0.0, 0.0, 0.5, 0.0, 0.0);
    }
    let luminances: Vec<_> = samples.iter().map(|sample| luminance(*sample)).collect();
    let mean = luminances.iter().sum::<f32>() / luminances.len() as f32;
    let contrast = (luminances
        .iter()
        .map(|value| (*value - mean).powi(2))
        .sum::<f32>()
        / luminances.len() as f32)
        .sqrt()
        .mul_add(2.2, 0.0)
        .clamp(0.0, 1.0);
    let colorfulness = samples
        .iter()
        .map(|sample| {
            sample.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - sample.iter().copied().fold(f32::INFINITY, f32::min)
        })
        .sum::<f32>()
        / samples.len() as f32;
    let warmth = (0.5
        + samples
            .iter()
            .map(|sample| sample[0] - sample[2])
            .sum::<f32>()
            / samples.len() as f32
            * 0.5)
        .clamp(0.0, 1.0);
    let mut hue_vector = Vec2::ZERO;
    for sample in samples {
        let (hue, saturation) = hue_and_saturation(*sample);
        let angle = hue * std::f32::consts::TAU;
        hue_vector += Vec2::new(angle.cos(), angle.sin()) * saturation;
    }
    let dominant_hue = if hue_vector.length_squared() > 1.0e-6 {
        hue_vector
            .y
            .atan2(hue_vector.x)
            .rem_euclid(std::f32::consts::TAU)
            / std::f32::consts::TAU
    } else {
        0.0
    };
    let edge_density = if luminances.len() < 2 {
        0.0
    } else {
        (luminances
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .sum::<f32>()
            / (luminances.len() - 1) as f32
            * 3.2)
            .clamp(0.0, 1.0)
    };
    (
        mean.clamp(0.0, 1.0),
        contrast,
        colorfulness.clamp(0.0, 1.0),
        warmth,
        dominant_hue,
        edge_density,
    )
}

fn luminance(sample: [f32; 3]) -> f32 {
    sample[0] * 0.2126 + sample[1] * 0.7152 + sample[2] * 0.0722
}

fn hue_and_saturation(sample: [f32; 3]) -> (f32, f32) {
    let maximum = sample.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let minimum = sample.iter().copied().fold(f32::INFINITY, f32::min);
    let delta = maximum - minimum;
    if delta <= 1.0e-6 || maximum <= 1.0e-6 {
        return (0.0, 0.0);
    }
    let hue = if maximum == sample[0] {
        ((sample[1] - sample[2]) / delta).rem_euclid(6.0)
    } else if maximum == sample[1] {
        (sample[2] - sample[0]) / delta + 2.0
    } else {
        (sample[0] - sample[1]) / delta + 4.0
    } / 6.0;
    (hue.rem_euclid(1.0), (delta / maximum).clamp(0.0, 1.0))
}

struct EnumerationContext {
    overlay: Option<HWND>,
    surfaces: Vec<DesktopSurface>,
}

unsafe extern "system" fn enum_window(hwnd: HWND, parameter: LPARAM) -> i32 {
    let context = unsafe { &mut *(parameter as *mut EnumerationContext) };
    if Some(hwnd) == context.overlay || unsafe { IsWindowVisible(hwnd) } == 0 {
        return 1;
    }
    if let Some(bounds) = window_bounds(hwnd)
        && bounds.width() >= 80
        && bounds.height() >= 50
    {
        context.surfaces.push(DesktopSurface {
            transient_id: format!("window:{hwnd:p}"),
            bounds,
        });
    }
    1
}

fn cursor_position() -> Option<PhysicalDesktopPoint> {
    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0).then_some(PhysicalDesktopPoint {
        x: point.x,
        y: point.y,
    })
}

fn idle_seconds() -> Option<f32> {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return None;
    }
    let elapsed_milliseconds = (unsafe { GetTickCount64() } as u32).wrapping_sub(info.dwTime);
    Some(elapsed_milliseconds as f32 / 1_000.0)
}

fn window_bounds(hwnd: HWND) -> Option<RectI> {
    if hwnd.is_null() {
        return None;
    }
    let mut rect = RECT::default();
    let dwm_result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        )
    };
    if dwm_result < 0 && unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let bounds = RectI {
        minimum: PhysicalDesktopPoint {
            x: rect.left,
            y: rect.top,
        },
        maximum: PhysicalDesktopPoint {
            x: rect.right,
            y: rect.bottom,
        },
    };
    bounds.is_valid().then_some(bounds)
}

fn application_for_window(hwnd: HWND) -> Option<ApplicationInfo> {
    if hwnd.is_null() {
        return None;
    }
    let mut process_id = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) };
    if process_id == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0_u16; 32_768];
    let mut size = buffer.len() as u32;
    let success = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe { CloseHandle(process) };
    if success == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    let name = Path::new(&path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_lowercase();
    Some(ApplicationInfo {
        category: classify_application(&name),
        transient_identifier: Some(name),
    })
}

fn classify_application(name: &str) -> AppCategory {
    if [
        "code", "devenv", "idea", "rider", "excel", "word", "obsidian",
    ]
    .iter()
    .any(|needle| name.contains(needle))
    {
        AppCategory::FocusedWork
    } else if ["photoshop", "blender", "figma", "krita", "resolve"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        AppCategory::Creative
    } else if ["discord", "slack", "teams", "telegram", "signal"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        AppCategory::Communication
    } else if ["steam", "spotify", "vlc", "netflix"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        AppCategory::Entertainment
    } else if ["explorer", "settings", "taskmgr"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        AppCategory::System
    } else {
        AppCategory::Unknown
    }
}

#[cfg(test)]
mod visual_sampling_tests {
    use super::*;

    #[test]
    fn self_mask_covers_the_body_cell_and_its_immediate_neighbors() {
        let centered = visual_self_mask(Vec2::splat(0.5));
        assert_eq!(centered.into_iter().filter(|masked| *masked).count(), 9);
        let corner = visual_self_mask(Vec2::ZERO);
        assert_eq!(corner.into_iter().filter(|masked| *masked).count(), 4);
    }

    #[test]
    fn four_by_four_sampling_distinguishes_structure_from_flat_color() {
        let flat = [[0.5_f32; 3]; 16];
        assert_eq!(grid_edge_density_4x4(&flat), 0.0);
        let checker: [[f32; 3]; 16] = std::array::from_fn(|index| {
            let row = index / 4;
            let column = index % 4;
            if (row + column) % 2 == 0 {
                [0.0; 3]
            } else {
                [1.0; 3]
            }
        });
        assert!(grid_edge_density_4x4(&checker) > 0.95);
    }
}

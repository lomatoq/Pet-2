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
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                ID3D11Texture2D,
            },
            Dxgi::{
                DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTPUT_DESC, IDXGIAdapter,
                IDXGIDevice, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
            },
        },
    },
    core::Interface,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HWND, LPARAM, POINT, RECT},
    Graphics::{
        Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
        Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection,
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
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowDisplayAffinity,
            SetWindowLongPtrW, SetWindowPos, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW,
        },
    },
};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::platform::windows::WindowAttributesExtWindows;
use winit::window::{Window, WindowAttributes};

use super::visual_sampling::{
    CAPTURE_HEIGHT as VISUAL_CAPTURE_HEIGHT, CAPTURE_WIDTH as VISUAL_CAPTURE_WIDTH,
    background_luminance_stats, frame_from_bgra,
};

use crate::{
    ApplicationInfo, DesktopBackgroundCaptureRegion, DesktopBackgroundFrame, DesktopSnapshot,
    DesktopSurface, DesktopVisualFrame, DisplayTopology, HostError, PhysicalDesktopPoint,
    PlatformBackend, PlatformCapabilities, PlatformKind, RectI,
};

pub struct WindowsBackend {
    started: Instant,
    overlay_hwnd: Option<HWND>,
    overlay_capture_excluded: Option<bool>,
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
            overlay_capture_excluded: None,
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
        // The first desktop frame seeds the immutable history beneath the den.
        // Exclude this top-level overlay before it is shown so Desktop Duplication
        // reveals the real wallpaper/application pixels instead of an empty or
        // self-captured composition surface. The app clears the affinity as soon
        // as that clean frame has uploaded.
        self.set_overlay_capture_excluded(window, true)?;
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

    fn capture_overlay_background(
        &mut self,
        window: &Window,
        region: DesktopBackgroundCaptureRegion,
    ) -> Option<DesktopBackgroundFrame> {
        self.background_worker
            .as_mut()?
            .submit_and_poll(window, region)
    }

    fn set_overlay_capture_excluded(
        &mut self,
        _window: &Window,
        excluded: bool,
    ) -> Result<(), HostError> {
        if self.overlay_capture_excluded == Some(excluded) {
            return Ok(());
        }
        let hwnd = self
            .overlay_hwnd
            .ok_or_else(|| HostError::WindowHandle("backend is not initialized".into()))?;
        let affinity = if excluded {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            WDA_NONE
        };
        let success = unsafe { SetWindowDisplayAffinity(hwnd, affinity) };
        if success == 0 {
            return Err(HostError::Platform(format!(
                "SetWindowDisplayAffinity({affinity:#x}) failed with Win32 error {}",
                unsafe { GetLastError() }
            )));
        }
        self.overlay_capture_excluded = Some(excluded);
        Ok(())
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
        if self.overlay_capture_excluded == Some(true)
            && let Some(hwnd) = self.overlay_hwnd
        {
            unsafe {
                SetWindowDisplayAffinity(hwnd, WDA_NONE);
            }
            self.overlay_capture_excluded = Some(false);
        }
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
    normalized_region: [f32; 4],
}

fn capture_request(
    window_position: PhysicalPosition<i32>,
    window_size: PhysicalSize<u32>,
    region: DesktopBackgroundCaptureRegion,
) -> Option<CaptureRequest> {
    if window_size.width == 0 || window_size.height == 0 {
        return None;
    }

    let minimum = region.minimum_normalized.clamp(Vec2::ZERO, Vec2::ONE);
    let maximum = region.maximum_normalized.clamp(Vec2::ZERO, Vec2::ONE);
    if !maximum.cmpgt(minimum).all() || region.maximum_output_dimension == 0 {
        return None;
    }

    let minimum_x = (minimum.x * window_size.width as f32)
        .floor()
        .clamp(0.0, window_size.width.saturating_sub(1) as f32) as u32;
    let minimum_y = (minimum.y * window_size.height as f32)
        .floor()
        .clamp(0.0, window_size.height.saturating_sub(1) as f32) as u32;
    let maximum_x = (maximum.x * window_size.width as f32)
        .ceil()
        .clamp((minimum_x + 1) as f32, window_size.width as f32) as u32;
    let maximum_y = (maximum.y * window_size.height as f32)
        .ceil()
        .clamp((minimum_y + 1) as f32, window_size.height as f32) as u32;
    let source_width = maximum_x - minimum_x;
    let source_height = maximum_y - minimum_y;
    let scale =
        (region.maximum_output_dimension as f32 / source_width.max(source_height) as f32).min(1.0);
    let width = (source_width as f32 * scale).round().max(1.0) as u32;
    let height = (source_height as f32 * scale).round().max(1.0) as u32;

    let minimum_x_i32 = i32::try_from(minimum_x).unwrap_or(i32::MAX);
    let minimum_y_i32 = i32::try_from(minimum_y).unwrap_or(i32::MAX);
    let maximum_x_i32 = i32::try_from(maximum_x).unwrap_or(i32::MAX);
    let maximum_y_i32 = i32::try_from(maximum_y).unwrap_or(i32::MAX);
    Some(CaptureRequest {
        physical_rect: RectI {
            minimum: PhysicalDesktopPoint {
                x: window_position.x.saturating_add(minimum_x_i32),
                y: window_position.y.saturating_add(minimum_y_i32),
            },
            maximum: PhysicalDesktopPoint {
                x: window_position.x.saturating_add(maximum_x_i32),
                y: window_position.y.saturating_add(maximum_y_i32),
            },
        },
        width,
        height,
        normalized_region: [
            minimum_x as f32 / window_size.width as f32,
            minimum_y as f32 / window_size.height as f32,
            source_width as f32 / window_size.width as f32,
            source_height as f32 / window_size.height as f32,
        ],
    })
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

    fn submit_and_poll(
        &mut self,
        window: &Window,
        region: DesktopBackgroundCaptureRegion,
    ) -> Option<DesktopBackgroundFrame> {
        let mut latest = self.frame_rx.try_iter().last();
        // A local den-sized crop is small enough to follow desktop motion at
        // display cadence. This restores fluid displacement without resuming
        // full virtual-desktop uploads on the render thread.
        if self.last_submit.elapsed() >= Duration::from_secs_f64(1.0 / 60.0) {
            let position = window.outer_position().ok()?;
            let size = window.inner_size();
            if let Some(request) = capture_request(position, size, region)
                && self
                    .request_tx
                    .as_ref()
                    .is_some_and(|sender| sender.send(request).is_ok())
            {
                self.last_submit = Instant::now();
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
    let mut dxgi_capture: Option<DxgiBackgroundCapture> = None;
    let mut gdi_fallback: Option<GdiBackgroundCapture> = None;
    let mut last_dxgi_attempt = Instant::now() - Duration::from_secs(2);
    let mut dxgi_failure_reported = false;
    let mut sequence = 0_u64;
    while let Ok(mut request) = requests.recv() {
        for newer in requests.try_iter() {
            request = newer;
        }

        if dxgi_capture
            .as_ref()
            .is_some_and(|capture| !capture.covers(request.physical_rect))
        {
            dxgi_capture = None;
        }

        if dxgi_capture.is_none() && last_dxgi_attempt.elapsed() >= Duration::from_secs(1) {
            last_dxgi_attempt = Instant::now();
            match DxgiBackgroundCapture::new(request.physical_rect) {
                Ok(capture) => {
                    if capture.covers(request.physical_rect) {
                        dxgi_capture = Some(capture);
                        dxgi_failure_reported = false;
                    }
                }
                Err(error) => {
                    if !dxgi_failure_reported {
                        eprintln!("DXGI desktop capture unavailable; using GDI fallback: {error}");
                        dxgi_failure_reported = true;
                    }
                }
            }
        }

        let mut tight = None;
        if let Some(capture) = dxgi_capture.as_mut() {
            match capture.capture(&request) {
                Ok(frame) => tight = frame,
                Err(error) => {
                    if !dxgi_failure_reported {
                        eprintln!("DXGI desktop capture interrupted; retrying: {error}");
                        dxgi_failure_reported = true;
                    }
                    dxgi_capture = None;
                }
            }
        }

        if tight.is_none() {
            let resize = gdi_fallback.as_ref().is_none_or(|capture| {
                capture.width != request.width || capture.height != request.height
            });
            if resize {
                gdi_fallback = GdiBackgroundCapture::new(request.width, request.height);
            }
            tight = gdi_fallback.as_mut().and_then(|capture| {
                capture
                    .capture(
                        request.physical_rect.minimum.x,
                        request.physical_rect.minimum.y,
                        request.physical_rect.width().max(1) as u32,
                        request.physical_rect.height().max(1) as u32,
                    )
                    .map(<[u8]>::to_vec)
            });
        }

        let Some(tight) = tight else {
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
                normalized_region: request.normalized_region,
            })
            .is_err()
        {
            break;
        }
    }
}

struct DxgiBackgroundCapture {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging: Option<ID3D11Texture2D>,
    staging_desc: Option<D3D11_TEXTURE2D_DESC>,
    output_rect: RectI,
    last_frame: Option<Vec<u8>>,
    last_request_rect: Option<RectI>,
    last_request_size: (u32, u32),
}

impl DxgiBackgroundCapture {
    fn new(request_rect: RectI) -> Result<Self, String> {
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map_err(|error| format!("D3D11CreateDevice failed: {error}"))?;
        let device = device.ok_or_else(|| "D3D11 returned no device".to_owned())?;
        let context = context.ok_or_else(|| "D3D11 returned no immediate context".to_owned())?;

        let dxgi_device: IDXGIDevice = device
            .cast()
            .map_err(|error| format!("ID3D11Device -> IDXGIDevice failed: {error}"))?;
        let adapter = unsafe { dxgi_device.GetAdapter() }
            .map_err(|error| format!("IDXGIDevice::GetAdapter failed: {error}"))?;
        let requested_center = PhysicalDesktopPoint {
            x: request_rect
                .minimum
                .x
                .saturating_add(request_rect.width() / 2),
            y: request_rect
                .minimum
                .y
                .saturating_add(request_rect.height() / 2),
        };

        let mut selected = None;
        for index in 0..16 {
            let Ok(output) = (unsafe { adapter.EnumOutputs(index) }) else {
                break;
            };
            let mut desc = DXGI_OUTPUT_DESC::default();
            if unsafe { output.GetDesc(&mut desc) }.is_err() || !desc.AttachedToDesktop.as_bool() {
                continue;
            }
            let rect = RectI {
                minimum: PhysicalDesktopPoint {
                    x: desc.DesktopCoordinates.left,
                    y: desc.DesktopCoordinates.top,
                },
                maximum: PhysicalDesktopPoint {
                    x: desc.DesktopCoordinates.right,
                    y: desc.DesktopCoordinates.bottom,
                },
            };
            selected.get_or_insert_with(|| (output.clone(), rect));
            if rect.contains(requested_center) {
                selected = Some((output, rect));
                break;
            }
        }
        let (output, output_rect) = selected.ok_or_else(|| "no attached DXGI output".to_owned())?;
        let output1: IDXGIOutput1 = output
            .cast()
            .map_err(|error| format!("IDXGIOutput -> IDXGIOutput1 failed: {error}"))?;
        let duplication = unsafe { output1.DuplicateOutput(&device) }
            .map_err(|error| format!("DuplicateOutput failed: {error}"))?;

        Ok(Self {
            device,
            context,
            duplication,
            staging: None,
            staging_desc: None,
            output_rect,
            last_frame: None,
            last_request_rect: None,
            last_request_size: (0, 0),
        })
    }

    fn covers(&self, rect: RectI) -> bool {
        rect.minimum.x >= self.output_rect.minimum.x
            && rect.minimum.y >= self.output_rect.minimum.y
            && rect.maximum.x <= self.output_rect.maximum.x
            && rect.maximum.y <= self.output_rect.maximum.y
    }

    fn capture(&mut self, request: &CaptureRequest) -> Result<Option<Vec<u8>>, String> {
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut desktop_resource: Option<IDXGIResource> = None;
        let acquired = unsafe {
            self.duplication
                .AcquireNextFrame(8, &mut frame_info, &mut desktop_resource)
        };
        if let Err(error) = acquired {
            if error.code() == DXGI_ERROR_WAIT_TIMEOUT {
                return Ok((self.last_request_rect == Some(request.physical_rect)
                    && self.last_request_size == (request.width, request.height))
                    .then(|| self.last_frame.clone())
                    .flatten());
            }
            return Err(format!("AcquireNextFrame failed: {error}"));
        }

        let result = (|| {
            let resource = desktop_resource
                .ok_or_else(|| "AcquireNextFrame returned no desktop resource".to_owned())?;
            let texture: ID3D11Texture2D = resource
                .cast()
                .map_err(|error| format!("desktop resource is not Texture2D: {error}"))?;
            let mut source_desc = D3D11_TEXTURE2D_DESC::default();
            unsafe { texture.GetDesc(&mut source_desc) };

            let source_left = (request.physical_rect.minimum.x - self.output_rect.minimum.x) as u32;
            let source_top = (request.physical_rect.minimum.y - self.output_rect.minimum.y) as u32;
            let source_width = request.physical_rect.width().max(1) as u32;
            let source_height = request.physical_rect.height().max(1) as u32;

            let needs_staging = self.staging_desc.is_none_or(|desc| {
                desc.Width != source_width
                    || desc.Height != source_height
                    || desc.Format != source_desc.Format
            });
            if needs_staging {
                let staging_desc = D3D11_TEXTURE2D_DESC {
                    Width: source_width,
                    Height: source_height,
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                    ..source_desc
                };
                let mut staging = None;
                unsafe {
                    self.device
                        .CreateTexture2D(&staging_desc, None, Some(&mut staging))
                }
                .map_err(|error| format!("CreateTexture2D(staging) failed: {error}"))?;
                self.staging = staging;
                self.staging_desc = Some(staging_desc);
            }
            let staging = self
                .staging
                .as_ref()
                .ok_or_else(|| "D3D11 returned no staging texture".to_owned())?;
            let source_box = D3D11_BOX {
                left: source_left,
                top: source_top,
                front: 0,
                right: source_left.saturating_add(source_width),
                bottom: source_top.saturating_add(source_height),
                back: 1,
            };
            unsafe {
                self.context.CopySubresourceRegion(
                    staging,
                    0,
                    0,
                    0,
                    0,
                    &texture,
                    0,
                    Some(&source_box),
                )
            };

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            unsafe {
                self.context
                    .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            }
            .map_err(|error| format!("Map(staging) failed: {error}"))?;
            let tight = copy_dxgi_region(
                &mapped,
                source_width,
                source_height,
                request.physical_rect,
                request,
            );
            unsafe { self.context.Unmap(staging, 0) };
            tight
        })();
        let release_result = unsafe { self.duplication.ReleaseFrame() };
        if let Err(error) = release_result {
            return Err(format!("ReleaseFrame failed: {error}"));
        }
        let tight = result?;
        self.last_frame = Some(tight.clone());
        self.last_request_rect = Some(request.physical_rect);
        self.last_request_size = (request.width, request.height);
        Ok(Some(tight))
    }
}

fn copy_dxgi_region(
    mapped: &D3D11_MAPPED_SUBRESOURCE,
    source_width: u32,
    source_height: u32,
    output_rect: RectI,
    request: &CaptureRequest,
) -> Result<Vec<u8>, String> {
    if mapped.pData.is_null() || mapped.RowPitch < source_width.saturating_mul(4) {
        return Err("DXGI mapped desktop has an invalid row pitch".to_owned());
    }
    let source_length = mapped.RowPitch as usize * source_height as usize;
    let source = unsafe { std::slice::from_raw_parts(mapped.pData.cast::<u8>(), source_length) };
    let mut tight = vec![0_u8; request.width as usize * request.height as usize * 4];
    let request_width = request.physical_rect.width().max(1) as i64;
    let request_height = request.physical_rect.height().max(1) as i64;
    for y in 0..request.height {
        let physical_y = request.physical_rect.minimum.y as i64
            + (y as i64 * request_height / request.height.max(1) as i64);
        let source_y = physical_y - output_rect.minimum.y as i64;
        if !(0..source_height as i64).contains(&source_y) {
            continue;
        }
        for x in 0..request.width {
            let physical_x = request.physical_rect.minimum.x as i64
                + (x as i64 * request_width / request.width.max(1) as i64);
            let source_x = physical_x - output_rect.minimum.x as i64;
            if !(0..source_width as i64).contains(&source_x) {
                continue;
            }
            let source_index = source_y as usize * mapped.RowPitch as usize + source_x as usize * 4;
            let target_index = (y as usize * request.width as usize + x as usize) * 4;
            tight[target_index..target_index + 4]
                .copy_from_slice(&source[source_index..source_index + 4]);
        }
    }
    Ok(tight)
}

struct GdiBackgroundCapture {
    memory_dc: HDC,
    bitmap: HBITMAP,
    previous_bitmap: HGDIOBJ,
    bits: *mut u8,
    width: u32,
    height: u32,
    copy_failure_reported: bool,
}

impl GdiBackgroundCapture {
    fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return None;
        }
        let screen_dc = unsafe { GetDC(std::ptr::null_mut()) };
        if screen_dc.is_null() {
            eprintln!("desktop capture GetDC(NULL) failed: {}", unsafe {
                GetLastError()
            });
            return None;
        }
        let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
        if memory_dc.is_null() {
            eprintln!("desktop capture CreateCompatibleDC failed: {}", unsafe {
                GetLastError()
            });
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
            eprintln!("desktop capture CreateDIBSection failed: {}", unsafe {
                GetLastError()
            });
            unsafe {
                DeleteDC(memory_dc);
                ReleaseDC(std::ptr::null_mut(), screen_dc);
            }
            return None;
        }
        let previous_bitmap = unsafe { SelectObject(memory_dc, bitmap) };
        if previous_bitmap.is_null() || previous_bitmap.addr() == usize::MAX {
            eprintln!("desktop capture SelectObject failed: {}", unsafe {
                GetLastError()
            });
            unsafe {
                DeleteObject(bitmap);
                DeleteDC(memory_dc);
                ReleaseDC(std::ptr::null_mut(), screen_dc);
            }
            return None;
        }
        unsafe { ReleaseDC(std::ptr::null_mut(), screen_dc) };
        Some(Self {
            memory_dc,
            bitmap,
            previous_bitmap,
            bits: bits.cast(),
            width,
            height,
            copy_failure_reported: false,
        })
    }

    fn capture(&mut self, x: i32, y: i32, source_width: u32, source_height: u32) -> Option<&[u8]> {
        // A display DC can be invalidated when WGPU/DirectComposition finishes
        // configuring the transparent swapchain. Acquire it for exactly one
        // copy and release it on this worker thread instead of retaining a
        // startup handle indefinitely.
        let screen_dc = unsafe { GetDC(std::ptr::null_mut()) };
        if screen_dc.is_null() {
            return None;
        }
        unsafe { SetStretchBltMode(self.memory_dc, HALFTONE) };
        let copied = if self.width == source_width && self.height == source_height {
            unsafe {
                BitBlt(
                    self.memory_dc,
                    0,
                    0,
                    self.width as i32,
                    self.height as i32,
                    screen_dc,
                    x,
                    y,
                    SRCCOPY | NOMIRRORBITMAP,
                )
            }
        } else {
            unsafe {
                StretchBlt(
                    self.memory_dc,
                    0,
                    0,
                    self.width as i32,
                    self.height as i32,
                    screen_dc,
                    x,
                    y,
                    source_width as i32,
                    source_height as i32,
                    SRCCOPY | NOMIRRORBITMAP,
                )
            }
        };
        unsafe { ReleaseDC(std::ptr::null_mut(), screen_dc) };
        if copied == 0 {
            if !self.copy_failure_reported {
                eprintln!(
                    "desktop capture BitBlt/StretchBlt failed ({}x{} <- {source_width}x{source_height} at {x},{y}): {}",
                    self.width,
                    self.height,
                    unsafe { GetLastError() }
                );
                self.copy_failure_reported = true;
            }
            return None;
        }
        self.copy_failure_reported = false;
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
    frame_from_bgra(pixels, pet_position, previous, sequence, timestamp)
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
    fn den_capture_request_is_local_and_bounded() {
        let region =
            DesktopBackgroundCaptureRegion::new(Vec2::new(0.40, 0.25), Vec2::new(0.60, 0.75), 512)
                .unwrap();
        let request = capture_request(
            PhysicalPosition::new(-1920, 0),
            PhysicalSize::new(3840, 1080),
            region,
        )
        .unwrap();

        assert_eq!(request.physical_rect.width(), 768);
        assert_eq!(request.physical_rect.height(), 540);
        assert_eq!(request.width, 512);
        assert_eq!(request.height, 360);
        assert_eq!(request.physical_rect.minimum.x, -384);
        assert!((request.normalized_region[0] - 0.40).abs() < 1.0e-6);
        assert!((request.normalized_region[2] - 0.20).abs() < 1.0e-6);
    }

    #[test]
    fn den_capture_request_clips_safely_at_overlay_edge() {
        let region =
            DesktopBackgroundCaptureRegion::new(Vec2::new(0.0, 0.0), Vec2::new(0.08, 0.12), 512)
                .unwrap();
        let request = capture_request(
            PhysicalPosition::new(100, -900),
            PhysicalSize::new(1920, 1080),
            region,
        )
        .unwrap();

        assert_eq!(request.physical_rect.minimum.x, 100);
        assert_eq!(request.physical_rect.minimum.y, -900);
        assert_eq!(request.physical_rect.width(), 154);
        assert_eq!(request.physical_rect.height(), 130);
        assert_eq!(request.width, 154);
        assert_eq!(request.height, 130);
    }
}

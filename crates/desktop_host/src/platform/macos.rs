use std::{
    ffi::c_void,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use directories::ProjectDirs;
use glam::Vec2;
use lifecore::AppCategory;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSColor, NSFloatingWindowLevel, NSView, NSWindow, NSWindowCollectionBehavior,
    NSWindowSharingType, NSWorkspace,
};
use objc2_core_foundation::{
    CFArray, CFDictionary, CFNumber, CFString, CFType, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGDisplayBounds, CGDisplayPixelsHigh,
    CGDisplayPixelsWide, CGEvent, CGEventSource, CGEventSourceStateID, CGEventType,
    CGImageAlphaInfo, CGImageByteOrderInfo, CGMainDisplayID, CGMouseButton,
    CGRectMakeWithDictionaryRepresentation, CGWindowImageOption, CGWindowListCopyWindowInfo,
    CGWindowListOption, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer, kCGWindowNumber,
    kCGWindowOwnerPID,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use super::visual_sampling::{CAPTURE_HEIGHT, CAPTURE_WIDTH, frame_from_bgra};

use crate::{
    ApplicationInfo, DesktopBackgroundFrame, DesktopSnapshot, DesktopSurface, DesktopVisualFrame,
    DisplayTopology, HostError, MonitorId, MonitorInfo, PhysicalDesktopPoint, PlatformBackend,
    PlatformCapabilities, PlatformKind, RectI,
};

pub(super) fn fallback_display_topology(revision: u64) -> Option<DisplayTopology> {
    let display = CGMainDisplayID();
    let pixel_width = CGDisplayPixelsWide(display);
    let pixel_height = CGDisplayPixelsHigh(display);
    let quartz_bounds = CGDisplayBounds(display);
    let point_width = quartz_bounds.size.width.max(1.0);
    let point_height = quartz_bounds.size.height.max(1.0);
    let physical_width = pixel_width.max(quartz_bounds.size.width.round().max(0.0) as usize);
    let physical_height = pixel_height.max(quartz_bounds.size.height.round().max(0.0) as usize);
    if physical_width == 0 || physical_height == 0 {
        return None;
    }
    let scale = (physical_width as f64 / point_width)
        .max(physical_height as f64 / point_height)
        .max(1.0);
    let minimum = PhysicalDesktopPoint {
        x: (quartz_bounds.origin.x * scale).round() as i32,
        y: (quartz_bounds.origin.y * scale).round() as i32,
    };
    let bounds = RectI {
        minimum,
        maximum: PhysicalDesktopPoint {
            x: minimum
                .x
                .saturating_add(physical_width.min(i32::MAX as usize) as i32),
            y: minimum
                .y
                .saturating_add(physical_height.min(i32::MAX as usize) as i32),
        },
    };
    Some(DisplayTopology::new(
        vec![MonitorInfo {
            id: MonitorId(format!("coregraphics-main-{display}")),
            physical_bounds: bounds,
            working_area: bounds,
            scale_factor: scale,
            primary: true,
        }],
        revision,
    ))
}

pub struct MacOsBackend {
    started: Instant,
    window: Option<Retained<NSWindow>>,
    window_geometry_available: bool,
    cursor_hittest_enabled: Option<bool>,
    screen_capture_available: bool,
    visual_worker: Option<VisualSampleWorker>,
    background_worker: Option<QuartzBackgroundWorker>,
}

impl Default for MacOsBackend {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            window: None,
            window_geometry_available: true,
            cursor_hittest_enabled: None,
            screen_capture_available: false,
            visual_worker: None,
            background_worker: None,
        }
    }
}

impl PlatformBackend for MacOsBackend {
    fn kind(&self) -> PlatformKind {
        PlatformKind::MacOs
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            global_cursor: true,
            user_idle_time: true,
            active_application: true,
            visible_window_geometry: self.window_geometry_available,
            transparent_overlay: true,
            dynamic_cursor_hittest: true,
            nonactivating_overlay: true,
            multiple_monitors: true,
            microphone: false,
            camera: false,
            screen_capture: self.screen_capture_available,
        }
    }

    fn initialize(&mut self, window: &Window) -> Result<(), HostError> {
        let handle = window
            .window_handle()
            .map_err(|error| HostError::WindowHandle(error.to_string()))?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err(HostError::WindowHandle("expected an AppKit NSView".into()));
        };
        let view: Retained<NSView> = unsafe { Retained::retain(handle.ns_view.as_ptr().cast()) }
            .ok_or_else(|| HostError::WindowHandle("could not retain NSView".into()))?;
        self.window = view.window();
        let Some(native) = self.window.as_ref() else {
            return Err(HostError::WindowHandle(
                "NSView is not attached to an NSWindow".into(),
            ));
        };
        native.setSharingType(NSWindowSharingType::None);
        self.cursor_hittest_enabled = Some(false);
        self.screen_capture_available = screen_capture_access();
        if self.screen_capture_available {
            self.visual_worker = Some(VisualSampleWorker::new(native.windowNumber() as u32));
            self.background_worker =
                Some(QuartzBackgroundWorker::new(native.windowNumber() as u32));
        }
        Ok(())
    }

    fn poll_desktop(&mut self, topology: &DisplayTopology) -> DesktopSnapshot {
        let cursor = CGEvent::new(None).map(|event| {
            let point = CGEvent::location(Some(&event));
            quartz_point_to_physical(topology, point.x, point.y)
        });
        let idle = CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::CombinedSessionState,
            CGEventType(u32::MAX),
        );
        let workspace = NSWorkspace::sharedWorkspace();
        let frontmost = workspace.frontmostApplication();
        let frontmost_pid = frontmost
            .as_ref()
            .map(|application| application.processIdentifier());
        let active_application = frontmost.map(|application| {
            let identifier = application
                .bundleIdentifier()
                .or_else(|| application.localizedName())
                .map(|name| name.to_string());
            let category = identifier
                .as_deref()
                .map(classify_application)
                .unwrap_or(AppCategory::Unknown);
            ApplicationInfo {
                category,
                transient_identifier: identifier,
            }
        });
        let quartz_windows = quartz_windows(topology);
        self.window_geometry_available = quartz_windows.is_some();
        let quartz_windows = quartz_windows.unwrap_or_default();
        let active_window = frontmost_pid.and_then(|pid| {
            quartz_windows
                .iter()
                .find(|window| window.owner_pid == pid)
                .map(|window| window.surface.bounds)
        });
        let visible_surfaces = quartz_windows
            .into_iter()
            .map(|window| window.surface)
            .collect();
        DesktopSnapshot {
            timestamp: self.started.elapsed().as_secs_f64(),
            topology_revision: topology.revision,
            cursor,
            primary_button_down: Some(CGEventSource::button_state(
                CGEventSourceStateID::CombinedSessionState,
                CGMouseButton::Left,
            )),
            idle_seconds: idle.is_finite().then_some(idle as f32),
            active_application,
            active_window,
            visible_surfaces,
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

    fn overlay_background_excludes_pet(&self) -> bool {
        true
    }

    fn capture_overlay_background(&mut self, window: &Window) -> Option<DesktopBackgroundFrame> {
        let native = self.window.as_ref()?;
        let frame = native.frame();
        // AppKit is bottom-up in points; Quartz is top-down in points. Using
        // the native frame avoids dividing a mixed-DPI desktop origin by the
        // current display scale, which would shift the crop between monitors.
        let main = CGDisplayBounds(CGMainDisplayID());
        let bounds = appkit_capture_bounds(
            [
                frame.origin.x,
                frame.origin.y,
                frame.size.width,
                frame.size.height,
            ],
            main.size.height,
        );
        self.background_worker
            .as_mut()?
            .submit_and_poll(window, bounds)
    }

    fn apply_overlay_policy(&mut self, _window: &Window) -> Result<(), HostError> {
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| HostError::WindowHandle("backend is not initialized".into()))?;
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setHidesOnDeactivate(false);
        window.setIgnoresMouseEvents(true);
        window.setSharingType(NSWindowSharingType::None);
        window.setLevel(NSFloatingWindowLevel);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        Ok(())
    }

    fn set_cursor_hittest(&mut self, window: &Window, enabled: bool) -> Result<(), HostError> {
        if self.cursor_hittest_enabled == Some(enabled) {
            return Ok(());
        }
        window.set_cursor_hittest(enabled).map_err(|error| {
            HostError::Platform(format!("could not update AppKit hit testing: {error}"))
        })?;
        let native = self
            .window
            .as_ref()
            .ok_or_else(|| HostError::WindowHandle("backend is not initialized".into()))?;
        native.setIgnoresMouseEvents(!enabled);
        self.cursor_hittest_enabled = Some(enabled);
        Ok(())
    }

    fn app_data_directory(&self) -> Result<PathBuf, HostError> {
        ProjectDirs::from("io", "lomatoq", "Pet 2")
            .map(|dirs| dirs.data_local_dir().to_path_buf())
            .ok_or(HostError::AppDataUnavailable)
    }

    fn shutdown(&mut self) {
        self.background_worker.take();
        if let Some(mut worker) = self.visual_worker.take() {
            worker.shutdown();
        }
    }
}

fn screen_capture_access() -> bool {
    if objc2_core_graphics::CGPreflightScreenCaptureAccess() {
        return true;
    }
    // macOS displays its standard one-time consent sheet. A denial remains a
    // supported reduced-capability mode; approval becomes active after restart.
    objc2_core_graphics::CGRequestScreenCaptureAccess()
        && objc2_core_graphics::CGPreflightScreenCaptureAccess()
}

fn appkit_capture_bounds(frame: [f64; 4], main_height: f64) -> [f64; 4] {
    [
        frame[0],
        main_height - frame[1] - frame[3],
        frame[2],
        frame[3],
    ]
}

#[derive(Clone, Copy)]
struct QuartzBackgroundRequest {
    bounds: [f64; 4],
    physical_rect: RectI,
    width: u32,
    height: u32,
}

/// One queued request and one queued frame keep captures bounded even while
/// the UI is suspended. Raw pixels are used only by the den renderer.
struct QuartzBackgroundWorker {
    request_tx: Option<SyncSender<QuartzBackgroundRequest>>,
    frame_rx: Receiver<DesktopBackgroundFrame>,
    join: Option<JoinHandle<()>>,
    last_submit: Instant,
}

impl QuartzBackgroundWorker {
    fn new(overlay_window: u32) -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel::<QuartzBackgroundRequest>(1);
        let (frame_tx, frame_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("pet2-quartz-background".into())
            .spawn(move || {
                let started = Instant::now();
                let mut sequence = 0_u64;
                while let Ok(request) = request_rx.recv() {
                    let bounds = CGRect {
                        origin: CGPoint {
                            x: request.bounds[0],
                            y: request.bounds[1],
                        },
                        size: CGSize {
                            width: request.bounds[2],
                            height: request.bounds[3],
                        },
                    };
                    let Some(bgra8) = capture_desktop_bgra(
                        bounds,
                        overlay_window,
                        request.width as usize,
                        request.height as usize,
                    ) else {
                        continue;
                    };
                    sequence = sequence.saturating_add(1);
                    let (mean_luminance, contrast) =
                        super::visual_sampling::background_luminance_stats(
                            &bgra8,
                            request.width,
                            request.height,
                            request.width * 4,
                        );
                    let frame = DesktopBackgroundFrame {
                        width: request.width,
                        height: request.height,
                        bytes_per_row: request.width * 4,
                        bgra8,
                        physical_rect: request.physical_rect,
                        sequence,
                        timestamp: started.elapsed().as_secs_f64(),
                        mean_luminance,
                        contrast,
                    };
                    if let Err(mpsc::TrySendError::Disconnected(_)) = frame_tx.try_send(frame) {
                        break;
                    }
                }
            })
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
        bounds: [f64; 4],
    ) -> Option<DesktopBackgroundFrame> {
        let latest = self.frame_rx.try_recv().ok();
        if self.last_submit.elapsed() >= Duration::from_secs_f64(1.0 / 30.0) {
            let position = window.outer_position().ok()?;
            let size = window.inner_size();
            if size.width > 0 && size.height > 0 {
                // Half physical resolution, capped proportionally at 1920×1080.
                let divisor = 2.0_f64
                    .max(size.width as f64 / 1920.0)
                    .max(size.height as f64 / 1080.0);
                let request = QuartzBackgroundRequest {
                    bounds,
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
                    width: (size.width as f64 / divisor).ceil() as u32,
                    height: (size.height as f64 / divisor).ceil() as u32,
                };
                if self
                    .request_tx
                    .as_ref()
                    .is_some_and(|tx| tx.try_send(request).is_ok())
                {
                    self.last_submit = Instant::now();
                }
            }
        }
        latest
    }
}

impl Drop for QuartzBackgroundWorker {
    fn drop(&mut self) {
        self.request_tx.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VisualSampleRequest {
    quartz_bounds: [f64; 4],
    pet_position: Vec2,
}

struct VisualSampleWorker {
    request_tx: Option<Sender<VisualSampleRequest>>,
    sample_rx: Receiver<DesktopVisualFrame>,
    join: Option<JoinHandle<()>>,
    latest: Option<DesktopVisualFrame>,
}

impl VisualSampleWorker {
    fn new(overlay_window: u32) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<VisualSampleRequest>();
        let (sample_tx, sample_rx) = mpsc::channel::<DesktopVisualFrame>();
        let join = thread::Builder::new()
            .name("pet2-visual-sample".into())
            .spawn(move || visual_sample_loop(overlay_window, &request_rx, &sample_tx))
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
        let quartz_bounds = quartz_virtual_bounds(topology)?;
        let request = VisualSampleRequest {
            quartz_bounds: [
                quartz_bounds.origin.x,
                quartz_bounds.origin.y,
                quartz_bounds.size.width,
                quartz_bounds.size.height,
            ],
            pet_position,
        };
        let _ = self.request_tx.as_ref().map(|sender| sender.send(request));
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
    overlay_window: u32,
    requests: &Receiver<VisualSampleRequest>,
    samples: &Sender<DesktopVisualFrame>,
) {
    let mut previous = Vec::with_capacity(64);
    let started = Instant::now();
    let mut sequence = 0_u64;
    while let Ok(mut request) = requests.recv() {
        for newer in requests.try_iter() {
            request = newer;
        }
        let bounds = CGRect {
            origin: CGPoint {
                x: request.quartz_bounds[0],
                y: request.quartz_bounds[1],
            },
            size: CGSize {
                width: request.quartz_bounds[2],
                height: request.quartz_bounds[3],
            },
        };
        let Some(pixels) =
            capture_desktop_bgra(bounds, overlay_window, CAPTURE_WIDTH, CAPTURE_HEIGHT)
        else {
            continue;
        };
        sequence = sequence.saturating_add(1);
        let Some(sample) = frame_from_bgra(
            &pixels,
            request.pet_position,
            &mut previous,
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

#[allow(deprecated)]
fn capture_desktop_bgra(
    bounds: CGRect,
    overlay_window: u32,
    width: usize,
    height: usize,
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
        return None;
    }
    // Capturing only windows below Pet excludes its transparent overlay before
    // sampling, matching the Windows self-mask without granting Pet access to
    // its own pixels.
    let image = objc2_core_graphics::CGWindowListCreateImage(
        bounds,
        CGWindowListOption::OptionOnScreenBelowWindow,
        overlay_window,
        CGWindowImageOption::BoundsIgnoreFraming | CGWindowImageOption::BestResolution,
    )?;
    let color_space = CGColorSpace::new_device_rgb()?;
    let bytes_per_row = width.checked_mul(4)?;
    let mut pixels = vec![0_u8; bytes_per_row.checked_mul(height)?];
    let bitmap_info =
        CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0;
    // SAFETY: `pixels` remains allocated and unmoved until the bitmap context
    // has been dropped below. Its row length and total capacity match exactly.
    let context = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast::<c_void>(),
            width,
            height,
            8,
            bytes_per_row,
            Some(&color_space),
            bitmap_info,
        )
    }?;
    CGContext::translate_ctm(Some(&context), 0.0, height as f64);
    CGContext::scale_ctm(Some(&context), 1.0, -1.0);
    CGContext::draw_image(
        Some(&context),
        CGRect {
            origin: CGPoint::ZERO,
            size: CGSize {
                width: width as f64,
                height: height as f64,
            },
        },
        Some(&image),
    );
    drop(context);
    Some(pixels)
}

fn quartz_virtual_bounds(topology: &DisplayTopology) -> Option<CGRect> {
    let mut minimum = [f64::INFINITY; 2];
    let mut maximum = [f64::NEG_INFINITY; 2];
    for monitor in &topology.monitors {
        let scale = monitor.scale_factor.max(0.25);
        minimum[0] = minimum[0].min(monitor.physical_bounds.minimum.x as f64 / scale);
        minimum[1] = minimum[1].min(monitor.physical_bounds.minimum.y as f64 / scale);
        maximum[0] = maximum[0].max(monitor.physical_bounds.maximum.x as f64 / scale);
        maximum[1] = maximum[1].max(monitor.physical_bounds.maximum.y as f64 / scale);
    }
    (minimum[0].is_finite()
        && minimum[1].is_finite()
        && maximum[0] > minimum[0]
        && maximum[1] > minimum[1])
        .then_some(CGRect {
            origin: CGPoint {
                x: minimum[0],
                y: minimum[1],
            },
            size: CGSize {
                width: maximum[0] - minimum[0],
                height: maximum[1] - minimum[1],
            },
        })
}

fn quartz_point_to_physical(topology: &DisplayTopology, x: f64, y: f64) -> PhysicalDesktopPoint {
    let monitor = topology
        .monitors
        .iter()
        .find(|monitor| {
            let scale = monitor.scale_factor.max(0.25);
            x >= monitor.physical_bounds.minimum.x as f64 / scale
                && x < monitor.physical_bounds.maximum.x as f64 / scale
                && y >= monitor.physical_bounds.minimum.y as f64 / scale
                && y < monitor.physical_bounds.maximum.y as f64 / scale
        })
        .or_else(|| topology.primary());
    let scale = monitor.map_or(1.0, |monitor| monitor.scale_factor.max(0.25));
    PhysicalDesktopPoint {
        x: (x * scale).round() as i32,
        y: (y * scale).round() as i32,
    }
}

fn quartz_rect_to_physical(topology: &DisplayTopology, rectangle: CGRect) -> RectI {
    RectI {
        minimum: quartz_point_to_physical(topology, rectangle.origin.x, rectangle.origin.y),
        maximum: quartz_point_to_physical(
            topology,
            rectangle.origin.x + rectangle.size.width,
            rectangle.origin.y + rectangle.size.height,
        ),
    }
}

struct QuartzWindow {
    owner_pid: i32,
    surface: DesktopSurface,
}

fn quartz_windows(topology: &DisplayTopology) -> Option<Vec<QuartzWindow>> {
    let array = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        kCGNullWindowID,
    )?;
    let dictionaries: &CFArray<CFDictionary> = unsafe { array.cast_unchecked() };
    let mut windows = Vec::with_capacity(dictionaries.len().min(128));
    for dictionary in dictionaries {
        if let Some(window) = quartz_window(&dictionary, topology) {
            windows.push(window);
        }
    }
    Some(windows)
}

fn quartz_window(dictionary: &CFDictionary, topology: &DisplayTopology) -> Option<QuartzWindow> {
    if dictionary_number(dictionary, unsafe { kCGWindowLayer })? != 0 {
        return None;
    }
    let window_number = dictionary_number(dictionary, unsafe { kCGWindowNumber })?;
    let owner_pid = dictionary_number(dictionary, unsafe { kCGWindowOwnerPID })? as i32;
    let typed: &CFDictionary<CFString, CFType> = unsafe { dictionary.cast_unchecked() };
    let bounds_value = typed.get(unsafe { kCGWindowBounds })?;
    let bounds_dictionary = bounds_value.downcast::<CFDictionary>().ok()?;
    let mut rectangle = CGRect::ZERO;
    if !unsafe { CGRectMakeWithDictionaryRepresentation(Some(&bounds_dictionary), &mut rectangle) }
    {
        return None;
    }
    let bounds = quartz_rect_to_physical(topology, rectangle);
    (bounds.width() >= 80 && bounds.height() >= 50).then(|| QuartzWindow {
        owner_pid,
        surface: DesktopSurface {
            transient_id: format!("window:{window_number}"),
            bounds,
        },
    })
}

fn dictionary_number(dictionary: &CFDictionary, key: &CFString) -> Option<i64> {
    let typed: &CFDictionary<CFString, CFType> = unsafe { dictionary.cast_unchecked() };
    typed.get(key)?.downcast::<CFNumber>().ok()?.as_i64()
}

fn classify_application(identifier: &str) -> AppCategory {
    let lower = identifier.to_lowercase();
    if ["xcode", "code", "jetbrains", "office", "notion", "obsidian"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        AppCategory::FocusedWork
    } else if ["adobe", "blender", "figma", "krita", "resolve"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        AppCategory::Creative
    } else if ["slack", "discord", "teams", "telegram", "signal"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        AppCategory::Communication
    } else if ["steam", "spotify", "vlc", "netflix"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        AppCategory::Entertainment
    } else if lower.starts_with("com.apple.") {
        AppCategory::System
    } else {
        AppCategory::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_crop_uses_points_and_flips_appkit_y() {
        // A Retina backing store is twice this size, but Quartz takes points.
        assert_eq!(
            appkit_capture_bounds([100.0, 200.0, 800.0, 600.0], 900.0),
            [100.0, 100.0, 800.0, 600.0]
        );
    }

    #[test]
    fn background_crop_preserves_displays_left_of_and_above_main() {
        assert_eq!(
            appkit_capture_bounds([-1280.0, 900.0, 1280.0, 720.0], 900.0),
            [-1280.0, -720.0, 1280.0, 720.0]
        );
        assert_eq!(
            appkit_capture_bounds([0.0, -600.0, 800.0, 600.0], 900.0),
            [0.0, 900.0, 800.0, 600.0]
        );
    }
}

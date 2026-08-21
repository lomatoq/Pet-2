from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    if new in text:
        return
    if old not in text:
        raise RuntimeError(f"source anchor not found in {path}: {old[:80]!r}")
    write(path, text.replace(old, new, 1))


def insert_before(path: str, anchor: str, block: str, sentinel: str) -> None:
    text = read(path)
    if sentinel in text:
        return
    if anchor not in text:
        raise RuntimeError(f"insertion anchor not found in {path}: {anchor[:80]!r}")
    write(path, text.replace(anchor, block + anchor, 1))


def main() -> None:
    replace_once(
        "crates/desktop_host/Cargo.toml",
        '    "Win32_Graphics_Dwm",\n',
        '    "Win32_Graphics_Dwm",\n    "Win32_Graphics_Gdi",\n',
    )

    actions = "crates/lifecore/src/actions.rs"
    replace_once(
        actions,
        "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub struct SensorFrame {",
        "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n#[serde(default)]\npub struct SensorFrame {",
    )
    replace_once(
        actions,
        "    pub audio_rms: Option<f32>,\n    pub voice_activity: Option<f32>,\n",
        "    pub audio_rms: Option<f32>,\n"
        "    pub voice_activity: Option<f32>,\n"
        "    pub mean_luminance: Option<f32>,\n"
        "    pub local_luminance: Option<f32>,\n",
    )
    replace_once(
        actions,
        "            audio_rms: None,\n            voice_activity: None,\n",
        "            audio_rms: None,\n"
        "            voice_activity: None,\n"
        "            mean_luminance: None,\n"
        "            local_luminance: None,\n",
    )

    host_sensors = "crates/desktop_host/src/sensors.rs"
    visual_struct = '''\n#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DesktopVisualSample {
    pub mean_luminance: f32,
    pub local_luminance: f32,
    pub contrast: f32,
    pub colorfulness: f32,
    pub warmth: f32,
    pub dominant_hue: f32,
    pub motion_energy: f32,
    pub edge_density: f32,
    pub sudden_change: f32,
}

impl DesktopVisualSample {
    #[must_use]
    pub fn is_finite(self) -> bool {
        [
            self.mean_luminance,
            self.local_luminance,
            self.contrast,
            self.colorfulness,
            self.warmth,
            self.dominant_hue,
            self.motion_energy,
            self.edge_density,
            self.sudden_change,
        ]
        .into_iter()
        .all(f32::is_finite)
    }
}

'''
    insert_before(
        host_sensors,
        "impl DesktopSnapshot {",
        visual_struct,
        "pub struct DesktopVisualSample",
    )
    replace_once(
        host_sensors,
        "            audio_rms: None,\n            voice_activity: None,\n",
        "            audio_rms: None,\n"
        "            voice_activity: None,\n"
        "            mean_luminance: None,\n"
        "            local_luminance: None,\n",
    )

    contract = "crates/desktop_host/src/contract.rs"
    replace_once(contract, "use thiserror::Error;\n", "use glam::Vec2;\nuse thiserror::Error;\n")
    replace_once(
        contract,
        "use crate::{DesktopSnapshot, DisplayTopology, PlatformCapabilities};",
        "use crate::{DesktopSnapshot, DesktopVisualSample, DisplayTopology, PlatformCapabilities};",
    )
    replace_once(
        contract,
        "    fn poll_desktop(&mut self, topology: &DisplayTopology) -> DesktopSnapshot;\n",
        "    fn poll_desktop(&mut self, topology: &DisplayTopology) -> DesktopSnapshot;\n"
        "    fn poll_visual_features(\n"
        "        &mut self,\n"
        "        _topology: &DisplayTopology,\n"
        "        _pet_position: Vec2,\n"
        "    ) -> Option<DesktopVisualSample> {\n"
        "        None\n"
        "    }\n",
    )

    windows = "crates/desktop_host/src/platform/windows.rs"
    replace_once(windows, "use directories::ProjectDirs;\n", "use directories::ProjectDirs;\nuse glam::Vec2;\n")
    replace_once(
        windows,
        "    Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},\n",
        "    Graphics::{\n"
        "        Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},\n"
        "        Gdi::{GetDC, GetPixel, HDC, ReleaseDC},\n"
        "    },\n",
    )
    replace_once(
        windows,
        "    ApplicationInfo, DesktopSnapshot, DesktopSurface, DisplayTopology, HostError,\n"
        "    PhysicalDesktopPoint, PlatformBackend, PlatformCapabilities, PlatformKind, RectI,\n",
        "    ApplicationInfo, DesktopSnapshot, DesktopSurface, DesktopVisualSample, DisplayTopology,\n"
        "    HostError, PhysicalDesktopPoint, PlatformBackend, PlatformCapabilities, PlatformKind,\n"
        "    RectI,\n",
    )
    replace_once(
        windows,
        "pub struct WindowsBackend {\n    started: Instant,\n    overlay_hwnd: Option<HWND>,\n}",
        "pub struct WindowsBackend {\n"
        "    started: Instant,\n"
        "    overlay_hwnd: Option<HWND>,\n"
        "    previous_visual: Vec<[f32; 3]>,\n"
        "}",
    )
    replace_once(
        windows,
        "            started: Instant::now(),\n            overlay_hwnd: None,\n",
        "            started: Instant::now(),\n"
        "            overlay_hwnd: None,\n"
        "            previous_visual: Vec::with_capacity(64),\n",
    )
    replace_once(windows, "            screen_capture: false,\n", "            screen_capture: true,\n")
    visual_method = '''
    fn poll_visual_features(
        &mut self,
        topology: &DisplayTopology,
        pet_position: Vec2,
    ) -> Option<DesktopVisualSample> {
        sample_desktop_visual(topology, pet_position, &mut self.previous_visual)
    }

'''
    insert_before(
        windows,
        "    fn apply_overlay_policy(&mut self, _window: &Window) -> Result<(), HostError> {",
        visual_method,
        "fn poll_visual_features(\n        &mut self,\n        topology: &DisplayTopology",
    )
    visual_helpers = r'''fn sample_desktop_visual(
    topology: &DisplayTopology,
    pet_position: Vec2,
    previous: &mut Vec<[f32; 3]>,
) -> Option<DesktopVisualSample> {
    let virtual_bounds = topology.virtual_physical_bounds;
    if !virtual_bounds.is_valid() {
        return None;
    }
    let foreground = unsafe { GetForegroundWindow() };
    let global_bounds = window_bounds(foreground)
        .and_then(|bounds| intersect_rect(bounds, virtual_bounds))
        .unwrap_or(virtual_bounds);
    let pet_center = PhysicalDesktopPoint {
        x: virtual_bounds.minimum.x
            + (pet_position.x.clamp(0.0, 1.0) * virtual_bounds.width() as f32).round() as i32,
        y: virtual_bounds.minimum.y
            + (pet_position.y.clamp(0.0, 1.0) * virtual_bounds.height() as f32).round() as i32,
    };
    let local_bounds = intersect_rect(
        RectI {
            minimum: PhysicalDesktopPoint {
                x: pet_center.x - 96,
                y: pet_center.y - 96,
            },
            maximum: PhysicalDesktopPoint {
                x: pet_center.x + 97,
                y: pet_center.y + 97,
            },
        },
        virtual_bounds,
    )
    .unwrap_or(global_bounds);

    let device_context = unsafe { GetDC(std::ptr::null_mut()) };
    if device_context.is_null() {
        return None;
    }
    let mut samples = Vec::with_capacity(60);
    sample_grid(device_context, global_bounds, 7, 5, &mut samples);
    let global_count = samples.len();
    sample_grid(device_context, local_bounds, 5, 5, &mut samples);
    unsafe {
        ReleaseDC(std::ptr::null_mut(), device_context);
    }
    if global_count == 0 || samples.len() == global_count {
        return None;
    }

    let (mean_luminance, contrast, colorfulness, warmth, dominant_hue, edge_density) =
        summarize_samples(&samples[..global_count]);
    let local_luminance = samples[global_count..]
        .iter()
        .map(|sample| luminance(*sample))
        .sum::<f32>()
        / (samples.len() - global_count) as f32;
    let motion_energy = if previous.len() == samples.len() {
        samples
            .iter()
            .zip(previous.iter())
            .map(|(current, old)| {
                ((current[0] - old[0]).abs()
                    + (current[1] - old[1]).abs()
                    + (current[2] - old[2]).abs())
                    / 3.0
            })
            .sum::<f32>()
            / samples.len() as f32
    } else {
        0.0
    };
    let previous_mean = if previous.is_empty() {
        mean_luminance
    } else {
        previous.iter().map(|sample| luminance(*sample)).sum::<f32>() / previous.len() as f32
    };
    let sudden_change = ((mean_luminance - previous_mean).abs() * 1.8
        + motion_energy * 1.25)
        .clamp(0.0, 1.0);
    *previous = samples;

    let sample = DesktopVisualSample {
        mean_luminance,
        local_luminance,
        contrast,
        colorfulness,
        warmth,
        dominant_hue,
        motion_energy: (motion_energy * 2.6).clamp(0.0, 1.0),
        edge_density,
        sudden_change,
    };
    sample.is_finite().then_some(sample)
}

fn sample_grid(
    device_context: HDC,
    bounds: RectI,
    columns: i32,
    rows: i32,
    output: &mut Vec<[f32; 3]>,
) {
    if !bounds.is_valid() || columns <= 0 || rows <= 0 {
        return;
    }
    for row in 0..rows {
        for column in 0..columns {
            let x = bounds.minimum.x
                + (((column as f32 + 0.5) / columns as f32) * bounds.width() as f32).round()
                    as i32;
            let y = bounds.minimum.y
                + (((row as f32 + 0.5) / rows as f32) * bounds.height() as f32).round() as i32;
            let color = unsafe { GetPixel(device_context, x, y) };
            if color == u32::MAX {
                continue;
            }
            output.push([
                (color & 0xff) as f32 / 255.0,
                ((color >> 8) & 0xff) as f32 / 255.0,
                ((color >> 16) & 0xff) as f32 / 255.0,
            ]);
        }
    }
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
            sample
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max)
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
        hue_vector.y.atan2(hue_vector.x).rem_euclid(std::f32::consts::TAU)
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

fn intersect_rect(left: RectI, right: RectI) -> Option<RectI> {
    let intersection = RectI {
        minimum: PhysicalDesktopPoint {
            x: left.minimum.x.max(right.minimum.x),
            y: left.minimum.y.max(right.minimum.y),
        },
        maximum: PhysicalDesktopPoint {
            x: left.maximum.x.min(right.maximum.x),
            y: left.maximum.y.min(right.maximum.y),
        },
    };
    intersection.is_valid().then_some(intersection)
}

'''
    insert_before(
        windows,
        "struct EnumerationContext {",
        visual_helpers,
        "fn sample_desktop_visual(",
    )

    embodiment = "crates/pet_body/src/embodiment.rs"
    replace_once(
        embodiment,
        "        let luminance = 0.5;\n",
        "        let luminance = sensors\n"
        "            .local_luminance\n"
        "            .or(sensors.mean_luminance)\n"
        "            .unwrap_or(0.5);\n",
    )

    vita_runtime = "app/src/vita_runtime.rs"
    replace_once(
        vita_runtime,
        "use lifecore::{\n",
        "use desktop_host::DesktopVisualSample;\nuse lifecore::{\n",
    )
    replace_once(
        vita_runtime,
        "use pet_perception::PerceptionRuntime;\n",
        "use pet_perception::{PerceptionRuntime, VisualFeatureFrame};\n",
    )
    visual_bridge = '''
    pub fn set_visual_features(&mut self, sample: DesktopVisualSample) {
        self.perception.set_visual_features(VisualFeatureFrame {
            mean_luminance: sample.mean_luminance,
            local_luminance: sample.local_luminance,
            contrast: sample.contrast,
            colorfulness: sample.colorfulness,
            warmth: sample.warmth,
            dominant_hue: sample.dominant_hue,
            motion_energy: sample.motion_energy,
            edge_density: sample.edge_density,
            sudden_change: sample.sudden_change,
        });
    }

'''
    insert_before(
        vita_runtime,
        "    #[must_use]\n    pub fn snapshot(&self) -> VitaState {",
        visual_bridge,
        "pub fn set_visual_features(&mut self, sample: DesktopVisualSample)",
    )

    app = "app/src/main.rs"
    replace_once(
        app,
        "    sensor_accumulator: f32,\n    save_accumulator: f32,\n",
        "    sensor_accumulator: f32,\n"
        "    visual_accumulator: f32,\n"
        "    save_accumulator: f32,\n",
    )
    replace_once(
        app,
        "        runtime.sensor_accumulator += elapsed;\n        runtime.save_accumulator += elapsed;\n",
        "        runtime.sensor_accumulator += elapsed;\n"
        "        runtime.visual_accumulator += elapsed;\n"
        "        runtime.save_accumulator += elapsed;\n",
    )
    visual_poll = '''            if runtime.visual_accumulator >= 0.2 {
                runtime.visual_accumulator %= 0.2;
                if let Some(sample) = runtime.platform.poll_visual_features(
                    &runtime.topology,
                    runtime.body.simulation.feedback.world_position,
                ) {
                    runtime.sensors.mean_luminance = Some(sample.mean_luminance);
                    runtime.sensors.local_luminance = Some(sample.local_luminance);
                    runtime.vita.set_visual_features(sample);
                }
            }
'''
    insert_before(
        app,
        "            runtime.vita.observe(\n",
        visual_poll,
        "if runtime.visual_accumulator >= 0.2",
    )
    replace_once(
        app,
        "            sensor_accumulator: 1.0,\n            save_accumulator: 0.0,\n",
        "            sensor_accumulator: 1.0,\n"
        "            visual_accumulator: 0.2,\n"
        "            save_accumulator: 0.0,\n",
    )


if __name__ == "__main__":
    main()

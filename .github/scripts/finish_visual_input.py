from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_first(path: str, variants: list[str], new: str) -> None:
    text = read(path)
    if new in text:
        return
    for old in variants:
        if old in text:
            write(path, text.replace(old, new, 1))
            return
    raise RuntimeError(f"no source variant found in {path}")


def insert_before(path: str, anchor: str, block: str, sentinel: str) -> None:
    text = read(path)
    if sentinel in text:
        return
    if anchor not in text:
        raise RuntimeError(f"source anchor not found in {path}")
    write(path, text.replace(anchor, block + anchor, 1))


def main() -> None:
    embodiment = "crates/pet_body/src/embodiment.rs"
    replace_first(
        embodiment,
        ["        let luminance = 0.5;\n", "        let luminance: f32 = 0.5;\n"],
        "        let luminance = sensors\n"
        "            .local_luminance\n"
        "            .or(sensors.mean_luminance)\n"
        "            .unwrap_or(0.5);\n",
    )

    vita_runtime = "app/src/vita_runtime.rs"
    replace_first(
        vita_runtime,
        ["use lifecore::{\n"],
        "use desktop_host::DesktopVisualSample;\nuse lifecore::{\n",
    )
    replace_first(
        vita_runtime,
        ["use pet_perception::PerceptionRuntime;\n"],
        "use pet_perception::{PerceptionRuntime, VisualFeatureFrame};\n",
    )
    insert_before(
        vita_runtime,
        "    #[must_use]\n    pub fn snapshot(&self) -> VitaState {",
        '''    pub fn set_visual_features(&mut self, sample: DesktopVisualSample) {
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

''',
        "pub fn set_visual_features(&mut self, sample: DesktopVisualSample)",
    )

    app = "app/src/main.rs"
    replace_first(
        app,
        ["    sensor_accumulator: f32,\n    save_accumulator: f32,\n"],
        "    sensor_accumulator: f32,\n"
        "    visual_accumulator: f32,\n"
        "    save_accumulator: f32,\n",
    )
    replace_first(
        app,
        ["        runtime.sensor_accumulator += elapsed;\n        runtime.save_accumulator += elapsed;\n"],
        "        runtime.sensor_accumulator += elapsed;\n"
        "        runtime.visual_accumulator += elapsed;\n"
        "        runtime.save_accumulator += elapsed;\n",
    )
    insert_before(
        app,
        "            runtime.vita.observe(\n",
        '''            if runtime.visual_accumulator >= 0.2 {
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
''',
        "if runtime.visual_accumulator >= 0.2",
    )
    replace_first(
        app,
        ["            sensor_accumulator: 1.0,\n            save_accumulator: 0.0,\n"],
        "            sensor_accumulator: 1.0,\n"
        "            visual_accumulator: 0.2,\n"
        "            save_accumulator: 0.0,\n",
    )


if __name__ == "__main__":
    main()

from pathlib import Path


def replace_if_present(path: str, old: str, new: str) -> bool:
    file_path = Path(path)
    text = file_path.read_text(encoding="utf-8")
    if old not in text:
        return False
    file_path.write_text(text.replace(old, new, 1), encoding="utf-8")
    return True


def require_final(path: str, final: str) -> None:
    if final not in Path(path).read_text(encoding="utf-8"):
        raise RuntimeError(f"expected final source form not found in {path}")


def require_missing(path: str, obsolete: str) -> None:
    if obsolete in Path(path).read_text(encoding="utf-8"):
        raise RuntimeError(f"obsolete source form still present in {path}")


def main() -> None:
    perception_path = "crates/pet_perception/src/lib.rs"
    perception_final = (
        "        let sensors = SensorFrame {\n"
        "            timestamp: 1.2,\n"
        "            ..SensorFrame::default()\n"
        "        };"
    )
    replace_if_present(
        perception_path,
        "        let mut sensors = SensorFrame::default();\n"
        "        sensors.timestamp = 1.2;",
        perception_final,
    )
    replace_if_present(
        perception_path,
        "        let mut sensors = SensorFrame {\n"
        "            timestamp: 1.2,\n"
        "            ..SensorFrame::default()\n"
        "        };",
        perception_final,
    )
    require_final(perception_path, perception_final)

    embodiment_path = "crates/pet_body/src/embodiment.rs"
    gaze_final = (
        "#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]\n"
        "#[serde(rename_all = \"snake_case\")]\n"
        "pub enum GazeMode {\n"
        "    #[default]\n"
        "    TrackWorldTarget,"
    )
    replace_if_present(
        embodiment_path,
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]\n"
        "#[serde(rename_all = \"snake_case\")]\n"
        "pub enum GazeMode {\n"
        "    TrackWorldTarget,",
        gaze_final,
    )
    replace_if_present(
        embodiment_path,
        "\nimpl Default for GazeMode {\n"
        "    fn default() -> Self {\n"
        "        Self::TrackWorldTarget\n"
        "    }\n"
        "}\n",
        "\n",
    )
    require_final(embodiment_path, gaze_final)

    feedback_final = (
        "        let feedback = BodyFeedback {\n"
        "            velocity: Vec2::new(0.8, -0.2),\n"
        "            acceleration: Vec2::new(0.4, 0.1),\n"
        "            ..BodyFeedback::default()\n"
        "        };"
    )
    replace_if_present(
        embodiment_path,
        "        let mut feedback = BodyFeedback::default();\n"
        "        feedback.velocity = Vec2::new(0.8, -0.2);\n"
        "        feedback.acceleration = Vec2::new(0.4, 0.1);",
        feedback_final,
    )
    require_final(embodiment_path, feedback_final)

    mesh_path = "crates/pet_body/src/mesh.rs"
    replace_if_present(
        mesh_path,
        "        for triangle in self.indices.chunks_exact(3) {",
        "        for triangle in self.indices.as_chunks::<3>().0 {",
    )
    require_final(mesh_path, "        for triangle in self.indices.as_chunks::<3>().0 {")

    vita_runtime_path = "app/src/vita_runtime.rs"
    replace_if_present(
        vita_runtime_path,
        "use pet_perception::{PerceptionRuntime, VisualFeatureFrame};",
        "use pet_perception::PerceptionRuntime;",
    )
    replace_if_present(vita_runtime_path, "    last_output: Option<VitaOutput>,\n", "")
    replace_if_present(vita_runtime_path, "            last_output: None,\n", "")
    replace_if_present(
        vita_runtime_path,
        "        let output = self\n"
        "            .mind\n"
        "            .tick(&self.percept, sensors, life, body, base_intent, dt);\n"
        "        self.last_output = Some(output.clone());\n"
        "        output",
        "        self.mind\n"
        "            .tick(&self.percept, sensors, life, body, base_intent, dt)",
    )
    replace_if_present(
        vita_runtime_path,
        "\n    pub fn set_visual_features(&mut self, frame: VisualFeatureFrame) {\n"
        "        self.perception.set_visual_features(frame);\n"
        "    }\n",
        "",
    )
    replace_if_present(
        vita_runtime_path,
        "\n    #[must_use]\n"
        "    pub fn last_output(&self) -> Option<&VitaOutput> {\n"
        "        self.last_output.as_ref()\n"
        "    }\n",
        "",
    )
    require_missing(vita_runtime_path, "VisualFeatureFrame")
    require_missing(vita_runtime_path, "last_output")


if __name__ == "__main__":
    main()

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


if __name__ == "__main__":
    main()

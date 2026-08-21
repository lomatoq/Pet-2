from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file_path = Path(path)
    text = file_path.read_text(encoding="utf-8")
    if new in text:
        return
    if old not in text:
        raise RuntimeError(f"expected source anchor not found in {path}")
    file_path.write_text(text.replace(old, new, 1), encoding="utf-8")


def main() -> None:
    replace_once(
        "crates/pet_perception/src/lib.rs",
        "        let mut sensors = SensorFrame::default();\n        sensors.timestamp = 1.2;",
        "        let mut sensors = SensorFrame {\n            timestamp: 1.2,\n            ..SensorFrame::default()\n        };",
    )


if __name__ == "__main__":
    main()

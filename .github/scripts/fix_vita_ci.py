from pathlib import Path


def replace_first_available(path: str, replacements: list[tuple[str, str]], final: str) -> None:
    file_path = Path(path)
    text = file_path.read_text(encoding="utf-8")
    if final in text:
        return
    for old, new in replacements:
        if old in text:
            file_path.write_text(text.replace(old, new, 1), encoding="utf-8")
            return
    raise RuntimeError(f"expected source anchor not found in {path}")


def main() -> None:
    final = (
        "        let sensors = SensorFrame {\n"
        "            timestamp: 1.2,\n"
        "            ..SensorFrame::default()\n"
        "        };"
    )
    replace_first_available(
        "crates/pet_perception/src/lib.rs",
        [
            (
                "        let mut sensors = SensorFrame::default();\n"
                "        sensors.timestamp = 1.2;",
                final,
            ),
            (
                "        let mut sensors = SensorFrame {\n"
                "            timestamp: 1.2,\n"
                "            ..SensorFrame::default()\n"
                "        };",
                final,
            ),
        ],
        final,
    )


if __name__ == "__main__":
    main()

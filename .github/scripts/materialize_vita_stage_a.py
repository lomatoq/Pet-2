#!/usr/bin/env python3
from __future__ import annotations

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if new in text:
        return
    if old not in text:
        raise RuntimeError(f"expected patch anchor was not found in {path}: {old!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def main() -> None:
    subprocess.run(
        ["python", str(ROOT / ".github/scripts/materialize_vita_payload.py")],
        cwd=ROOT,
        check=True,
    )
    subprocess.run(
        ["python", str(ROOT / ".github/scripts/materialize_perception_payload.py")],
        cwd=ROOT,
        check=True,
    )

    perception_cargo = ROOT / "crates/pet_perception/Cargo.toml"
    perception_cargo.parent.mkdir(parents=True, exist_ok=True)
    perception_cargo.write_text(
        """[package]
name = "pet_perception"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
glam.workspace = true
lifecore = { path = "../lifecore" }
serde.workspace = true

[dev-dependencies]
serde_json.workspace = true
""",
        encoding="utf-8",
    )

    vita = ROOT / "crates/lifecore/src/vita.rs"
    replace_once(
        vita,
        """#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = \"snake_case\")]
pub enum InfluenceMode {
    Off,
    Gentle,
    Playful,
    Experimental,
}

impl Default for InfluenceMode {
    fn default() -> Self {
        Self::Playful
    }
}
""",
        """#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = \"snake_case\")]
pub enum InfluenceMode {
    Off,
    Gentle,
    #[default]
    Playful,
    Experimental,
}
""",
    )

    replace_once(
        ROOT / "Cargo.toml",
        'members = ["app", "crates/desktop_host", "crates/lifecore", "crates/pet_audio", "crates/pet_body"]',
        'members = ["app", "crates/desktop_host", "crates/lifecore", "crates/pet_audio", "crates/pet_body", "crates/pet_perception"]',
    )
    replace_once(
        ROOT / "crates/lifecore/src/lib.rs",
        "mod persistence;\n",
        "mod persistence;\nmod vita;\n",
    )
    replace_once(
        ROOT / "crates/lifecore/src/lib.rs",
        "pub use persistence::*;\n",
        "pub use persistence::*;\npub use vita::*;\n",
    )


if __name__ == "__main__":
    main()

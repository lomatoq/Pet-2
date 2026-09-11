from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if old in text:
        target.write_text(text.replace(old, new, 1), encoding="utf-8")


# Fix a latent test-name typo that only became visible once the companion module
# was exported into the production crate graph.
replace_once(
    "crates/lifecore/src/companion.rs",
    "fn preferences_require repeated evidence_to_become_confident()",
    "fn preferences_require_repeated_evidence_to_become_confident()",
)

# Keep the newly exported social-memory implementation clean under the project's
# strict `clippy -D warnings` gate.
replace_once(
    "crates/lifecore/src/companion.rs",
    """        if self.preferences.len() >= MAX_COMPANION_PREFERENCES {\n            if let Some((index, _)) = self\n                .preferences\n                .iter()\n                .enumerate()\n                .min_by(|left, right| left.1.1.confidence.total_cmp(&right.1.1.confidence))\n            {\n                self.preferences.remove(index);\n            }\n        }\n""",
    """        if self.preferences.len() >= MAX_COMPANION_PREFERENCES\n            && let Some((index, _)) = self\n                .preferences\n                .iter()\n                .enumerate()\n                .min_by(|left, right| left.1.1.confidence.total_cmp(&right.1.1.confidence))\n        {\n            self.preferences.remove(index);\n        }\n""",
)

print("R14 preflight fixes applied")

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

print("R14 preflight fixes applied")

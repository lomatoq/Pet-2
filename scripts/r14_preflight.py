from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if old in text:
        target.write_text(text.replace(old, new, 1), encoding="utf-8")


def replace_all(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if old in text:
        target.write_text(text.replace(old, new), encoding="utf-8")


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

# Pet Body already had `embodiment::GazeMode`. R14 adds a richer companion gaze
# controller with a different enum. Never rely on glob re-export resolution here:
# use an explicit sibling-module alias so the expression director cannot bind to
# the legacy embodiment gaze enum by accident.
expression_path = "crates/pet_body/src/companion_expression_director.rs"
replace_once(
    expression_path,
    "use crate::{BlinkController, BlinkOwner, BlinkRequest, GazeController, GazeMode, GazePlan};",
    "use crate::gaze_controller::GazeMode as CompanionGazeMode;\nuse crate::{BlinkController, BlinkOwner, BlinkRequest, GazeController, GazePlan};",
)
replace_once(
    expression_path,
    "fn gaze_mode_for(intent: PrimaryIntent, uncertainty: f32) -> GazeMode {",
    "fn gaze_mode_for(intent: PrimaryIntent, uncertainty: f32) -> CompanionGazeMode {",
)
replace_all(expression_path, "GazeMode::", "CompanionGazeMode::")

# Quiet-vocal admission must account for sounds that were ACTUALLY heard by the
# callback, not commands merely accepted by the host queue. Otherwise a device
# rejection burns the eight-second social cooldown even though the pet emitted
# nothing, and LifeCore's delivery-credit contract becomes false.
main_path = "app/src/main.rs"
replace_once(
    main_path,
    """                AudioWorkerEvent::RequestHeard { request_id } => {\n                    if self.remove_pending_request(request_id) {\n                        self.heard_requests.push_back(request_id);\n                        self.accepted_requests = self.accepted_requests.saturating_add(1);\n                        self.last_error = None;\n                    }\n                }\n""",
    """                AudioWorkerEvent::RequestHeard { request_id } => {\n                    if self.remove_pending_request(request_id) {\n                        let now = Instant::now();\n                        self.recent_voice_accepts.push_back(now);\n                        // Any heard vocal, including a safety vocal, buys the pet\n                        // a quiet refractory period before ordinary chatter.\n                        self.last_nonurgent_voice = Some(now);\n                        self.heard_requests.push_back(request_id);\n                        self.accepted_requests = self.accepted_requests.saturating_add(1);\n                        self.last_error = None;\n                    }\n                }\n""",
)
replace_once(
    main_path,
    """        if !urgent {\n            if self.recent_voice_accepts.len() >= 6\n                || self\n                    .last_nonurgent_voice\n                    .is_some_and(|instant| now.duration_since(instant) < Duration::from_secs(8))\n            {\n                return false;\n            }\n        }\n""",
    """        if !urgent {\n            if !self.pending_requests.is_empty()\n                || self.recent_voice_accepts.len() >= 6\n                || self\n                    .last_nonurgent_voice\n                    .is_some_and(|instant| now.duration_since(instant) < Duration::from_secs(8))\n            {\n                return false;\n            }\n        }\n""",
)
replace_once(
    main_path,
    """            Ok(()) => {\n                self.pending_requests.push_back(request.performance_seed);\n                self.last_error = None;\n                self.recent_voice_accepts.push_back(now);\n                if !urgent {\n                    self.last_nonurgent_voice = Some(now);\n                }\n                true\n            }\n""",
    """            Ok(()) => {\n                self.pending_requests.push_back(request.performance_seed);\n                self.last_error = None;\n                true\n            }\n""",
)

print("R14 preflight fixes applied")

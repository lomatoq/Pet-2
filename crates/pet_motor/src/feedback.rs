use crate::{CompletionReason, SomaticPerformanceFeedback};

impl SomaticPerformanceFeedback {
    pub fn sanitize(&mut self) {
        self.phase_progress = unit(self.phase_progress);
        self.contact_fraction = unit(self.contact_fraction);
        self.support_stability = unit(self.support_stability);
        self.supported_seconds = finite(self.supported_seconds).clamp(0.0, 86_400.0);
        self.local_deformation_energy = unit(self.local_deformation_energy);
        self.locality_fraction = unit(self.locality_fraction);
        self.maximum_strain = unit(self.maximum_strain);
        self.mass_conservation_error = unit(self.mass_conservation_error);
        self.motor_error = unit(self.motor_error);
        self.user_response_credit = unit(self.user_response_credit);
        if !self.supported {
            self.supported_seconds = 0.0;
        }
        if self.completion_reason == CompletionReason::SupportConfirmed && !self.supported {
            self.completion_reason = CompletionReason::None;
        }
    }
}

fn unit(value: f32) -> f32 {
    finite(value).clamp(0.0, 1.0)
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

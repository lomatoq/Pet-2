use lifecore::BodyVoiceFrame;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NonPhonatedKind {
    #[default]
    SniffSingle,
    SniffPair,
    SoftHuff,
    ContentExhale,
    StartleInhale,
    EffortExhale,
    SleepBreath,
    ShakeOffBreath,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NonPhonatedRequest {
    pub kind: NonPhonatedKind,
    pub intensity: f32,
    pub pan: f32,
    pub seed: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NonPhonatedFrame {
    pub airflow: f32,
    pub turbulence: f32,
    pub nasal: f32,
    pub body_resonance: f32,
    pub mouth_open: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NonPhonatedGesture {
    request: NonPhonatedRequest,
    elapsed: f32,
    duration: f32,
}

impl NonPhonatedGesture {
    #[must_use]
    pub fn new(mut request: NonPhonatedRequest) -> Self {
        request.intensity = finite(request.intensity, 0.0).clamp(0.0, 1.0);
        request.pan = finite(request.pan, 0.0).clamp(-1.0, 1.0);
        let duration = match request.kind {
            NonPhonatedKind::SniffSingle => 0.16,
            NonPhonatedKind::SniffPair => 0.42,
            NonPhonatedKind::SoftHuff => 0.22,
            NonPhonatedKind::ContentExhale => 0.55,
            NonPhonatedKind::StartleInhale => 0.13,
            NonPhonatedKind::EffortExhale => 0.28,
            NonPhonatedKind::SleepBreath => 1.20,
            NonPhonatedKind::ShakeOffBreath => 0.34,
        };
        Self {
            request,
            elapsed: 0.0,
            duration,
        }
    }

    #[must_use]
    pub fn finished(self) -> bool {
        self.elapsed >= self.duration
    }

    #[must_use]
    pub fn tick(&mut self, body: BodyVoiceFrame, dt: f32) -> NonPhonatedFrame {
        let dt = finite(dt, 0.0).clamp(0.0, 0.1);
        self.elapsed += dt;
        let t = (self.elapsed / self.duration.max(0.01)).clamp(0.0, 1.0);
        let intensity = self.request.intensity;
        let body = body.sanitized();
        let body_load =
            (body.slosh_energy * 0.25 + body.bond_strain * 0.35 + body.material_stress * 0.40)
                .clamp(0.0, 1.0);
        let (airflow, turbulence, nasal, resonance, mouth) = match self.request.kind {
            NonPhonatedKind::SniffSingle => {
                let pulse = gaussian(t, 0.46, 0.18);
                (pulse * 0.52, pulse * 0.42, pulse * 0.92, pulse * 0.16, 0.04)
            }
            NonPhonatedKind::SniffPair => {
                let pulse = gaussian(t, 0.28, 0.10) + gaussian(t, 0.68, 0.12) * 0.86;
                (pulse * 0.48, pulse * 0.40, pulse * 0.95, pulse * 0.14, 0.04)
            }
            NonPhonatedKind::SoftHuff => {
                let pulse = gaussian(t, 0.42, 0.24);
                (
                    pulse * 0.62,
                    pulse * 0.30,
                    pulse * 0.18,
                    pulse * 0.24,
                    pulse * 0.10,
                )
            }
            NonPhonatedKind::ContentExhale => {
                let pulse = smooth_release(t);
                (
                    pulse * 0.40,
                    pulse * 0.14,
                    pulse * 0.12,
                    pulse * 0.32,
                    pulse * 0.08,
                )
            }
            NonPhonatedKind::StartleInhale => {
                let pulse = gaussian(t, 0.26, 0.12);
                (
                    pulse * 0.72,
                    pulse * 0.55,
                    pulse * 0.48,
                    pulse * 0.12,
                    pulse * 0.14,
                )
            }
            NonPhonatedKind::EffortExhale => {
                let pulse = gaussian(t, 0.46, 0.24) * (0.65 + body_load * 0.35);
                (
                    pulse * 0.64,
                    pulse * 0.38,
                    pulse * 0.10,
                    pulse * 0.36,
                    pulse * 0.15,
                )
            }
            NonPhonatedKind::SleepBreath => {
                let phase = (t * std::f32::consts::PI).sin().max(0.0);
                (phase * 0.18, phase * 0.06, phase * 0.09, phase * 0.24, 0.02)
            }
            NonPhonatedKind::ShakeOffBreath => {
                let pulse = gaussian(t, 0.34, 0.17) + gaussian(t, 0.67, 0.12) * 0.45;
                (
                    pulse * 0.58,
                    pulse * 0.48,
                    pulse * 0.14,
                    pulse * 0.30,
                    pulse * 0.09,
                )
            }
        };
        NonPhonatedFrame {
            airflow: (airflow * intensity).clamp(0.0, 1.0),
            turbulence: (turbulence * intensity).clamp(0.0, 1.0),
            nasal: (nasal * intensity).clamp(0.0, 1.0),
            body_resonance: (resonance * intensity).clamp(0.0, 1.0),
            mouth_open: (mouth * intensity).clamp(0.0, 0.35),
        }
    }
}

/// The policy deliberately returns `None` most of the time. The caller supplies
/// a deterministic 0..1 draw generated by the organism-owned RNG.
#[must_use]
pub fn choose_nonphonated(
    intent_is_inspection: bool,
    pleasant_contact: f32,
    startle: f32,
    effort: f32,
    settling_after_stress: bool,
    deterministic_draw: f32,
    seed: u64,
) -> Option<NonPhonatedRequest> {
    let draw = finite(deterministic_draw, 1.0).clamp(0.0, 1.0);
    let request = if startle > 0.62 && draw < 0.42 {
        NonPhonatedRequest {
            kind: NonPhonatedKind::StartleInhale,
            intensity: startle.clamp(0.20, 0.75),
            seed,
            ..NonPhonatedRequest::default()
        }
    } else if settling_after_stress && draw < 0.25 {
        NonPhonatedRequest {
            kind: NonPhonatedKind::ShakeOffBreath,
            intensity: 0.28,
            seed,
            ..NonPhonatedRequest::default()
        }
    } else if effort > 0.72 && draw < 0.20 {
        NonPhonatedRequest {
            kind: NonPhonatedKind::EffortExhale,
            intensity: (effort * 0.45).clamp(0.18, 0.48),
            seed,
            ..NonPhonatedRequest::default()
        }
    } else if pleasant_contact > 0.62 && draw < 0.16 {
        NonPhonatedRequest {
            kind: NonPhonatedKind::ContentExhale,
            intensity: 0.22 + pleasant_contact * 0.16,
            seed,
            ..NonPhonatedRequest::default()
        }
    } else if intent_is_inspection && draw < 0.18 {
        NonPhonatedRequest {
            kind: if draw < 0.07 {
                NonPhonatedKind::SniffPair
            } else {
                NonPhonatedKind::SniffSingle
            },
            intensity: 0.22,
            seed,
            ..NonPhonatedRequest::default()
        }
    } else {
        return None;
    };
    Some(request)
}

fn gaussian(t: f32, center: f32, width: f32) -> f32 {
    let x = (t - center) / width.max(0.01);
    (-0.5 * x * x).exp()
}

fn smooth_release(t: f32) -> f32 {
    let attack = (t / 0.18).clamp(0.0, 1.0);
    let release = ((1.0 - t) / 0.82).clamp(0.0, 1.0);
    let a = attack * attack * (3.0 - 2.0 * attack);
    let r = release * release * (3.0 - 2.0 * release);
    a * r
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pleasant_touch_is_usually_silent() {
        let mut sounds = 0;
        for index in 0..100 {
            if choose_nonphonated(false, 0.9, 0.0, 0.0, false, index as f32 / 100.0, 1).is_some() {
                sounds += 1;
            }
        }
        assert!(sounds <= 17);
    }

    #[test]
    fn sniff_has_no_large_mouth_motion() {
        let mut gesture = NonPhonatedGesture::new(NonPhonatedRequest {
            kind: NonPhonatedKind::SniffPair,
            intensity: 1.0,
            ..NonPhonatedRequest::default()
        });
        for _ in 0..20 {
            assert!(gesture.tick(BodyVoiceFrame::default(), 0.02).mouth_open <= 0.35);
        }
    }
}

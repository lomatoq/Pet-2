#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlinkOwner {
    #[default]
    Physiological,
    Fatigue,
    Social,
    Sleep,
    SleepCheck,
    Protective,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BlinkReason {
    #[default]
    None,
    TimeSinceBlink,
    OcularDryness,
    Fatigue,
    FixationTransition,
    SocialResponse,
    SleepState,
    SleepContactCheck,
    ProtectiveReflex,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BlinkRequest {
    pub owner: BlinkOwner,
    pub strength: f32,
    pub duration: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BlinkContext {
    pub sleeping: bool,
    pub protective: bool,
    pub fatigue: f32,
    pub fixation_transition: bool,
    pub awaiting_important_outcome: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BlinkOutput {
    pub left: f32,
    pub right: f32,
    pub owner: BlinkOwner,
    pub reason: BlinkReason,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlinkController {
    active: Option<BlinkRequest>,
    active_reason: BlinkReason,
    presentation_elapsed: f32,
    social_refractory: f32,
    time_since_blink: f32,
    awake_seconds: f32,
    accumulated_hazard: f32,
    hazard_threshold: f32,
    seed: u64,
    was_protective: bool,
    sequence: u32,
    sleep_check_refractory: f32,
    last_output: BlinkOutput,
}

impl BlinkController {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            active: None,
            active_reason: BlinkReason::None,
            presentation_elapsed: 0.0,
            social_refractory: 0.0,
            time_since_blink: 0.0,
            awake_seconds: 0.0,
            accumulated_hazard: 0.0,
            hazard_threshold: hazard_threshold(seed, 0),
            seed,
            was_protective: false,
            sequence: 0,
            sleep_check_refractory: 0.0,
            last_output: BlinkOutput::default(),
        }
    }

    /// Requests a semantic blink accent. Social callers must invoke this only
    /// for a real entry/response event; a held expression label is not an event.
    pub fn request(&mut self, request: BlinkRequest) {
        self.request_with_reason(request, reason_for_owner(request.owner));
    }

    fn request_with_reason(&mut self, mut request: BlinkRequest, reason: BlinkReason) {
        if request.owner == BlinkOwner::SleepCheck && self.sleep_check_refractory > 0.0 {
            return;
        }
        request.strength = finite_unit(request.strength);
        request.duration = sanitize_duration(request.owner, request.duration);
        if request.strength <= 0.0 {
            return;
        }
        if request.owner == BlinkOwner::Social && self.social_refractory > 0.0 {
            return;
        }
        let replace = self
            .active
            .is_none_or(|current| request.owner > current.owner);
        if replace {
            if request.owner == BlinkOwner::SleepCheck {
                self.sleep_check_refractory = 50.0;
            }
            if request.owner == BlinkOwner::Social {
                self.social_refractory = 7.5;
            }
            self.active = Some(request);
            self.active_reason = reason;
            self.presentation_elapsed = 0.0;
            self.last_output = self.sample();
        }
    }

    /// Advances causal clocks and selects events. It deliberately does not
    /// advance the fast eyelid envelope; call `present` once per body step.
    pub fn update_context(&mut self, dt: f32, context: BlinkContext) {
        let dt = finite_dt(dt);
        self.social_refractory = (self.social_refractory - dt).max(0.0);
        self.sleep_check_refractory = (self.sleep_check_refractory - dt).max(0.0);

        if !context.sleeping
            && self
                .active
                .is_some_and(|a| matches!(a.owner, BlinkOwner::Sleep | BlinkOwner::SleepCheck))
        {
            self.finish_active(false);
        }

        if context.protective && !self.was_protective {
            self.request_with_reason(
                BlinkRequest {
                    owner: BlinkOwner::Protective,
                    strength: 1.0,
                    duration: 0.20,
                },
                BlinkReason::ProtectiveReflex,
            );
        } else if context.sleeping {
            self.request_with_reason(
                BlinkRequest {
                    owner: BlinkOwner::Sleep,
                    strength: 1.0,
                    duration: 2.5,
                },
                BlinkReason::SleepState,
            );
        } else if self.active.is_none() {
            self.awake_seconds += dt;
            self.time_since_blink += dt;
            let fatigue = finite_unit(context.fatigue);
            let age_drive = smoothstep(1.8, 8.5, self.time_since_blink);
            let dryness_drive = smoothstep(18.0, 240.0, self.awake_seconds);
            let outcome_gate = if context.awaiting_important_outcome {
                0.12
            } else {
                1.0
            };
            let rate =
                (0.035 + age_drive * 0.105 + dryness_drive * 0.025 + fatigue * 0.07) * outcome_gate;
            self.accumulated_hazard += rate * dt;
            if context.fixation_transition && self.time_since_blink > 1.25 {
                self.accumulated_hazard += 0.12 + age_drive * 0.10;
            }
            if self.accumulated_hazard >= self.hazard_threshold {
                let reason = if context.fixation_transition {
                    BlinkReason::FixationTransition
                } else if fatigue > 0.72 {
                    BlinkReason::Fatigue
                } else if dryness_drive > age_drive * 0.7 {
                    BlinkReason::OcularDryness
                } else {
                    BlinkReason::TimeSinceBlink
                };
                let fatigued = fatigue > 0.82;
                self.request_with_reason(
                    BlinkRequest {
                        owner: if fatigued {
                            BlinkOwner::Fatigue
                        } else {
                            BlinkOwner::Physiological
                        },
                        strength: if fatigued { 0.82 } else { 1.0 },
                        duration: if fatigued { 0.38 } else { 0.29 },
                    },
                    reason,
                );
            }
        }
        self.was_protective = context.protective;
    }

    /// Evaluates the final visible closure at motor/presentation cadence.
    #[must_use]
    pub fn present(&mut self, dt: f32) -> BlinkOutput {
        let dt = finite_dt(dt);
        let Some(active) = self.active else {
            self.last_output = BlinkOutput::default();
            return self.last_output;
        };

        if active.owner != BlinkOwner::Sleep {
            self.presentation_elapsed = (self.presentation_elapsed + dt).min(active.duration);
        }
        self.last_output = self.sample();
        let output = self.last_output;
        if active.owner != BlinkOwner::Sleep && self.presentation_elapsed >= active.duration {
            self.finish_active(true);
        }
        output
    }

    #[must_use]
    pub fn sample(&self) -> BlinkOutput {
        let Some(active) = self.active else {
            return BlinkOutput::default();
        };
        let closure = if active.owner == BlinkOwner::Sleep {
            1.0
        } else {
            blink_envelope(self.presentation_elapsed, active.duration, active.owner)
        };
        BlinkOutput {
            left: if active.owner == BlinkOwner::SleepCheck {
                1.0 - closure * active.strength.min(0.72)
            } else {
                closure * active.strength
            },
            right: if active.owner == BlinkOwner::SleepCheck {
                1.0
            } else {
                closure
                    * active.strength
                    * if active.owner == BlinkOwner::Sleep {
                        1.0
                    } else {
                        0.985
                    }
            },
            owner: active.owner,
            reason: self.active_reason,
        }
    }

    /// Compatibility path for standalone users that have one clock. Live Pet2
    /// uses `update_context` at cognition cadence and `present` at body cadence.
    #[must_use]
    pub fn tick(&mut self, dt: f32, sleeping: bool, protective: bool, fatigue: f32) -> BlinkOutput {
        self.update_context(
            dt,
            BlinkContext {
                sleeping,
                protective,
                fatigue,
                ..BlinkContext::default()
            },
        );
        self.present(dt)
    }

    fn finish_active(&mut self, completed: bool) {
        let finished_owner = self.active.map(|request| request.owner);
        self.active = None;
        self.active_reason = BlinkReason::None;
        self.presentation_elapsed = 0.0;
        self.last_output = BlinkOutput::default();
        if completed && finished_owner.is_some_and(|owner| owner != BlinkOwner::SleepCheck) {
            self.time_since_blink = 0.0;
            self.accumulated_hazard = 0.0;
            self.sequence = self.sequence.wrapping_add(1);
            self.hazard_threshold = hazard_threshold(self.seed, self.sequence);
        }
    }
}

impl Default for BlinkController {
    fn default() -> Self {
        Self::new(1)
    }
}

fn reason_for_owner(owner: BlinkOwner) -> BlinkReason {
    match owner {
        BlinkOwner::Physiological => BlinkReason::TimeSinceBlink,
        BlinkOwner::Fatigue => BlinkReason::Fatigue,
        BlinkOwner::Social => BlinkReason::SocialResponse,
        BlinkOwner::Sleep => BlinkReason::SleepState,
        BlinkOwner::SleepCheck => BlinkReason::SleepContactCheck,
        BlinkOwner::Protective => BlinkReason::ProtectiveReflex,
    }
}

fn sanitize_duration(owner: BlinkOwner, duration: f32) -> f32 {
    let fallback = match owner {
        BlinkOwner::Physiological | BlinkOwner::Protective => 0.29,
        BlinkOwner::Fatigue => 0.38,
        BlinkOwner::Social => 0.52,
        BlinkOwner::Sleep | BlinkOwner::SleepCheck => 1.1,
    };
    let duration = if duration.is_finite() {
        duration
    } else {
        fallback
    };
    match owner {
        BlinkOwner::Physiological | BlinkOwner::Protective => duration.clamp(0.22, 0.42),
        BlinkOwner::Fatigue | BlinkOwner::Social => duration.clamp(0.28, 0.80),
        BlinkOwner::Sleep | BlinkOwner::SleepCheck => duration.clamp(0.20, 2.5),
    }
}

fn blink_envelope(elapsed: f32, duration: f32, owner: BlinkOwner) -> f32 {
    let phase = (elapsed / duration.max(0.001)).clamp(0.0, 1.0);
    let (close_end, hold_end, main_open_end) = match owner {
        BlinkOwner::Protective => (0.27, 0.43, 0.82),
        BlinkOwner::Fatigue | BlinkOwner::Social => (0.24, 0.39, 0.74),
        BlinkOwner::SleepCheck => (0.36, 0.58, 0.82),
        _ => (0.24, 0.34, 0.72),
    };
    if phase < close_end {
        smoothstep(0.0, close_end, phase).powf(0.72)
    } else if phase < hold_end {
        1.0
    } else if phase < main_open_end {
        1.0 - smoothstep(hold_end, main_open_end, phase) * 0.86
    } else {
        0.14 * (1.0 - smoothstep(main_open_end, 1.0, phase))
    }
}

fn hazard_threshold(seed: u64, sequence: u32) -> f32 {
    let mut value = seed
        .wrapping_add(u64::from(sequence).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0xD1B5_4A32_D192_ED03);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    let unit = ((value >> 40) as u32) as f32 / ((1_u32 << 24) - 1) as f32;
    (-(1.0 - unit.clamp(0.001, 0.999)).ln()).clamp(0.45, 2.4)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(0.000_001)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn finite_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn finite_dt(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 0.1)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_envelope_has_fast_close_hold_and_slower_open_at_all_rates() {
        for hz in [30, 60, 120] {
            let mut blink = BlinkController::new(7);
            blink.request(BlinkRequest {
                owner: BlinkOwner::Physiological,
                strength: 1.0,
                duration: 0.29,
            });
            let mut samples = Vec::new();
            while blink.active.is_some() {
                samples.push(blink.present(1.0 / hz as f32).left);
            }
            let peak = samples
                .iter()
                .position(|value| *value >= 0.99)
                .expect("blink reaches closure");
            let tail = samples.len() - 1 - peak;
            assert!(tail > peak, "hz={hz} peak={peak} tail={tail} {samples:?}");
            assert!(samples.len() >= 7, "hz={hz} samples={samples:?}");
            assert!(samples.iter().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn split_context_and_presentation_clocks_match_across_body_rates() {
        let mut endpoints = Vec::new();
        for hz in [30, 60, 120] {
            let mut blink = BlinkController::new(0xE1E5);
            blink.request(BlinkRequest {
                owner: BlinkOwner::Social,
                strength: 0.82,
                duration: 0.52,
            });
            let mut peak = 0.0_f32;
            for _ in 0..hz {
                peak = peak.max(blink.present(1.0 / hz as f32).left);
            }
            endpoints.push((peak, blink.sample().left));
        }
        for (peak, final_value) in endpoints {
            assert!((peak - 0.82).abs() < 0.015, "peak={peak}");
            assert_eq!(final_value, 0.0);
        }
    }

    #[test]
    fn sleep_contact_check_is_unilateral_finite_and_refractory_at_all_rates() {
        for hz in [30, 60, 120] {
            let mut blink = BlinkController::new(7);
            let _ = blink.tick(1.0 / hz as f32, true, false, 0.9);
            blink.request(BlinkRequest {
                owner: BlinkOwner::SleepCheck,
                strength: 0.72,
                duration: 1.1,
            });
            let mut lowest = 1.0_f32;
            for _ in 0..2 * hz {
                blink.update_context(
                    1.0 / hz as f32,
                    BlinkContext {
                        sleeping: true,
                        ..BlinkContext::default()
                    },
                );
                let out = blink.present(1.0 / hz as f32);
                lowest = lowest.min(out.left);
                assert_eq!(out.right, 1.0);
            }
            assert!((0.279..0.30).contains(&lowest));
            blink.request(BlinkRequest {
                owner: BlinkOwner::SleepCheck,
                strength: 0.72,
                duration: 1.1,
            });
            assert_eq!(blink.present(1.0 / hz as f32).left, 1.0);
        }
    }

    #[test]
    fn sustained_fatigue_has_open_eye_intervals() {
        let mut blink = BlinkController::new(7);
        let mut peaks = 0;
        let mut closed = false;
        for _ in 0..3600 {
            let out = blink.tick(1.0 / 60.0, false, false, 0.95);
            let next = out.left > 0.5;
            peaks += usize::from(next && !closed);
            closed = next;
        }
        assert!((3..=18).contains(&peaks), "fatigue peaks/minute: {peaks}");
    }

    #[test]
    fn repeated_social_label_does_not_create_a_periodic_blink() {
        let mut blink = BlinkController::new(7);
        blink.request(BlinkRequest {
            owner: BlinkOwner::Social,
            strength: 0.82,
            duration: 0.52,
        });
        let mut social_starts = 0;
        let mut active = false;
        for _ in 0..3600 {
            blink.update_context(1.0 / 60.0, BlinkContext::default());
            let out = blink.present(1.0 / 60.0);
            let next = out.owner == BlinkOwner::Social && out.left > 0.05;
            social_starts += usize::from(next && !active);
            active = next;
        }
        assert_eq!(social_starts, 1);
    }

    #[test]
    fn physiological_hazard_is_deterministic_sparse_and_outcome_suppressed() {
        let run = |awaiting_important_outcome| {
            let mut blink = BlinkController::new(0xCAFE);
            let mut starts = 0;
            let mut active = false;
            for _ in 0..60 * 600 {
                blink.update_context(
                    1.0 / 60.0,
                    BlinkContext {
                        awaiting_important_outcome,
                        ..BlinkContext::default()
                    },
                );
                let out = blink.present(1.0 / 60.0);
                let next = out.owner == BlinkOwner::Physiological && out.left > 0.05;
                starts += usize::from(next && !active);
                active = next;
            }
            starts
        };
        let ordinary = run(false);
        assert_eq!(ordinary, run(false));
        assert!((25..=130).contains(&ordinary), "ordinary={ordinary}");
        assert!(run(true) < ordinary / 2);
    }

    #[test]
    fn waking_releases_sleep_closure() {
        let mut blink = BlinkController::new(7);
        for _ in 0..120 {
            let _ = blink.tick(1.0 / 60.0, true, false, 0.0);
        }
        for _ in 0..60 {
            let _ = blink.tick(1.0 / 60.0, false, false, 0.0);
        }
        assert_eq!(blink.tick(1.0 / 60.0, false, false, 0.0).left, 0.0);
    }

    #[test]
    fn protective_blink_preempts_social() {
        let mut blink = BlinkController::new(3);
        blink.request(BlinkRequest {
            owner: BlinkOwner::Social,
            strength: 0.8,
            duration: 0.6,
        });
        let out = blink.tick(0.02, false, true, 0.0);
        assert_eq!(out.owner, BlinkOwner::Protective);
        assert_eq!(out.reason, BlinkReason::ProtectiveReflex);
    }
}

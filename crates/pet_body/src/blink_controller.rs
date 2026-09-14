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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BlinkRequest {
    pub owner: BlinkOwner,
    pub strength: f32,
    pub duration: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BlinkOutput {
    pub left: f32,
    pub right: f32,
    pub owner: BlinkOwner,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlinkController {
    active: Option<BlinkRequest>,
    elapsed: f32,
    social_refractory: f32,
    physiological_clock: f32,
    next_physiological: f32,
    seed_phase: f32,
    was_protective: bool,
    sequence: u32,
    sleep_check_refractory: f32,
}

impl BlinkController {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let seed_phase = (seed as u32 as f32 / u32::MAX as f32) * 6.2831855;
        Self {
            active: None,
            elapsed: 0.0,
            social_refractory: 0.0,
            physiological_clock: 0.0,
            next_physiological: 3.8 + seed_phase.sin().abs() * 2.7,
            seed_phase,
            was_protective: false,
            sequence: 0,
            sleep_check_refractory: 0.0,
        }
    }

    pub fn request(&mut self, request: BlinkRequest) {
        if request.owner == BlinkOwner::SleepCheck && self.sleep_check_refractory > 0.0 {
            return;
        }
        let mut request = request;
        request.strength = finite_unit(request.strength);
        request.duration = if request.duration.is_finite() {
            request.duration.clamp(0.04, 2.5)
        } else {
            0.1
        };
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
            self.elapsed = 0.0;
        }
    }

    #[must_use]
    pub fn tick(&mut self, dt: f32, sleeping: bool, protective: bool, fatigue: f32) -> BlinkOutput {
        let dt = finite_dt(dt);
        self.social_refractory = (self.social_refractory - dt).max(0.0);
        self.physiological_clock += dt;
        self.sleep_check_refractory = (self.sleep_check_refractory - dt).max(0.0);

        if !sleeping
            && self
                .active
                .is_some_and(|a| matches!(a.owner, BlinkOwner::Sleep | BlinkOwner::SleepCheck))
        {
            self.active = None;
            self.elapsed = 0.0;
            self.physiological_clock = 0.0;
        }

        if protective && !self.was_protective {
            self.request(BlinkRequest {
                owner: BlinkOwner::Protective,
                strength: 1.0,
                duration: 0.12,
            });
        } else if sleeping {
            self.request(BlinkRequest {
                owner: BlinkOwner::Sleep,
                strength: 1.0,
                duration: 2.5,
            });
        } else if self.active.is_none()
            && self.social_refractory <= 0.0
            && self.physiological_clock >= self.next_physiological
        {
            self.request(BlinkRequest {
                owner: if fatigue.is_finite() && fatigue > 0.82 {
                    BlinkOwner::Fatigue
                } else {
                    BlinkOwner::Physiological
                },
                strength: if fatigue > 0.82 { 0.75 } else { 1.0 },
                duration: if fatigue > 0.82 { 0.42 } else { 0.14 },
            });
        }
        self.was_protective = protective;

        let Some(active) = self.active else {
            return BlinkOutput::default();
        };
        self.elapsed += dt;
        let progress = (self.elapsed / active.duration.max(0.04)).clamp(0.0, 1.0);
        let envelope = if active.owner == BlinkOwner::Sleep {
            1.0
        } else {
            (progress * std::f32::consts::PI).sin().powi(2)
        };
        let output = BlinkOutput {
            left: if active.owner == BlinkOwner::SleepCheck {
                1.0 - envelope * active.strength.min(0.72)
            } else {
                envelope * active.strength
            },
            right: if active.owner == BlinkOwner::SleepCheck {
                1.0
            } else {
                envelope * active.strength * if sleeping { 1.0 } else { 0.97 }
            },
            owner: active.owner,
        };
        if self.elapsed >= active.duration && active.owner != BlinkOwner::Sleep {
            self.active = None;
            self.elapsed = 0.0;
            self.physiological_clock = 0.0;
            self.sequence = self.sequence.wrapping_add(1);
            self.next_physiological =
                4.5 + 3.0 * (0.5 + 0.5 * (self.seed_phase + self.sequence as f32 * 0.73).sin());
        }
        output
    }
}

impl Default for BlinkController {
    fn default() -> Self {
        Self::new(1)
    }
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
                let out = blink.tick(1.0 / hz as f32, true, false, 0.9);
                lowest = lowest.min(out.left);
                assert_eq!(out.right, 1.0);
            }
            assert!((0.279..0.30).contains(&lowest));
            blink.request(BlinkRequest {
                owner: BlinkOwner::SleepCheck,
                strength: 0.72,
                duration: 1.1,
            });
            assert_eq!(blink.tick(1.0 / hz as f32, true, false, 0.9).left, 1.0);
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
        assert!((4..=16).contains(&peaks), "fatigue peaks/minute: {peaks}");
    }

    #[test]
    fn repeated_social_request_finishes_and_respects_refractory_period() {
        let mut blink = BlinkController::new(7);
        let mut peaks = 0;
        let mut closed = false;
        for _ in 0..3600 {
            blink.request(BlinkRequest {
                owner: BlinkOwner::Social,
                strength: 0.82,
                duration: 0.52,
            });
            let out = blink.tick(1.0 / 60.0, false, false, 0.0);
            let next = out.left > 0.5;
            peaks += usize::from(next && !closed);
            closed = next;
        }
        assert!((4..=12).contains(&peaks), "social peaks/minute: {peaks}");
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
    fn social_blink_blocks_lower_priority_physiological_blink() {
        let mut blink = BlinkController::new(7);
        blink.request(BlinkRequest {
            owner: BlinkOwner::Social,
            strength: 1.0,
            duration: 0.5,
        });
        for _ in 0..12 {
            let _ = blink.tick(0.05, false, false, 0.0);
        }
        assert!(blink.social_refractory > 1.0);
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
    }
}

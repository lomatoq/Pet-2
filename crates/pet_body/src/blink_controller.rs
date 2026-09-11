#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlinkOwner {
    #[default]
    Physiological,
    Fatigue,
    Social,
    Sleep,
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
        }
    }

    pub fn request(&mut self, request: BlinkRequest) {
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
        let replace = self
            .active
            .is_none_or(|current| request.owner >= current.owner);
        if replace {
            if request.owner == BlinkOwner::Social {
                self.social_refractory = 1.8;
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

        if protective {
            self.request(BlinkRequest { owner: BlinkOwner::Protective, strength: 1.0, duration: 0.12 });
        } else if sleeping {
            self.request(BlinkRequest { owner: BlinkOwner::Sleep, strength: 1.0, duration: 2.5 });
        } else if fatigue.is_finite() && fatigue > 0.82 && self.active.is_none() {
            self.request(BlinkRequest { owner: BlinkOwner::Fatigue, strength: 0.75, duration: 0.42 });
        } else if self.active.is_none()
            && self.social_refractory <= 0.0
            && self.physiological_clock >= self.next_physiological
        {
            self.request(BlinkRequest { owner: BlinkOwner::Physiological, strength: 1.0, duration: 0.14 });
            self.physiological_clock = 0.0;
            self.next_physiological = 3.5
                + 3.4 * (0.5 + 0.5 * (self.seed_phase + self.physiological_clock * 0.37).sin());
        }

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
            left: envelope * active.strength,
            right: envelope * active.strength * 0.97,
            owner: active.owner,
        };
        if self.elapsed >= active.duration && active.owner != BlinkOwner::Sleep {
            self.active = None;
            self.elapsed = 0.0;
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
    if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 }
}

fn finite_dt(value: f32) -> f32 {
    if value.is_finite() { value.clamp(0.0, 0.1) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn social_blink_blocks_lower_priority_physiological_blink() {
        let mut blink = BlinkController::new(7);
        blink.request(BlinkRequest { owner: BlinkOwner::Social, strength: 1.0, duration: 0.5 });
        for _ in 0..12 { let _ = blink.tick(0.05, false, false, 0.0); }
        assert!(blink.social_refractory > 1.0);
    }

    #[test]
    fn protective_blink_preempts_social() {
        let mut blink = BlinkController::new(3);
        blink.request(BlinkRequest { owner: BlinkOwner::Social, strength: 0.8, duration: 0.6 });
        let out = blink.tick(0.02, false, true, 0.0);
        assert_eq!(out.owner, BlinkOwner::Protective);
    }
}
//! Brief viewer-oriented attention from aggregate activity edges only. No text,
//! key identities, cursor chasing, capture hooks or independent blink writer.
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default)]
pub struct ActivityGlanceInput {
    pub typing_rate_hz: f32,
    pub typing_pause_seconds: f32,
    /// Generic recent activity (possibly idle-time derived), not proof of typing.
    pub user_activity_rate: f32,
    pub enabled: bool,
    pub sleeping: bool,
    pub danger: bool,
    pub dragging: bool,
    pub object_busy: bool,
    pub focus_mode: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum ActivityGlanceReason {
    #[default]
    None,
    TypingOnset,
    ActivityOnset,
    AfterBurstPause,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ActivityGlanceOutput {
    pub active: bool,
    pub started: bool,
    pub progress: f32,
    pub reason: ActivityGlanceReason,
}

const HOLD_SECONDS: f32 = 1.3;
const REFRACTORY_SECONDS: f32 = 22.0;

#[derive(Debug, Clone, Default)]
pub struct ActivityGlance {
    initialized: bool,
    armed: bool,
    typing_armed: bool,
    typing_quiet_seconds: f32,
    quiet_seconds: f32,
    onset_seconds: f32,
    burst_active: bool,
    burst_typing: bool,
    burst_seconds: f32,
    pause_seconds: f32,
    hold_remaining: f32,
    refractory: f32,
    reason: ActivityGlanceReason,
}

impl ActivityGlance {
    /// Root should still prioritize danger, direct touch and committed object
    /// goals. Active means nominate semantic viewer gaze, not a screen target.
    pub fn tick(&mut self, input: ActivityGlanceInput, dt: f32) -> ActivityGlanceOutput {
        if !dt.is_finite() || dt <= 0.0 {
            return ActivityGlanceOutput::default();
        }
        let dt = dt.min(0.25);
        self.refractory = (self.refractory - dt).max(0.0);
        self.hold_remaining = (self.hold_remaining - dt).max(0.0);
        if !input.enabled || input.sleeping || input.danger || input.dragging || input.object_busy {
            // Consume suppressed activity instead of replaying a stale glance
            // when the competing task ends. Keep the refractory clock.
            let refractory = self.refractory;
            *self = Self {
                initialized: true,
                refractory,
                ..Self::default()
            };
            return ActivityGlanceOutput::default();
        }
        if !self.initialized {
            self.initialized = true;
            return ActivityGlanceOutput::default();
        }
        let typing = input.typing_rate_hz.is_finite()
            && input.typing_rate_hz >= 0.65
            && input.typing_pause_seconds.is_finite()
            && input.typing_pause_seconds < 0.35;
        let generic = input.user_activity_rate.is_finite() && input.user_activity_rate >= 0.65;
        let quiet =
            !typing && input.user_activity_rate.is_finite() && input.user_activity_rate <= 0.25;
        // Generic idle-derived activity can remain high after typing stops; it
        // must not mask a new keyboard burst after a quiet typing baseline.
        if typing {
            self.typing_quiet_seconds = 0.0;
        } else {
            self.typing_quiet_seconds += dt;
            if self.typing_quiet_seconds >= 0.8 {
                self.typing_armed = true;
            }
        }
        let mut started = false;
        if self.burst_active {
            self.burst_typing |= typing;
            let continues = if self.burst_typing { typing } else { generic };
            if continues {
                self.burst_seconds += dt;
                self.pause_seconds = 0.0;
            } else {
                self.pause_seconds += dt;
                let meaningful_pause = self.pause_seconds >= 1.6
                    && (!self.burst_typing
                        || (input.typing_pause_seconds.is_finite()
                            && input.typing_pause_seconds >= 1.6));
                if meaningful_pause {
                    if self.burst_seconds >= 1.2 {
                        // Focus mode suppresses onset, but this one quiet glance
                        // after a real pause is non-vocal and makes no demand.
                        started = self.begin(ActivityGlanceReason::AfterBurstPause);
                    }
                    self.burst_active = false;
                    self.armed = true;
                    self.onset_seconds = 0.0;
                    self.burst_seconds = 0.0;
                }
            }
        } else if typing || generic {
            self.quiet_seconds = 0.0;
            if self.armed || (typing && self.typing_armed) {
                self.onset_seconds += dt;
                if self.onset_seconds >= 0.3 {
                    self.burst_active = true;
                    self.burst_typing = typing;
                    self.burst_seconds = self.onset_seconds;
                    self.pause_seconds = 0.0;
                    self.armed = false;
                    self.typing_armed = false;
                    if !input.focus_mode {
                        started = self.begin(if typing {
                            ActivityGlanceReason::TypingOnset
                        } else {
                            ActivityGlanceReason::ActivityOnset
                        });
                    }
                }
            }
        } else {
            self.onset_seconds = 0.0;
            if quiet {
                self.quiet_seconds += dt;
                if self.quiet_seconds >= 0.8 {
                    self.armed = true;
                }
            } else {
                self.quiet_seconds = 0.0;
            }
        }
        let active = self.hold_remaining > 0.0;
        ActivityGlanceOutput {
            active,
            started,
            progress: if active {
                (1.0 - self.hold_remaining / HOLD_SECONDS).clamp(0.0, 1.0)
            } else {
                0.0
            },
            reason: if active {
                self.reason
            } else {
                ActivityGlanceReason::None
            },
        }
    }

    fn begin(&mut self, reason: ActivityGlanceReason) -> bool {
        if self.refractory > 0.0 || self.hold_remaining > 0.0 {
            return false;
        }
        self.hold_remaining = HOLD_SECONDS;
        self.refractory = REFRACTORY_SECONDS;
        self.reason = reason;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn idle() -> ActivityGlanceInput {
        ActivityGlanceInput {
            enabled: true,
            typing_pause_seconds: 60.0,
            ..Default::default()
        }
    }
    fn typing() -> ActivityGlanceInput {
        ActivityGlanceInput {
            typing_rate_hz: 4.0,
            typing_pause_seconds: 0.0,
            user_activity_rate: 1.0,
            ..idle()
        }
    }
    fn run(
        state: &mut ActivityGlance,
        input: ActivityGlanceInput,
        seconds: f32,
        hz: u32,
    ) -> (usize, f32) {
        let mut starts = 0;
        let mut active_seconds = 0.0;
        for _ in 0..(seconds * hz as f32).round() as usize {
            let out = state.tick(input, 1.0 / hz as f32);
            starts += usize::from(out.started);
            if out.active {
                active_seconds += 1.0 / hz as f32;
            }
        }
        (starts, active_seconds)
    }
    #[test]
    fn activity_glance_real_typing_edge_is_not_masked_by_generic_recent_activity() {
        let mut state = ActivityGlance::default();
        assert_eq!(
            run(
                &mut state,
                ActivityGlanceInput {
                    user_activity_rate: 1.0,
                    ..idle()
                },
                5.0,
                60
            )
            .0,
            0
        );
        assert_eq!(run(&mut state, typing(), 5.0, 60).0, 1);
    }

    #[test]
    fn activity_glance_held_typing_is_one_bounded_glance_at_all_rates() {
        for hz in [30, 60, 120] {
            let mut state = ActivityGlance::default();
            assert_eq!(run(&mut state, idle(), 2.0, hz).0, 0);
            let (starts, hold) = run(&mut state, typing(), 90.0, hz);
            assert_eq!(starts, 1);
            assert!((hold - 1.3).abs() <= 2.0 / hz as f32);
        }
    }
    #[test]
    fn activity_glance_initial_activity_idle_and_disabled_do_not_fire() {
        let mut state = ActivityGlance::default();
        assert_eq!(run(&mut state, typing(), 90.0, 60).0, 0);
        assert_eq!(run(&mut state, idle(), 90.0, 60).0, 0);
        assert_eq!(
            run(
                &mut state,
                ActivityGlanceInput {
                    enabled: false,
                    ..typing()
                },
                30.0,
                60
            )
            .0,
            0
        );
        assert_eq!(run(&mut state, typing(), 30.0, 60).0, 0);
    }
    #[test]
    fn activity_glance_short_typing_pauses_do_not_retrigger() {
        let mut state = ActivityGlance::default();
        run(&mut state, idle(), 2.0, 60);
        let mut starts = 0;
        for _ in 0..60 {
            starts += run(&mut state, typing(), 1.0, 60).0;
            starts += run(
                &mut state,
                ActivityGlanceInput {
                    typing_pause_seconds: 0.5,
                    typing_rate_hz: 2.0,
                    user_activity_rate: 1.0,
                    ..idle()
                },
                0.5,
                60,
            )
            .0;
        }
        assert_eq!(starts, 1);
    }
    #[test]
    fn activity_glance_focus_waits_for_real_pause_and_safety_cancels() {
        for hz in [30, 60, 120] {
            let mut state = ActivityGlance::default();
            run(&mut state, idle(), 2.0, hz);
            assert_eq!(
                run(
                    &mut state,
                    ActivityGlanceInput {
                        focus_mode: true,
                        ..typing()
                    },
                    3.0,
                    hz
                )
                .0,
                0
            );
            assert_eq!(
                run(
                    &mut state,
                    ActivityGlanceInput {
                        focus_mode: true,
                        user_activity_rate: 1.0,
                        typing_pause_seconds: 2.0,
                        ..idle()
                    },
                    2.0,
                    hz
                )
                .0,
                1
            );
            assert!(
                !state
                    .tick(
                        ActivityGlanceInput {
                            danger: true,
                            ..idle()
                        },
                        1.0 / hz as f32
                    )
                    .active
            );
            assert_eq!(run(&mut state, typing(), 30.0, hz).0, 0);
        }
    }
}

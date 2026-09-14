//! Native local clock and quiet vocal context. Reads time only; never changes
//! the OS clock/time zone or treats a calendar transition as an audio event.
use lifecore::VocalRequest;
use pet_audio::{NonPhonatedKind, NonPhonatedRequest};
use pet_motor::SomaticActuationPacket;

fn clock_fraction(hour: u32, minute: u32, second: u32) -> f32 {
    ((hour.min(23) * 3600 + minute.min(59) * 60 + second.min(59)) as f32 / 86400.0).clamp(0.0, 1.0)
}

pub fn local_time_01() -> f32 {
    #[cfg(windows)]
    {
        let mut time =
            std::mem::MaybeUninit::<windows_sys::Win32::Foundation::SYSTEMTIME>::zeroed();
        // GetLocalTime includes the system's selected time zone and DST.
        unsafe {
            windows_sys::Win32::System::SystemInformation::GetLocalTime(time.as_mut_ptr());
            let time = time.assume_init();
            clock_fraction(
                u32::from(time.wHour),
                u32::from(time.wMinute),
                u32::from(time.wSecond),
            )
        }
    }
    #[cfg(target_os = "macos")]
    {
        let seconds = unsafe { libc::time(std::ptr::null_mut()) };
        let mut local = std::mem::MaybeUninit::<libc::tm>::zeroed();
        if unsafe { libc::localtime_r(&seconds, local.as_mut_ptr()) }.is_null() {
            return 0.5;
        }
        let local = unsafe { local.assume_init() };
        clock_fraction(
            local.tm_hour.max(0) as u32,
            local.tm_min.max(0) as u32,
            local.tm_sec.max(0) as u32,
        )
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        0.5
    }
}

/// Smooth shoulders avoid an audible gain discontinuity exactly at bedtime.
pub fn night_softness(local_time: f32) -> f32 {
    if !local_time.is_finite() {
        return 0.0;
    }
    let hour = local_time.rem_euclid(1.0) * 24.0;
    let ramp = if hour >= 21.0 {
        ((hour - 21.0) / 1.0).clamp(0.0, 1.0)
    } else {
        ((7.0 - hour) / 1.0).clamp(0.0, 1.0)
    };
    ramp * ramp * (3.0 - 2.0 * ramp)
}

/// Apply after other tuning, before the existing VocalArbiter. This never
/// bypasses quiet/mute, raises priority, rewrites emotion, or starts a voice.
pub fn soften_night_request(request: &mut VocalRequest, local_time: f32) {
    let amount = night_softness(local_time);
    request.gain *= 1.0 - 0.78 * amount;
    request.phenotype.breathiness_delta =
        (request.phenotype.breathiness_delta + 0.25 * amount).clamp(-0.4, 0.4);
}

#[derive(Debug, Default)]
pub struct SleepSnuffleContext {
    previous_phase: String,
    was_sleeping: bool,
    onset_observed: bool,
    cooldown: f32,
    sequence: u64,
}

impl SleepSnuffleContext {
    /// This nominates one low-priority unvoiced gesture; the app must still
    /// admit it through its shared audio arbiter before sending to the engine.
    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        packet: &SomaticActuationPacket,
        sleeping: bool,
        supported: bool,
        quiet: bool,
        audio_busy: bool,
        local_time: f32,
        dt: f32,
    ) -> Option<NonPhonatedRequest> {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.cooldown = (self.cooldown - dt).max(0.0);
        if !sleeping {
            self.onset_observed = false;
        }
        let new_phase = self.previous_phase != packet.phase_name;
        let onset =
            sleeping && supported && !self.onset_observed && packet.phase_name == "nrem_hold";
        let micro_event = sleeping
            && self.was_sleeping
            && supported
            && new_phase
            && matches!(
                packet.phase_name.as_str(),
                "structured_local_twitches" | "micro_arousal_or_continue"
            );
        self.previous_phase = packet.phase_name.clone();
        self.was_sleeping = sleeping;
        if onset {
            self.onset_observed = true;
        }
        if !(onset || micro_event) || quiet || audio_busy || self.cooldown > 0.0 {
            return None;
        }
        self.cooldown = 30.0;
        self.sequence = self.sequence.wrapping_add(1);
        Some(NonPhonatedRequest {
            kind: NonPhonatedKind::SleepBreath,
            intensity: 0.10 - 0.055 * night_softness(local_time),
            pan: 0.0,
            seed: packet.source_bout_id ^ self.sequence.rotate_left(17),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_clock_and_night_boundaries_are_bounded_without_utc_assumption() {
        assert!((0.0..1.0).contains(&local_time_01()));
        assert_eq!(clock_fraction(12, 0, 0), 0.5);
        assert_eq!(night_softness(0.0), 1.0);
        assert_eq!(night_softness(0.5), 0.0);
        assert_eq!(night_softness(23.0 / 24.0), 1.0);
        assert_eq!(night_softness(f32::NAN), 0.0);
        assert!((night_softness(21.5 / 24.0) - 0.5).abs() < 0.001);
    }
    #[test]
    fn nighttime_only_attenuates_and_does_not_escalate_priority() {
        let original: VocalRequest = serde_json::from_value(serde_json::json!({
            "motif_id":1,"gain":0.6,"pan":0.0,"pitch_scale":1.0,
            "tempo_scale":1.0,"stress":0.2,"purr":false,"priority":120
        }))
        .unwrap();
        let mut daytime = original.clone();
        soften_night_request(&mut daytime, 0.5);
        assert_eq!(daytime, original);
        let mut nighttime = original.clone();
        soften_night_request(&mut nighttime, 0.0);
        assert!((nighttime.gain - 0.132).abs() < 0.001);
        assert_eq!(nighttime.priority, original.priority);
        assert_eq!(nighttime.style, original.style);
        assert!(nighttime.phenotype.breathiness_delta > original.phenotype.breathiness_delta);
    }

    #[test]
    fn sleep_snuffle_is_phase_caused_never_a_repeating_timer_or_quiet_bypass() {
        let mut c = SleepSnuffleContext::default();
        let mut packet = SomaticActuationPacket {
            phase_name: "nrem_hold".into(),
            ..Default::default()
        };
        let first = c
            .observe(&packet, true, true, false, false, 0.0, 0.05)
            .unwrap();
        assert_eq!(first.kind, NonPhonatedKind::SleepBreath);
        assert!(first.intensity <= 0.05);
        for _ in 0..2400 {
            assert!(
                c.observe(&packet, true, true, false, false, 0.0, 0.05)
                    .is_none()
            );
        }
        packet.phase_name = "structured_local_twitches".into();
        assert!(
            c.observe(&packet, true, true, false, false, 0.0, 0.05)
                .is_some()
        );
        let mut quiet = SleepSnuffleContext::default();
        packet.phase_name = "nrem_hold".into();
        assert!(
            quiet
                .observe(&packet, true, true, true, false, 0.0, 0.05)
                .is_none()
        );
        assert!(
            quiet
                .observe(&packet, true, true, false, false, 0.0, 0.05)
                .is_none()
        );
        let mut air = SleepSnuffleContext::default();
        assert!(
            air.observe(&packet, true, false, false, false, 0.0, 0.05)
                .is_none()
        );
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum PhonationRegime {
    #[default]
    Stable = 0,
    Breathy = 1,
    Subharmonic2 = 2,
    Aperiodic = 3,
    VoiceBreak = 4,
    Biphonic = 5,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PhonationState {
    regime: PhonationRegime,
    cycles_remaining: u8,
    refractory_cycles: u8,
}

impl PhonationState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn update_cycle(
        &mut self,
        instability: f32,
        pressure: f32,
        tension: f32,
        random: f32,
    ) -> PhonationRegime {
        if self.cycles_remaining > 0 {
            self.cycles_remaining -= 1;
            return self.regime;
        }
        if self.regime != PhonationRegime::Stable {
            self.regime = PhonationRegime::Stable;
            self.refractory_cycles = 4;
        }
        if self.refractory_cycles > 0 {
            self.refractory_cycles -= 1;
            return self.regime;
        }
        let drive = instability.clamp(0.0, 1.0);
        let choice = random.abs().clamp(0.0, 1.0);
        self.regime = if pressure < 0.12 && tension > 0.68 && drive > 0.46 {
            PhonationRegime::VoiceBreak
        } else if drive > 0.82 && choice > 0.72 {
            PhonationRegime::Biphonic
        } else if drive > 0.68 && choice > 0.44 {
            PhonationRegime::Aperiodic
        } else if drive > 0.52 && choice > 0.18 {
            PhonationRegime::Subharmonic2
        } else if pressure < 0.23 || drive > 0.30 {
            PhonationRegime::Breathy
        } else {
            PhonationRegime::Stable
        };
        self.cycles_remaining = match self.regime {
            PhonationRegime::Stable => 0,
            PhonationRegime::Breathy => 2,
            PhonationRegime::Subharmonic2 => 3 + (choice * 3.0) as u8,
            PhonationRegime::Aperiodic => 2 + (choice * 2.0) as u8,
            PhonationRegime::VoiceBreak => 1 + (choice * 2.0) as u8,
            PhonationRegime::Biphonic => 3 + (choice * 3.0) as u8,
        };
        self.regime
    }

    pub(crate) fn regime(self) -> PhonationRegime {
        self.regime
    }
}

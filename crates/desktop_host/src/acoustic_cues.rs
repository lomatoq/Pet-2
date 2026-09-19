//! Local, teachable acoustic-cue features and matching.
//!
//! This is deliberately a small-vocabulary, same-user/same-room matcher. It is
//! not speech-to-text. Enrollment keeps bounded MFCC time sequences; PCM never
//! enters the persisted model. The final enrollment example is held out from
//! threshold fitting and must pass before an existing model can be replaced.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CUE_MODEL_SCHEMA_VERSION: u32 = 1;
pub const TARGET_SAMPLE_RATE: u32 = 16_000;
pub const REQUIRED_POSITIVE_EXAMPLES: usize = 5;
pub const REQUIRED_OTHER_EXAMPLES: usize = 5;
const FEATURE_DIM: usize = 24;
const MEL_FILTERS: usize = 26;
const FFT_SIZE: usize = 512;
const WINDOW_SAMPLES: usize = 400;
const HOP_SAMPLES: usize = 160;
const MIN_CUE_SAMPLES: usize = 2_880;
const MAX_CUE_SAMPLES: usize = 40_000;
const MAX_TEMPLATES_PER_CLASS: usize = 40;
const MAX_FEATURE_FRAMES: usize = 248;
const MAX_FEATURE_ABS: f32 = 64.0;
const MIN_NEGATIVE_MARGIN: f32 = 0.055;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueKind {
    Name,
    Quiet,
    Sit,
    Jump,
    Circle,
    Come,
    Stay,
    Dash,
    Up,
    Down,
    Left,
    Right,
    Sleep,
    Wake,
    Blink,
    Look,
    Bow,
    Shake,
    Stretch,
    Play,
    Home,
}
impl CueKind {
    pub const ALL: [Self; 21] = [
        Self::Name,
        Self::Quiet,
        Self::Sit,
        Self::Jump,
        Self::Circle,
        Self::Come,
        Self::Stay,
        Self::Dash,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::Sleep,
        Self::Wake,
        Self::Blink,
        Self::Look,
        Self::Bow,
        Self::Shake,
        Self::Stretch,
        Self::Play,
        Self::Home,
    ];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Имя · Benny",
            Self::Quiet => "Тише",
            Self::Sit => "Сидеть",
            Self::Jump => "Прыгни",
            Self::Circle => "Сделай круг",
            Self::Come => "Ко мне",
            Self::Stay => "Стой",
            Self::Dash => "Быстро",
            Self::Up => "Вверх",
            Self::Down => "Вниз",
            Self::Left => "Влево",
            Self::Right => "Вправо",
            Self::Sleep => "Спать",
            Self::Wake => "Проснись",
            Self::Blink => "Моргни",
            Self::Look => "Смотри",
            Self::Bow => "Поклонись",
            Self::Shake => "Отряхнись",
            Self::Stretch => "Потянись",
            Self::Play => "Играй",
            Self::Home => "Домой",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingCue {
    Name,
    Quiet,
    Other,
    Command(CueKind),
}

impl TrainingCue {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => CueKind::Name.label(),
            Self::Quiet => CueKind::Quiet.label(),
            Self::Other => "Посторонние слова",
            Self::Command(cue) => cue.label(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectralFrameV1 {
    /// Twelve cepstral coefficients followed by their first differences.
    pub values: [f32; FEATURE_DIM],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueTemplateV1 {
    pub frames: Vec<SpectralFrameV1>,
    pub duration_ms: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueClassModelV1 {
    pub cue: CueKind,
    pub examples: Vec<CueTemplateV1>,
    /// `None` means positives exist but negative enrollment is not sufficient.
    pub acceptance_distance: Option<f32>,
    pub required_negative_margin: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueModelV1 {
    pub schema_version: u32,
    pub name: Option<CueClassModelV1>,
    pub quiet: Option<CueClassModelV1>,
    /// Explicit local counterexamples: room sounds, unrelated speech, and noise.
    pub other_examples: Vec<CueTemplateV1>,
    #[serde(default)]
    pub commands: Vec<CueClassModelV1>,
}

impl Default for CueModelV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl CueModelV1 {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            schema_version: CUE_MODEL_SCHEMA_VERSION,
            name: None,
            quiet: None,
            other_examples: Vec::new(),
            commands: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), CueModelValidationError> {
        if self.schema_version != CUE_MODEL_SCHEMA_VERSION {
            return Err(CueModelValidationError::UnsupportedSchema {
                found: self.schema_version,
                expected: CUE_MODEL_SCHEMA_VERSION,
            });
        }
        if self.other_examples.len() > MAX_TEMPLATES_PER_CLASS {
            return Err(CueModelValidationError::TooManyTemplates);
        }
        for template in &self.other_examples {
            validate_template(template)?;
        }
        if self.commands.len() > 19
            || self.commands.iter().enumerate().any(|(i, c)| {
                matches!(c.cue, CueKind::Name | CueKind::Quiet)
                    || self.commands[..i].iter().any(|p| p.cue == c.cue)
            })
        {
            return Err(CueModelValidationError::WrongClassSlot);
        }
        for expected in CueKind::ALL {
            let Some(class) = self.class(expected) else {
                continue;
            };
            if class.cue != expected {
                return Err(CueModelValidationError::WrongClassSlot);
            }
            if class.examples.len() > MAX_TEMPLATES_PER_CLASS {
                return Err(CueModelValidationError::TooManyTemplates);
            }
            for template in &class.examples {
                validate_template(template)?;
            }
            if !class.required_negative_margin.is_finite()
                || !(MIN_NEGATIVE_MARGIN..=2.0).contains(&class.required_negative_margin)
            {
                return Err(CueModelValidationError::InvalidThreshold);
            }
            if let Some(threshold) = class.acceptance_distance
                && (!threshold.is_finite() || !(0.01..=16.0).contains(&threshold))
            {
                return Err(CueModelValidationError::InvalidThreshold);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn is_ready(&self, cue: CueKind) -> bool {
        self.class(cue).is_some_and(|class| {
            class.examples.len() >= REQUIRED_POSITIVE_EXAMPLES
                && class.acceptance_distance.is_some()
                && self.other_examples.len() >= REQUIRED_OTHER_EXAMPLES
        })
    }

    pub fn class(&self, cue: CueKind) -> Option<&CueClassModelV1> {
        match cue {
            CueKind::Name => self.name.as_ref(),
            CueKind::Quiet => self.quiet.as_ref(),
            _ => self.commands.iter().find(|c| c.cue == cue),
        }
    }

    fn replace_class(&mut self, cue: CueKind, examples: Vec<CueTemplateV1>) {
        let mut combined = self
            .class(cue)
            .map_or_else(Vec::new, |c| c.examples.clone());
        combined.extend(examples);
        if combined.len() > MAX_TEMPLATES_PER_CLASS {
            combined.drain(..combined.len() - MAX_TEMPLATES_PER_CLASS);
        }
        let examples = combined;
        let value = CueClassModelV1 {
            cue,
            examples,
            acceptance_distance: None,
            required_negative_margin: MIN_NEGATIVE_MARGIN,
        };
        match cue {
            CueKind::Name => self.name = Some(value),
            CueKind::Quiet => self.quiet = Some(value),
            _ => {
                self.commands.retain(|c| c.cue != cue);
                self.commands.push(value);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum CueModelValidationError {
    #[error("unsupported cue model schema {found}; expected {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("cue model contains too many templates")]
    TooManyTemplates,
    #[error("cue model contains an invalid feature sequence")]
    InvalidTemplate,
    #[error("cue model contains a class in the wrong slot")]
    WrongClassSlot,
    #[error("cue model contains an invalid threshold")]
    InvalidThreshold,
}

fn validate_template(template: &CueTemplateV1) -> Result<(), CueModelValidationError> {
    if template.frames.len() < 2
        || template.frames.len() > MAX_FEATURE_FRAMES
        || !(120..=2_500).contains(&template.duration_ms)
        || template.frames.iter().any(|frame| {
            frame
                .values
                .iter()
                .any(|value| !value.is_finite() || value.abs() > MAX_FEATURE_ABS)
        })
    {
        return Err(CueModelValidationError::InvalidTemplate);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainingProgress {
    pub cue: TrainingCue,
    pub accepted: u8,
    pub required: u8,
    /// The final example is an explicit enrollment holdout, not used to fit the
    /// threshold. The UI should still ask the user to run Test after enrollment.
    pub held_out_required: bool,
}

#[derive(Debug, Clone)]
pub struct CueTrainer {
    cue: TrainingCue,
    base: CueModelV1,
    examples: Vec<CueTemplateV1>,
}

impl CueTrainer {
    pub fn begin(cue: TrainingCue, base: CueModelV1) -> Result<Self, EnrollmentError> {
        base.validate()?;
        Ok(Self {
            cue,
            base,
            examples: Vec::new(),
        })
    }

    /// Converts a transient 16 kHz mono recording directly into features. The
    /// supplied samples are not retained by the trainer or its serialized model.
    pub fn accept_segment(
        &mut self,
        samples_16khz: &[f32],
    ) -> Result<TrainingProgress, EnrollmentError> {
        if self.examples.len() >= self.required() {
            return Err(EnrollmentError::AlreadyComplete);
        }
        let require_voice = self.cue != TrainingCue::Other;
        let features = extract_features(samples_16khz, require_voice)?;
        self.accept_feature_segment(features)?;
        Ok(self.progress())
    }

    pub(crate) fn accept_feature_segment(
        &mut self,
        features: CueTemplateV1,
    ) -> Result<(), EnrollmentError> {
        validate_template(&features)?;
        if self.examples.len() >= self.required() {
            return Err(EnrollmentError::AlreadyComplete);
        }
        self.examples.push(features);
        Ok(())
    }

    #[must_use]
    pub fn progress(&self) -> TrainingProgress {
        TrainingProgress {
            cue: self.cue,
            accepted: self.examples.len() as u8,
            required: self.required() as u8,
            held_out_required: true,
        }
    }

    #[must_use]
    pub fn cancel(self) -> CueModelV1 {
        self.base
    }

    /// Validates the last example as a holdout before atomically producing a
    /// candidate replacement. On any error callers retain the prior model.
    pub fn finalize(self) -> Result<CueModelV1, EnrollmentError> {
        if self.examples.len() != self.required() {
            return Err(EnrollmentError::NeedMoreExamples {
                accepted: self.examples.len(),
                required: self.required(),
            });
        }
        validate_enrollment_cohesion(&self.examples, self.cue == TrainingCue::Other)?;

        let mut candidate = self.base.clone();
        match self.cue {
            TrainingCue::Name => candidate.replace_class(CueKind::Name, self.examples),
            TrainingCue::Quiet => candidate.replace_class(CueKind::Quiet, self.examples),
            TrainingCue::Command(cue) => candidate.replace_class(cue, self.examples),
            TrainingCue::Other => {
                candidate.other_examples.extend(self.examples);
                if candidate.other_examples.len() > MAX_TEMPLATES_PER_CLASS {
                    candidate
                        .other_examples
                        .drain(..candidate.other_examples.len() - MAX_TEMPLATES_PER_CLASS);
                }
            }
        }
        let changed = match self.cue {
            TrainingCue::Name => Some(CueKind::Name),
            TrainingCue::Quiet => Some(CueKind::Quiet),
            TrainingCue::Command(cue) => Some(cue),
            TrainingCue::Other => None,
        };
        calibrate_model(&mut candidate, changed)?;
        candidate.validate()?;
        Ok(candidate)
    }

    pub(crate) fn retry_last(&mut self) {
        self.examples.pop();
    }

    fn required(&self) -> usize {
        match self.cue {
            TrainingCue::Name | TrainingCue::Quiet | TrainingCue::Command(_) => {
                REQUIRED_POSITIVE_EXAMPLES
            }
            TrainingCue::Other => REQUIRED_OTHER_EXAMPLES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum EnrollmentError {
    #[error(transparent)]
    InvalidModel(#[from] CueModelValidationError),
    #[error("recording is silent or too quiet")]
    TooQuiet,
    #[error("recording duration is outside 180 ms to 2.5 seconds")]
    BadDuration,
    #[error("recording does not contain enough voice-like frames")]
    NotVoiceLike,
    #[error("recording resembles a short impulse rather than a cue")]
    TransientImpulse,
    #[error("training already has all required examples")]
    AlreadyComplete,
    #[error("need {required} examples; {accepted} accepted")]
    NeedMoreExamples { accepted: usize, required: usize },
    #[error("the held-out example differs too much from the enrollment set")]
    HeldOutMismatch,
    #[error("examples are too inconsistent for a safe cue model")]
    InconsistentExamples,
    #[error("positive and negative examples are not separable")]
    NotSeparable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueRejectReason {
    NoReadyModel,
    InvalidSegment,
    TooDistant,
    Ambiguous,
    OwnOutput,
    LowConfidenceBargeIn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueDecision {
    pub cue: Option<CueKind>,
    pub accepted: bool,
    pub confidence: f32,
    pub distance: f32,
    pub negative_distance: f32,
    pub margin: f32,
    pub reason: Option<CueRejectReason>,
}

impl CueDecision {
    fn rejected(reason: CueRejectReason) -> Self {
        Self {
            cue: None,
            accepted: false,
            confidence: 0.0,
            distance: f32::MAX,
            negative_distance: f32::MAX,
            margin: 0.0,
            reason: Some(reason),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SegmentEvidence {
    pub samples: Vec<f32>,
    pub duration_ms: u16,
    pub voice_likeness: f32,
    pub output_active_fraction: f32,
    pub onset_rms: f32,
    pub onset_output_rms: f32,
}

pub(crate) fn classify_segment(model: &CueModelV1, evidence: &SegmentEvidence) -> CueDecision {
    let Ok(query) = extract_features(&evidence.samples, true) else {
        return CueDecision::rejected(CueRejectReason::InvalidSegment);
    };
    let mut candidates = Vec::with_capacity(2);
    for cue in CueKind::ALL {
        let Some(class) = model.class(cue) else {
            continue;
        };
        let Some(threshold) = class.acceptance_distance else {
            continue;
        };
        let distance = nearest_distance(&query, &class.examples);
        candidates.push((cue, distance, threshold, class.required_negative_margin));
    }
    if candidates.is_empty() {
        return CueDecision::rejected(CueRejectReason::NoReadyModel);
    }
    candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
    let (cue, distance, threshold, required_margin) = candidates[0];
    let mut negative_distance = nearest_distance(&query, &model.other_examples);
    if let Some((_, runner_up, _, _)) = candidates.get(1) {
        negative_distance = negative_distance.min(*runner_up);
    }
    let margin = negative_distance - distance;
    let distance_confidence = (1.0 - distance / threshold.max(1.0e-4)).clamp(0.0, 1.0);
    let margin_confidence = (margin / (required_margin * 3.0)).clamp(0.0, 1.0);
    let confidence = (0.65 * distance_confidence + 0.35 * margin_confidence).clamp(0.0, 1.0);

    let reason = if distance > threshold {
        Some(CueRejectReason::TooDistant)
    } else if margin < required_margin {
        Some(CueRejectReason::Ambiguous)
    } else if evidence.output_active_fraction > 0.08 && cue == CueKind::Name {
        Some(CueRejectReason::OwnOutput)
    } else if evidence.output_active_fraction > 0.08
        && (confidence < 0.72 || evidence.onset_rms < evidence.onset_output_rms * 2.5 + 0.008)
    {
        Some(CueRejectReason::LowConfidenceBargeIn)
    } else {
        None
    };
    CueDecision {
        cue: Some(cue),
        accepted: reason.is_none(),
        confidence,
        distance,
        negative_distance,
        margin,
        reason,
    }
}

fn calibrate_model(
    model: &mut CueModelV1,
    changed: Option<CueKind>,
) -> Result<(), EnrollmentError> {
    for cue in CueKind::ALL {
        // Existing class thresholds stay valid; live classification still checks
        // every competing class. Avoid quadratic retraining of all old words
        // when the owner merely adds another five examples of one phrase.
        if changed.is_some_and(|changed| changed != cue) {
            continue;
        }
        let Some(class) = model.class(cue) else {
            continue;
        };
        if class.examples.len() < REQUIRED_POSITIVE_EXAMPLES
            || model.other_examples.len() < REQUIRED_OTHER_EXAMPLES
        {
            continue;
        }
        let positives = class.examples.clone();
        let mut negatives = model.other_examples.clone();
        for other in CueKind::ALL.into_iter().filter(|other| *other != cue) {
            if let Some(other) = model.class(other) {
                negatives.extend(other.examples.iter().cloned());
            }
        }
        // The last sample passed a separate holdout check above. Fit only on the
        // first four so enrollment cannot simply memorize its own validation.
        let mut within = leave_one_out_distances(&positives[..positives.len() - 1]);
        within.sort_by(f32::total_cmp);
        let fitted = within[within.len().saturating_sub(2)].max(0.015) * 1.55 + 0.012;
        let nearest_negative = positives
            .iter()
            .map(|positive| nearest_distance(positive, &negatives))
            .fold(f32::INFINITY, f32::min);
        if !nearest_negative.is_finite() || nearest_negative <= fitted + MIN_NEGATIVE_MARGIN {
            return Err(EnrollmentError::NotSeparable);
        }
        let threshold = fitted.min(nearest_negative - MIN_NEGATIVE_MARGIN);
        let target = match cue {
            CueKind::Name => model.name.as_mut(),
            CueKind::Quiet => model.quiet.as_mut(),
            _ => model.commands.iter_mut().find(|c| c.cue == cue),
        }
        .expect("class was checked above");
        target.acceptance_distance = Some(threshold);
        target.required_negative_margin = MIN_NEGATIVE_MARGIN;
    }
    Ok(())
}

fn validate_enrollment_cohesion(
    examples: &[CueTemplateV1],
    allow_diverse: bool,
) -> Result<(), EnrollmentError> {
    let training = &examples[..examples.len() - 1];
    let holdout = examples.last().expect("required example count is non-zero");
    let mut pair_distances = Vec::new();
    for left in 0..training.len() {
        for right in left + 1..training.len() {
            pair_distances.push(dtw_distance(&training[left], &training[right]));
        }
    }
    pair_distances.sort_by(f32::total_cmp);
    let median = pair_distances[pair_distances.len() / 2].max(0.02);
    let held_out = nearest_distance(holdout, training);
    let multiplier = if allow_diverse { 5.0 } else { 2.6 };
    if held_out > median * multiplier + 0.08 {
        return Err(EnrollmentError::HeldOutMismatch);
    }
    if !allow_diverse && pair_distances.last().copied().unwrap_or(0.0) > median * 4.0 + 0.12 {
        return Err(EnrollmentError::InconsistentExamples);
    }
    Ok(())
}

fn leave_one_out_distances(examples: &[CueTemplateV1]) -> Vec<f32> {
    examples
        .iter()
        .enumerate()
        .map(|(index, example)| {
            examples
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, other)| dtw_distance(example, other))
                .fold(f32::INFINITY, f32::min)
        })
        .collect()
}

fn nearest_distance(query: &CueTemplateV1, examples: &[CueTemplateV1]) -> f32 {
    examples
        .iter()
        .map(|example| dtw_distance(query, example))
        .fold(f32::INFINITY, f32::min)
}

fn dtw_distance(left: &CueTemplateV1, right: &CueTemplateV1) -> f32 {
    let n = left.frames.len();
    let m = right.frames.len();
    if n == 0 || m == 0 || n.abs_diff(m) > n.max(m) / 2 + 2 {
        return f32::INFINITY;
    }
    let band = n.abs_diff(m).max((n.max(m) as f32 * 0.18).ceil() as usize);
    let mut previous = vec![f32::INFINITY; m + 1];
    let mut current = vec![f32::INFINITY; m + 1];
    previous[0] = 0.0;
    for row in 1..=n {
        current.fill(f32::INFINITY);
        let center = row * m / n;
        let start = center.saturating_sub(band).max(1);
        let end = (center + band).min(m);
        for column in start..=end {
            let local = frame_distance(&left.frames[row - 1], &right.frames[column - 1]);
            current[column] = local
                + previous[column]
                    .min(current[column - 1])
                    .min(previous[column - 1]);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[m] / (n + m) as f32
}

fn frame_distance(left: &SpectralFrameV1, right: &SpectralFrameV1) -> f32 {
    let sum = left
        .values
        .iter()
        .zip(&right.values)
        .map(|(a, b)| {
            let delta = a - b;
            delta * delta
        })
        .sum::<f32>();
    (sum / FEATURE_DIM as f32).sqrt()
}

pub(crate) fn extract_features(
    samples: &[f32],
    require_voice: bool,
) -> Result<CueTemplateV1, EnrollmentError> {
    if samples.len() < MIN_CUE_SAMPLES || samples.len() > MAX_CUE_SAMPLES {
        return Err(EnrollmentError::BadDuration);
    }
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err(EnrollmentError::TooQuiet);
    }
    let rms =
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt();
    if rms < 0.0025 {
        return Err(EnrollmentError::TooQuiet);
    }
    let peak = samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0, f32::max);
    if peak / rms.max(1.0e-5) > 12.0 && samples.len() < TARGET_SAMPLE_RATE as usize / 3 {
        return Err(EnrollmentError::TransientImpulse);
    }

    let mut base = Vec::new();
    let mut voice_frames = 0usize;
    for offset in (0..=samples.len() - WINDOW_SAMPLES).step_by(HOP_SAMPLES) {
        let window = &samples[offset..offset + WINDOW_SAMPLES];
        let window_rms = (window.iter().map(|sample| sample * sample).sum::<f32>()
            / WINDOW_SAMPLES as f32)
            .sqrt();
        let crossings = window
            .windows(2)
            .filter(|pair| pair[0].is_sign_positive() != pair[1].is_sign_positive())
            .count() as f32
            / (WINDOW_SAMPLES - 1) as f32;
        if window_rms > rms * 0.24 && (0.008..=0.38).contains(&crossings) {
            voice_frames += 1;
        }
        base.push(mfcc_frame(window));
    }
    if base.len() < 2 {
        return Err(EnrollmentError::BadDuration);
    }
    if require_voice && voice_frames as f32 / (base.len() as f32) < 0.34 {
        return Err(EnrollmentError::NotVoiceLike);
    }

    let mut frames = Vec::with_capacity(base.len());
    for index in 0..base.len() {
        let before = &base[index.saturating_sub(2)];
        let after = &base[(index + 2).min(base.len() - 1)];
        let mut values = [0.0; FEATURE_DIM];
        for coefficient in 0..12 {
            values[coefficient] = base[index][coefficient].clamp(-MAX_FEATURE_ABS, MAX_FEATURE_ABS);
            values[12 + coefficient] = ((after[coefficient] - before[coefficient]) * 0.5)
                .clamp(-MAX_FEATURE_ABS, MAX_FEATURE_ABS);
        }
        frames.push(SpectralFrameV1 { values });
    }
    Ok(CueTemplateV1 {
        frames,
        duration_ms: ((samples.len() as u64 * 1_000) / TARGET_SAMPLE_RATE as u64)
            .min(u16::MAX as u64) as u16,
    })
}

fn mfcc_frame(samples: &[f32]) -> [f32; 12] {
    let mut real = [0.0f32; FFT_SIZE];
    let mut imaginary = [0.0f32; FFT_SIZE];
    let denominator = (WINDOW_SAMPLES - 1) as f32;
    for index in 0..WINDOW_SAMPLES {
        let emphasized =
            samples[index] - 0.97 * samples.get(index.wrapping_sub(1)).copied().unwrap_or(0.0);
        let hamming = 0.54 - 0.46 * (std::f32::consts::TAU * index as f32 / denominator).cos();
        real[index] = emphasized * hamming;
    }
    fft_in_place(&mut real, &mut imaginary);
    let mut power = [0.0f32; FFT_SIZE / 2 + 1];
    for index in 0..power.len() {
        power[index] =
            (real[index] * real[index] + imaginary[index] * imaginary[index]) / FFT_SIZE as f32;
    }

    let low_mel = hz_to_mel(80.0);
    let high_mel = hz_to_mel(TARGET_SAMPLE_RATE as f32 * 0.48);
    let mut bins = [0usize; MEL_FILTERS + 2];
    for (index, bin) in bins.iter_mut().enumerate() {
        let mel = low_mel + (high_mel - low_mel) * index as f32 / (MEL_FILTERS + 1) as f32;
        let hz = mel_to_hz(mel);
        *bin = ((FFT_SIZE + 1) as f32 * hz / TARGET_SAMPLE_RATE as f32)
            .floor()
            .clamp(0.0, (FFT_SIZE / 2) as f32) as usize;
    }
    let mut energies = [0.0f32; MEL_FILTERS];
    for filter in 0..MEL_FILTERS {
        let (left, center, right) = (bins[filter], bins[filter + 1], bins[filter + 2]);
        for (bin, value) in power.iter().enumerate().take(center).skip(left) {
            energies[filter] += value * (bin - left) as f32 / (center - left).max(1) as f32;
        }
        for (bin, value) in power.iter().enumerate().take(right + 1).skip(center) {
            energies[filter] +=
                value * (right.saturating_sub(bin)) as f32 / (right - center).max(1) as f32;
        }
        energies[filter] = (energies[filter] + 1.0e-10).ln();
    }
    let mut coefficients = [0.0f32; 12];
    for (coefficient, value) in coefficients.iter_mut().enumerate() {
        let order = coefficient + 1;
        *value = energies
            .iter()
            .enumerate()
            .map(|(index, energy)| {
                energy
                    * (std::f32::consts::PI * order as f32 * (index as f32 + 0.5)
                        / MEL_FILTERS as f32)
                        .cos()
            })
            .sum::<f32>()
            / MEL_FILTERS as f32;
    }
    coefficients
}

fn fft_in_place(real: &mut [f32; FFT_SIZE], imaginary: &mut [f32; FFT_SIZE]) {
    let mut reversed = 0usize;
    for index in 1..FFT_SIZE {
        let mut bit = FFT_SIZE >> 1;
        while reversed & bit != 0 {
            reversed ^= bit;
            bit >>= 1;
        }
        reversed ^= bit;
        if index < reversed {
            real.swap(index, reversed);
            imaginary.swap(index, reversed);
        }
    }
    let mut length = 2;
    while length <= FFT_SIZE {
        let angle = -std::f32::consts::TAU / length as f32;
        let step_real = angle.cos();
        let step_imaginary = angle.sin();
        for start in (0..FFT_SIZE).step_by(length) {
            let mut twiddle_real = 1.0;
            let mut twiddle_imaginary = 0.0;
            for offset in 0..length / 2 {
                let even = start + offset;
                let odd = even + length / 2;
                let odd_real = real[odd] * twiddle_real - imaginary[odd] * twiddle_imaginary;
                let odd_imaginary = real[odd] * twiddle_imaginary + imaginary[odd] * twiddle_real;
                real[odd] = real[even] - odd_real;
                imaginary[odd] = imaginary[even] - odd_imaginary;
                real[even] += odd_real;
                imaginary[even] += odd_imaginary;
                let next_real = twiddle_real * step_real - twiddle_imaginary * step_imaginary;
                twiddle_imaginary = twiddle_real * step_imaginary + twiddle_imaginary * step_real;
                twiddle_real = next_real;
            }
        }
        length <<= 1;
    }
}

fn hz_to_mel(hz: f32) -> f32 {
    2_595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0f32.powf(mel / 2_595.0) - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spoken_like(seed: usize, base: f32, sweep: f32) -> Vec<f32> {
        let length = (0.58 * TARGET_SAMPLE_RATE as f32) as usize + seed * 91;
        (0..length)
            .map(|index| {
                let time = index as f32 / TARGET_SAMPLE_RATE as f32;
                let phase = std::f32::consts::TAU * (base * time + sweep * time * time * 0.5);
                let envelope = ((time / 0.07).min(1.0)
                    * ((length - index) as f32 / (TARGET_SAMPLE_RATE as f32 * 0.09)).min(1.0))
                .clamp(0.0, 1.0);
                let tremolo =
                    0.72 + 0.28 * (std::f32::consts::TAU * (3.1 + seed as f32 * 0.03) * time).sin();
                envelope
                    * tremolo
                    * (0.16 * phase.sin()
                        + 0.075 * (phase * 2.03 + seed as f32 * 0.02).sin()
                        + 0.035 * (phase * 3.11).sin())
            })
            .collect()
    }

    fn train(model: CueModelV1, cue: TrainingCue, base: f32, sweep: f32) -> CueModelV1 {
        let mut trainer = CueTrainer::begin(cue, model).unwrap();
        for seed in 0..5 {
            trainer
                .accept_segment(&spoken_like(seed, base + seed as f32 * 0.7, sweep))
                .unwrap();
        }
        trainer.finalize().unwrap()
    }

    #[test]
    fn incremental_enrollment_preserves_other_commands_and_keeps_forty_examples() {
        fn batch(model: CueModelV1, cue: TrainingCue, value: f32) -> CueModelV1 {
            let mut trainer = CueTrainer::begin(cue, model).unwrap();
            for i in 0..5 {
                trainer
                    .accept_feature_segment(CueTemplateV1 {
                        frames: vec![
                            SpectralFrameV1 {
                                values: [value + i as f32 * 0.001; FEATURE_DIM]
                            };
                            4
                        ],
                        duration_ms: 300,
                    })
                    .unwrap();
            }
            trainer.finalize().unwrap()
        }
        let mut model = batch(CueModelV1::new(), TrainingCue::Other, 20.0);
        for (i, cue) in CueKind::ALL.into_iter().enumerate() {
            model = batch(model, TrainingCue::Command(cue), i as f32 * 0.4);
            assert!(model.is_ready(cue));
        }
        let sit = model.class(CueKind::Sit).unwrap().examples.clone();
        for _ in 0..8 {
            model = batch(model, TrainingCue::Command(CueKind::Name), 0.0);
        }
        assert_eq!(model.name.as_ref().unwrap().examples.len(), 40);
        assert_eq!(model.class(CueKind::Sit).unwrap().examples, sit);
        let restored: CueModelV1 =
            serde_json::from_str(&serde_json::to_string(&model).unwrap()).unwrap();
        assert_eq!(restored, model);
        restored.validate().unwrap();
    }

    #[test]
    fn synthetic_waveforms_train_distinct_name_and_quiet_cues() {
        let mut other = CueTrainer::begin(TrainingCue::Other, CueModelV1::new()).unwrap();
        for seed in 0..5 {
            other
                .accept_segment(&spoken_like(seed, 510.0 + seed as f32 * 9.0, -35.0))
                .unwrap();
        }
        let model = train(other.finalize().unwrap(), TrainingCue::Name, 145.0, 105.0);
        let model = train(model, TrainingCue::Quiet, 285.0, -72.0);
        assert!(model.is_ready(CueKind::Name));
        assert!(model.is_ready(CueKind::Quiet));

        let name = spoken_like(6, 148.0, 105.0);
        let quiet = spoken_like(6, 289.0, -72.0);
        let name_decision = classify_segment(&model, &SegmentEvidence::clean(name, 580));
        let quiet_decision = classify_segment(&model, &SegmentEvidence::clean(quiet, 580));
        assert!(name_decision.accepted, "{name_decision:?}");
        assert_eq!(name_decision.cue, Some(CueKind::Name));
        assert!(quiet_decision.accepted, "{quiet_decision:?}");
        assert_eq!(quiet_decision.cue, Some(CueKind::Quiet));
    }

    #[test]
    fn silence_impulse_and_unknown_waveform_are_rejected() {
        assert!(matches!(
            extract_features(&vec![0.0; 8_000], true),
            Err(EnrollmentError::TooQuiet)
        ));
        let mut clap = vec![0.0; 2_900];
        clap[900] = 1.0;
        assert!(matches!(
            extract_features(&clap, true),
            Err(EnrollmentError::TransientImpulse | EnrollmentError::NotVoiceLike)
        ));

        let mut other = CueTrainer::begin(TrainingCue::Other, CueModelV1::new()).unwrap();
        for seed in 0..5 {
            other
                .accept_segment(&spoken_like(seed, 500.0, 10.0))
                .unwrap();
        }
        let model = train(other.finalize().unwrap(), TrainingCue::Name, 130.0, 90.0);
        let unknown = spoken_like(8, 345.0, 210.0);
        let decision = classify_segment(&model, &SegmentEvidence::clean(unknown, 620));
        assert!(!decision.accepted, "{decision:?}");
    }

    #[test]
    fn serialized_model_is_features_only_and_validation_is_bounded() {
        let mut trainer = CueTrainer::begin(TrainingCue::Other, CueModelV1::new()).unwrap();
        for seed in 0..5 {
            trainer
                .accept_segment(&spoken_like(seed, 480.0, 15.0))
                .unwrap();
        }
        let model = trainer.finalize().unwrap();
        let json = serde_json::to_string(&model).unwrap();
        assert!(!json.contains("samples"));
        assert!(!json.contains("pcm"));
        let restored: CueModelV1 = serde_json::from_str(&json).unwrap();
        restored.validate().unwrap();

        let mut oversized = restored;
        let template = oversized.other_examples[0].clone();
        oversized.other_examples = vec![template; MAX_TEMPLATES_PER_CLASS + 1];
        assert_eq!(
            oversized.validate(),
            Err(CueModelValidationError::TooManyTemplates)
        );
    }

    impl SegmentEvidence {
        fn clean(samples: Vec<f32>, duration_ms: u16) -> Self {
            Self {
                samples,
                duration_ms,
                voice_likeness: 0.9,
                output_active_fraction: 0.0,
                onset_rms: 0.1,
                onset_output_rms: 0.0,
            }
        }
    }
}

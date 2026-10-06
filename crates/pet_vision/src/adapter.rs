//! Parameter-efficient visual-connector adaptation. The immutable ONNX base is
//! retained; these learned weights run natively between encoder and decoder.
use crate::{MODEL_REVISION, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterValidation {
    pub training_examples: usize,
    pub validation_examples: usize,
    pub baseline_nll: f32,
    pub candidate_nll: f32,
    pub baseline_accuracy: f32,
    pub candidate_accuracy: f32,
    pub approved: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorAdapter {
    pub schema_version: u32,
    pub model_revision: String,
    pub rank: usize,
    pub down: Vec<f32>,
    pub up: Vec<f32>,
    pub max_relative_change: f32,
    pub validation: AdapterValidation,
}
impl ConnectorAdapter {
    pub fn load_checked(path: &Path, expected: &str) -> Result<Self> {
        use sha2::{Digest, Sha256};
        if std::fs::metadata(path)?.len() > 2_000_000 {
            return Err("Adapter exceeds size limit".into());
        }
        let bytes = std::fs::read(path)?;
        if expected.len() != 64 || format!("{:x}", Sha256::digest(&bytes)) != expected {
            return Err("Adapter validation hash mismatch".into());
        }
        let a: Self = serde_json::from_slice(&bytes)?;
        if !a.valid() {
            return Err("Adapter is not valid".into());
        }
        Ok(a)
    }
    pub fn load(path: &Path) -> Result<Self> {
        if std::fs::metadata(path)?.len() > 2_000_000 {
            return Err("Adapter exceeds size limit".into());
        }
        let x: Self = serde_json::from_reader(std::fs::File::open(path)?)?;
        if !x.valid() {
            return Err("Invalid, unvalidated or incompatible visual adapter".into());
        }
        Ok(x)
    }
    pub fn valid(&self) -> bool {
        let v = &self.validation;
        self.schema_version == 1
            && self.model_revision == MODEL_REVISION
            && (1..=16).contains(&self.rank)
            && self.down.len() == 1024 * self.rank
            && self.up.len() == 1024 * self.rank
            && self
                .down
                .iter()
                .chain(&self.up)
                .all(|x| x.is_finite() && x.abs() <= 2.0)
            && self.max_relative_change.is_finite()
            && (0.0..=0.08).contains(&self.max_relative_change)
            && v.approved
            && v.training_examples >= 8
            && v.validation_examples >= 8
            && [
                v.baseline_nll,
                v.candidate_nll,
                v.baseline_accuracy,
                v.candidate_accuracy,
            ]
            .iter()
            .all(|x| x.is_finite() && *x >= 0.0)
            && v.candidate_nll <= v.baseline_nll
            && v.candidate_accuracy >= v.baseline_accuracy
            && v.candidate_accuracy <= 1.0
            && v.baseline_accuracy <= 1.0
    }
    pub fn apply(&self, features: &mut [f32]) -> Result<()> {
        if !self.valid()
            || features.is_empty()
            || !features.len().is_multiple_of(1024)
            || features.len() > 256 * 1024
            || features.iter().any(|v| !v.is_finite())
        {
            return Err("Invalid feature/adapter tensor".into());
        }
        for row in features.chunks_exact_mut(1024) {
            let mut latent = [0.0_f32; 16];
            for (k, z) in latent.iter_mut().enumerate().take(self.rank) {
                *z = row
                    .iter()
                    .zip(&self.down[k * 1024..(k + 1) * 1024])
                    .map(|(a, b)| a * b)
                    .sum();
            }
            let mut delta = [0.0_f32; 1024];
            for (j, d) in delta.iter_mut().enumerate() {
                *d = (0..self.rank)
                    .map(|k| self.up[j * self.rank + k] * latent[k])
                    .sum();
            }
            let source_norm = row.iter().map(|v| v * v).sum::<f32>().sqrt();
            let delta_norm = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
            if !delta_norm.is_finite() {
                return Err("Non-finite adapter output".into());
            }
            let gain = (self.max_relative_change * source_norm / (delta_norm + 1e-8)).min(1.0);
            for (v, d) in row.iter_mut().zip(delta) {
                *v += d * gain;
            }
        }
        Ok(())
    }
}
/// Private derived visual representations, not anonymized data. Kept only in
/// RAM by default; disk collection requires an explicit human label/consent.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingFeatures {
    pub schema_version: u32,
    pub model_revision: String,
    pub image_tokens: usize,
    pub features: Vec<f32>,
}
impl TrainingFeatures {
    pub fn valid(&self) -> bool {
        self.schema_version == 1
            && self.model_revision == MODEL_REVISION
            && (1..=256).contains(&self.image_tokens)
            && self.features.len() == self.image_tokens * 1024
            && self
                .features
                .iter()
                .all(|v| v.is_finite() && v.abs() < 1000.0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn adapter() -> ConnectorAdapter {
        ConnectorAdapter {
            schema_version: 1,
            model_revision: MODEL_REVISION.into(),
            rank: 1,
            down: vec![0.01; 1024],
            up: vec![0.1; 1024],
            max_relative_change: 0.03,
            validation: AdapterValidation {
                training_examples: 16,
                validation_examples: 8,
                baseline_nll: 1.0,
                candidate_nll: 0.9,
                baseline_accuracy: 0.5,
                candidate_accuracy: 0.6,
                approved: true,
            },
        }
    }
    #[test]
    fn bounded_real_weight_update_and_zero_strength_identity() {
        let mut a = adapter();
        let source = vec![1.0; 2048];
        let mut x = source.clone();
        a.apply(&mut x).unwrap();
        let change = (x
            .iter()
            .zip(&source)
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            / source.iter().map(|v| v * v).sum::<f32>())
        .sqrt();
        assert!(change > 0.001 && change <= 0.03001);
        a.max_relative_change = 0.0;
        let mut z = source.clone();
        a.apply(&mut z).unwrap();
        assert_eq!(source, z);
    }
    #[test]
    fn rejects_incompatible_unvalidated_nonfinite_and_oversized_adapter() {
        let mut a = adapter();
        a.validation.approved = false;
        assert!(!a.valid());
        a = adapter();
        a.up[0] = f32::NAN;
        assert!(!a.valid());
        a = adapter();
        a.model_revision = "different".into();
        assert!(!a.valid());
        a = adapter();
        a.rank = 17;
        assert!(!a.valid());
    }
    #[test]
    fn tensor_shape_and_nonfinite_inputs_rejected() {
        let a = adapter();
        assert!(a.apply(&mut vec![1.0; 1023]).is_err());
        assert!(a.apply(&mut vec![f32::INFINITY; 1024]).is_err());
    }
}

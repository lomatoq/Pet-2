//! Native verification of explicitly retained visual features, without images.
use pet_vision::{ConnectorAdapter, Result, TrainingFeatures, VisionEngine};
use std::path::PathBuf;
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    if a.len() < 3 {
        return Err("usage: vision_feature_probe MODEL DLL [--adapter FILE] FEATURE_JSON...".into());
    }
    let mut engine = VisionEngine::load(&PathBuf::from(&a[0]), &PathBuf::from(&a[1]))?;
    let mut i = 2;
    if a[i] == "--adapter" {
        engine.set_adapter(Some(ConnectorAdapter::load(&PathBuf::from(
            a.get(i + 1).ok_or("adapter path")?,
        ))?))?;
        i += 2;
    }
    for path in &a[i..] {
        if std::fs::metadata(path)?.len() > 4_000_000 {
            return Err("oversized feature example".into());
        }
        let sample: TrainingFeatures = serde_json::from_reader(std::fs::File::open(path)?)?;
        let prediction = engine.classify_features(&sample)?;
        println!(
            "{}",
            serde_json::json!({"features":path,"prediction":prediction,"accepted":prediction.trustworthy()})
        );
    }
    Ok(())
}

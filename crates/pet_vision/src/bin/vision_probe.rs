use pet_vision::{Result, VisionEngine};
use std::{path::PathBuf, time::Instant};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: vision_probe MODEL_DIR ONNX_RUNTIME_DLL IMAGE [IMAGE ...]".into());
    }
    let now = Instant::now();
    let mut engine = VisionEngine::load(&PathBuf::from(&args[0]), &PathBuf::from(&args[1]))?;
    eprintln!("native model load: {:.3}s", now.elapsed().as_secs_f64());
    for path in &args[2..] {
        let img = image::open(path)?.into_rgb8();
        let prediction = engine.classify(img.width(), img.height(), img.into_raw(), 256)?;
        println!(
            "{}",
            serde_json::json!({"image": path, "prediction": prediction, "accepted": prediction.trustworthy()})
        );
    }
    Ok(())
}

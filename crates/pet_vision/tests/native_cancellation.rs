//! Opt-in hardware test using the provisioned, hash-verified real model.
use pet_vision::{SceneKind, VisionEngine};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires the local pinned model pack, ONNX Runtime and a synthetic document fixture"]
fn native_inference_is_cancelled_on_revocation_and_the_engine_remains_usable() {
    let model = PathBuf::from(std::env::var_os("PET2_VISION_MODEL_DIR").expect("model directory"));
    let runtime =
        PathBuf::from(std::env::var_os("PET2_ONNX_RUNTIME_PATH").expect("runtime library"));
    let image = PathBuf::from(
        std::env::var_os("PET2_VISION_TEST_IMAGE").expect("synthetic document fixture"),
    );
    let mut engine = VisionEngine::load(&model, &runtime).expect("load verified native model");
    let rgb = image::open(&image).unwrap().into_rgb8();
    let epoch = Arc::new(AtomicU64::new(3));
    let revoked = Arc::clone(&epoch);
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        revoked.store(4, Ordering::Release);
    });
    let started = Instant::now();
    let result = engine.classify_with_cancellation(
        rgb.width(),
        rgb.height(),
        rgb.clone().into_raw(),
        256,
        Some((epoch, 3)),
    );
    cancel.join().unwrap();
    let elapsed = started.elapsed();
    eprintln!("native cancellation latency: {} ms", elapsed.as_millis());
    assert!(result.is_err(), "revoked inference returned an observation");
    assert!(
        elapsed < Duration::from_millis(2500),
        "cancellation did not interrupt native inference: {elapsed:?}"
    );
    let p = engine
        .classify(rgb.width(), rgb.height(), rgb.into_raw(), 256)
        .expect("reuse after cancellation");
    assert_eq!(p.kind, SceneKind::Document);
    assert!(
        p.trustworthy(),
        "document fixture no longer recognized: {p:?}"
    );
}

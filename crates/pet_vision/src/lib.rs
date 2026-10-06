//! Bounded, in-process visual-language sensing. No network, tools or text memory.
use ort::{
    session::{RunOptions, Session},
    value::{DynValue, Tensor},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tokenizers::Tokenizer;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const MODEL_REVISION: &str = "95c283d4497a56477a83177079fa6b7121abb1b1";
const QUESTION: &str = "What is visible? Reply with exactly one letter: A code editor, B text document, C photograph or video, D artwork or drawing canvas, E video game, F spreadsheet or chart, G empty desktop, H web page, I other or uncertain. Answer:";
const CLASSES: [SceneKind; 9] = [
    SceneKind::Code,
    SceneKind::Document,
    SceneKind::Media,
    SceneKind::Artwork,
    SceneKind::Game,
    SceneKind::Chart,
    SceneKind::Desktop,
    SceneKind::Web,
    SceneKind::Unknown,
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneKind {
    Code,
    Document,
    Media,
    Artwork,
    Game,
    Chart,
    Desktop,
    Web,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenePrediction {
    pub kind: SceneKind,
    /// Conditional label support, NOT calibrated real-world confidence.
    pub support: f32,
    pub margin: f32,
    pub label_mass: f32,
    pub elapsed_ms: f32,
    pub image_tokens: usize,
    pub signature: u64,
}
impl ScenePrediction {
    pub fn trustworthy(&self) -> bool {
        self.kind != SceneKind::Unknown
            && [self.support, self.margin, self.label_mass]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self.elapsed_ms >= 0.0
            && self.support >= 0.70
            && self.margin >= 0.25
            && self.label_mass >= 0.20
            && self.elapsed_ms.is_finite()
            && self.elapsed_ms < 8_000.0
    }
}

/// One owner thread, bounded CPU concurrency, no global Python or inference server.
pub struct VisionEngine {
    embedding: Session,
    encoder: Session,
    decoder: Session,
    tokenizer: Tokenizer,
    label_ids: [usize; 9],
}

#[derive(Deserialize)]
struct Manifest {
    model: String,
    revision: String,
    files: HashMap<String, FileDigest>,
}
#[derive(Deserialize)]
struct FileDigest {
    size: u64,
    sha256: String,
}

fn verify_model(root: &Path) -> Result<()> {
    let manifest: Manifest =
        serde_json::from_reader(File::open(root.join("model-manifest.json"))?)?;
    if manifest.revision != MODEL_REVISION || manifest.model != "LiquidAI/LFM2.5-VL-450M-ONNX" {
        return Err("Unrecognized model revision; install the pinned model pack".into());
    }
    // Never follow arbitrary paths supplied by a manifest.
    for name in [
        "tokenizer.json",
        "onnx/embed_tokens_fp16.onnx",
        "onnx/embed_tokens_fp16.onnx_data",
        "onnx/vision_encoder_q8.onnx",
        "onnx/vision_encoder_q8.onnx_data",
        "onnx/decoder_model_merged_q4.onnx",
        "onnx/decoder_model_merged_q4.onnx_data",
    ] {
        let spec = manifest.files.get(name).ok_or("Missing model hash")?;
        let mut file = File::open(root.join(name))?;
        if file.metadata()?.len() != spec.size {
            return Err(format!("Model size mismatch: {name}").into());
        }
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 65_536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        if format!("{:x}", hash.finalize()) != spec.sha256 {
            return Err(format!("Model checksum mismatch: {name}").into());
        }
    }
    Ok(())
}

impl VisionEngine {
    pub fn load(model: &Path, runtime_library: &Path) -> Result<Self> {
        verify_model(model)?;
        if !runtime_library.is_file() {
            return Err("Bundled ONNX Runtime library is missing".into());
        }
        ort::init_from(runtime_library)?
            .with_name("Pet2-local-vision")
            .commit();
        let session = |name: &str| -> Result<Session> {
            let builder = Session::builder()?;
            let builder = builder.with_intra_threads(2).map_err(|e| e.to_string())?;
            let mut builder = builder.with_inter_threads(1).map_err(|e| e.to_string())?;
            Ok(builder
                .commit_from_file(model.join("onnx").join(format!("{name}.onnx")))
                .map_err(|e| e.to_string())?)
        };
        let tokenizer = Tokenizer::from_file(model.join("tokenizer.json"))?;
        let mut label_ids = [0; 9];
        for (i, letter) in "ABCDEFGHI".chars().enumerate() {
            let encoding = tokenizer.encode(letter.to_string(), false)?;
            if encoding.len() != 1 {
                return Err("Unexpected label tokenization".into());
            }
            label_ids[i] = encoding.get_ids()[0] as usize;
            if label_ids[i] >= 65536 {
                return Err("Label token is outside the pinned decoder vocabulary".into());
            }
        }
        Ok(Self {
            embedding: session("embed_tokens_fp16")?,
            encoder: session("vision_encoder_q8")?,
            decoder: session("decoder_model_merged_q4")?,
            tokenizer,
            label_ids,
        })
    }

    pub fn classify(
        &mut self,
        width: u32,
        height: u32,
        rgb: Vec<u8>,
        side: u32,
    ) -> Result<ScenePrediction> {
        self.classify_with_cancellation(width, height, rgb, side, None)
    }

    pub fn classify_with_cancellation(
        &mut self,
        width: u32,
        height: u32,
        rgb: Vec<u8>,
        side: u32,
        permission: Option<(Arc<AtomicU64>, u64)>,
    ) -> Result<ScenePrediction> {
        if permission
            .as_ref()
            .is_some_and(|(epoch, token)| token & 1 == 0 || epoch.load(Ordering::Acquire) != *token)
        {
            return Err("Vision request was revoked".into());
        }
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || ![256, 384, 512].contains(&side)
            || rgb.len() != width as usize * height as usize * 3
        {
            return Err("Invalid or oversized vision input".into());
        }
        let started = Instant::now();
        let signature = image_signature(width, height, &rgb);
        let image = image::RgbImage::from_raw(width, height, rgb).ok_or("Invalid RGB buffer")?;
        let image =
            image::imageops::resize(&image, side, side, image::imageops::FilterType::Triangle);
        let n = side as usize / 16;
        let image_tokens = n * n / 4;
        let mut patches = Vec::with_capacity(n * n * 768);
        for py in 0..n {
            for px in 0..n {
                for dy in 0..16 {
                    for dx in 0..16 {
                        for value in image
                            .get_pixel((px * 16 + dx) as u32, (py * 16 + dy) as u32)
                            .0
                        {
                            patches.push(f32::from(value) / 127.5 - 1.0);
                        }
                    }
                }
            }
        }
        let options = Arc::new(RunOptions::new()?);
        let (done, wait) = mpsc::channel::<()>();
        let cancel = Arc::clone(&options);
        // The watchdog exits immediately on success/error; one inference at a time.
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                let revoked = permission.as_ref().is_some_and(|(epoch, token)| {
                    token & 1 == 0 || epoch.load(Ordering::Acquire) != *token
                });
                if revoked || Instant::now() >= deadline {
                    let _ = cancel.terminate();
                    break;
                }
                match wait.recv_timeout(Duration::from_millis(25)) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    _ => break,
                }
            }
        });
        let _completion = Completion(done);
        let features = {
            let out = self.encoder.run_with_options(
                ort::inputs! {
                    "pixel_values" => Tensor::from_array(([1, n*n, 768], patches))?,
                    "pixel_attention_mask" => Tensor::from_array(([1, n*n], vec![1_i64; n*n]))?,
                    "spatial_shapes" => Tensor::from_array(([1, 2], vec![n as i64, n as i64]))?
                },
                &options,
            )?;
            let (shape, data) = out["image_features"].try_extract_tensor::<f32>()?;
            if shape.as_ref() != [image_tokens as i64, 1024] || data.iter().any(|v| !v.is_finite())
            {
                return Err("Unexpected image feature tensor".into());
            }
            data.to_vec()
        };
        let prompt = format!(
            "<|startoftext|><|im_start|>user\n<|image_start|>{}<|image_end|>{QUESTION}<|im_end|>\n<|im_start|>assistant\n",
            "<image>".repeat(image_tokens)
        );
        let encoding = self.tokenizer.encode(prompt, false)?;
        let ids: Vec<i64> = encoding.get_ids().iter().map(|x| i64::from(*x)).collect();
        let count = ids.len();
        if count > 512 {
            return Err("Vision prompt exceeded its token budget".into());
        }
        let mut embeddings = {
            let out = self.embedding.run_with_options(
                ort::inputs! { "input_ids" => Tensor::from_array(([1, count], ids.clone()))? },
                &options,
            )?;
            out["inputs_embeds"].try_extract_tensor::<f32>()?.1.to_vec()
        };
        if embeddings.len() != count * 1024 {
            return Err("Unexpected embedding shape".into());
        }
        let mut visual_index = 0;
        for (index, token) in ids.iter().enumerate() {
            if *token == 396 {
                if visual_index >= image_tokens {
                    return Err("Too many image placeholders".into());
                }
                embeddings[index * 1024..(index + 1) * 1024]
                    .copy_from_slice(&features[visual_index * 1024..(visual_index + 1) * 1024]);
                visual_index += 1;
            }
        }
        if visual_index != image_tokens {
            return Err("Image placeholder mismatch".into());
        }
        let mut feeds: HashMap<String, DynValue> = HashMap::new();
        feeds.insert(
            "inputs_embeds".into(),
            Tensor::from_array(([1, count, 1024], embeddings))?.into_dyn(),
        );
        feeds.insert(
            "attention_mask".into(),
            Tensor::from_array(([1, count], vec![1_i64; count]))?.into_dyn(),
        );
        for input in self.decoder.inputs() {
            if input.name().starts_with("past_conv.") {
                feeds.insert(
                    input.name().into(),
                    Tensor::from_array(([1, 1024, 3], vec![0_f32; 3072]))?.into_dyn(),
                );
            } else if input.name().starts_with("past_key_values.") {
                feeds.insert(
                    input.name().into(),
                    Tensor::from_array(([1, 8, 0, 64], Vec::<f32>::new()))?.into_dyn(),
                );
            }
        }
        let out = self.decoder.run_with_options(feeds, &options)?;
        let (_, logits) = out["logits"].try_extract_tensor::<f32>()?;
        if logits.len() < 65536 {
            return Err("Unexpected decoder output".into());
        }
        let logits = &logits[logits.len() - 65536..];
        if logits.iter().any(|x| !x.is_finite()) {
            return Err("Non-finite decoder output".into());
        }
        let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let total: f32 = logits.iter().map(|x| (*x - max).exp()).sum();
        let values = self.label_ids.map(|id| (logits[id] - max).exp());
        let sum: f32 = values.iter().sum();
        let mut order: Vec<usize> = (0..9).collect();
        order.sort_by(|a, b| values[*b].total_cmp(&values[*a]));
        let first = order[0];
        Ok(ScenePrediction {
            kind: CLASSES[first],
            support: values[first] / sum.max(1e-20),
            margin: (values[first] - values[order[1]]) / sum.max(1e-20),
            label_mass: sum / total.max(1e-20),
            elapsed_ms: started.elapsed().as_secs_f32() * 1000.0,
            image_tokens,
            signature,
        })
    }
}

struct Completion(mpsc::Sender<()>);
impl Drop for Completion {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// A lossy 8x8 occupancy descriptor, not pixels or recoverable text.
pub fn image_signature(width: u32, height: u32, rgb: &[u8]) -> u64 {
    if width == 0 || height == 0 || rgb.len() != width as usize * height as usize * 3 {
        return 0;
    }
    let mut means = [0_f32; 64];
    for (i, mean) in means.iter_mut().enumerate() {
        let x = ((i % 8) as u32 * width / 8 + width / 16).min(width - 1);
        let y = ((i / 8) as u32 * height / 8 + height / 16).min(height - 1);
        let k = (y as usize * width as usize + x as usize) * 3;
        *mean = f32::from(rgb[k]) * 0.2126
            + f32::from(rgb[k + 1]) * 0.7152
            + f32::from(rgb[k + 2]) * 0.0722;
    }
    let average = means.iter().sum::<f32>() / 64.0;
    means
        .iter()
        .enumerate()
        .fold(0_u64, |bits, (i, v)| bits | (u64::from(*v > average) << i))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_or_ambiguous_output_never_becomes_a_fact() {
        let mut p = ScenePrediction {
            kind: SceneKind::Artwork,
            support: 0.56,
            margin: 0.36,
            label_mass: 0.99,
            elapsed_ms: 1000.,
            image_tokens: 64,
            signature: 0,
        };
        assert!(!p.trustworthy());
        p.support = 0.98;
        assert!(p.trustworthy());
        p.elapsed_ms = 9000.;
        assert!(!p.trustworthy());
        p.elapsed_ms = 1000.;
        for bad in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let mut invalid = p.clone();
            invalid.support = bad;
            assert!(!invalid.trustworthy());
            invalid = p.clone();
            invalid.margin = bad;
            assert!(!invalid.trustworthy());
            invalid = p.clone();
            invalid.label_mass = bad;
            assert!(!invalid.trustworthy());
        }
        p.elapsed_ms = -1.;
        assert!(!p.trustworthy());
        p.elapsed_ms = 1000.;
        p.kind = SceneKind::Unknown;
        assert!(!p.trustworthy());
    }
    #[test]
    fn malformed_images_do_not_panic() {
        assert_eq!(image_signature(0, 0, &[]), 0);
        assert_eq!(image_signature(8, 8, &[0; 2]), 0);
        assert_eq!(image_signature(8, 8, &[128; 192]), 0);
    }
}

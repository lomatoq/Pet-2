//! Opt-in local vision, using the existing host capture boundary and no OS actions.
use desktop_host::{
    DesktopBackgroundCaptureRegion, DesktopBackgroundFrame, DesktopSnapshot, RectI,
};
use glam::Vec2;
use lifecore::AppCategory;
use pet_vision::{SceneKind, ScenePrediction, VisionEngine};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use winit::window::Window;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    enabled: bool,
    interval_seconds: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_seconds: 12,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FamiliarScene {
    kind: SceneKind,
    signature: u64,
    observations: u32,
}
#[derive(Debug, Default, Serialize, Deserialize)]
struct SceneMemory {
    entries: Vec<FamiliarScene>,
}
impl SceneMemory {
    fn observe(&mut self, p: &ScenePrediction) -> f32 {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.kind == p.kind && (e.signature ^ p.signature).count_ones() <= 6)
        {
            let previous = entry.observations.min(1000);
            entry.observations = entry.observations.saturating_add(1);
            return previous as f32 / (previous as f32 + 3.0);
        }
        if self.entries.len() >= 64 {
            self.entries.remove(0);
        }
        self.entries.push(FamiliarScene {
            kind: p.kind,
            signature: p.signature,
            observations: 1,
        });
        0.0
    }
}

#[derive(Clone, Copy)]
pub struct SemanticAttention {
    pub position: Vec2,
    pub novelty: f32,
}
struct Job {
    id: u64,
    context: u64,
    region: DesktopBackgroundCaptureRegion,
    masks: [RectI; 2],
    position: Vec2,
    submitted: Instant,
    permission: u64,
}
struct Observation {
    id: u64,
    context: u64,
    prediction: ScenePrediction,
    position: Vec2,
    submitted: Instant,
    permission: u64,
}
enum Event {
    Ready,
    Observation(Observation),
    Skipped(u64),
    Failed(u64, String),
}

pub struct VisionBridge {
    root: PathBuf,
    window: Arc<Window>,
    settings: Settings,
    enabled: Arc<AtomicBool>,
    permission: Arc<AtomicU64>,
    tx: Option<SyncSender<Job>>,
    rx: Option<Receiver<Event>>,
    worker: Option<JoinHandle<()>>,
    settings_poll: Instant,
    last_submit: Instant,
    context_since: Instant,
    last_sampled_context: Option<u64>,
    budget_updated: Instant,
    query_tokens: f32,
    last_status: Instant,
    last_cue: Instant,
    next_id: u64,
    last_processed: u64,
    context: u64,
    busy: bool,
    state: String,
    last_error: Option<String>,
    proposed: Option<(u64, SceneKind)>,
    stable: u32,
    current: Option<(u64, ScenePrediction, Instant)>,
    attention: Option<(SemanticAttention, Instant)>,
    memory: SceneMemory,
    completed: u64,
    accepted: u64,
    ambiguous: u64,
    skipped: u64,
    last_latency_ms: f32,
}

fn atomic_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("No parent directory"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temp, value)?;
    temp.flush()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

impl VisionBridge {
    pub fn new(explicit_opt_in: bool, root: PathBuf, window: Arc<Window>) -> Self {
        let path = root.join("vision-settings.json");
        let mut settings = fs::read(&path)
            .ok()
            .filter(|b| b.len() <= 4096)
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default();
        if explicit_opt_in {
            settings.enabled = true;
            let _ = atomic_json(&path, &settings);
        }
        let memory = fs::read(root.join("vision-memory.json"))
            .ok()
            .filter(|b| b.len() < 32768)
            .and_then(|b| serde_json::from_slice::<SceneMemory>(&b).ok())
            .filter(|m| m.entries.len() <= 64)
            .unwrap_or_default();
        let old = Instant::now() - Duration::from_secs(120);
        Self {
            root,
            window,
            settings,
            enabled: Arc::new(AtomicBool::new(false)),
            permission: Arc::new(AtomicU64::new(0)),
            tx: None,
            rx: None,
            worker: None,
            settings_poll: old,
            last_submit: old,
            context_since: old,
            last_sampled_context: None,
            budget_updated: Instant::now(),
            query_tokens: 2.0,
            last_status: old,
            last_cue: old,
            next_id: 1,
            last_processed: 0,
            context: 0,
            busy: false,
            state: "disabled".into(),
            last_error: None,
            proposed: None,
            stable: 0,
            current: None,
            attention: None,
            memory,
            completed: 0,
            accepted: 0,
            ambiguous: 0,
            skipped: 0,
            last_latency_ms: 0.0,
        }
    }
    pub fn toggle(&mut self) {
        self.settings.enabled = !self.settings.enabled;
        if let Err(e) = atomic_json(&self.root.join("vision-settings.json"), &self.settings) {
            self.last_error = Some(e.to_string());
        }
        self.settings_poll = Instant::now() - Duration::from_secs(2);
    }
    fn poll_settings(&mut self) {
        if self.settings_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.settings_poll = Instant::now();
        let path = self.root.join("vision-settings.json");
        // Deletion, malformed input and unreadable settings all revoke capture.
        self.settings = fs::read(path)
            .ok()
            .filter(|b| b.len() <= 4096)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        self.settings.interval_seconds = self.settings.interval_seconds.clamp(8, 60);
        if !self.settings.enabled {
            update_permission(&self.permission, false, false);
            self.enabled.store(false, Ordering::Release);
            self.tx.take();
            self.rx.take();
            self.current = None;
            self.attention = None;
            self.proposed = None;
            self.stable = 0;
            self.busy = false;
            self.state = "disabled".into();
        }
    }
    fn ensure_worker(&mut self) {
        if !self.settings.enabled || self.tx.is_some() {
            return;
        }
        if self.worker.as_ref().is_some_and(|j| !j.is_finished()) {
            return;
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if self.last_error.is_some() && self.last_submit.elapsed() < Duration::from_secs(60) {
            return;
        }
        let package = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let model = std::env::var_os("PET2_VISION_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| package.join("models/lfm2.5-vl-450m"));
        let library = std::env::var_os("PET2_ONNX_RUNTIME_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                package.join(if cfg!(windows) {
                    "runtime/onnxruntime.dll"
                } else {
                    "runtime/libonnxruntime.dylib"
                })
            });
        let (tx, requests) = mpsc::sync_channel::<Job>(1);
        let (events, rx) = mpsc::sync_channel::<Event>(4);
        let window = Arc::clone(&self.window);
        let enabled = Arc::clone(&self.enabled);
        let permission = Arc::clone(&self.permission);
        enabled.store(true, Ordering::Release);
        self.state = "loading".into();
        self.last_error = None;
        self.worker = std::thread::Builder::new()
            .name("pet2-local-vision".into())
            .spawn(move || {
                let mut engine = match VisionEngine::load(&model, &library) {
                    Ok(engine) => engine,
                    Err(e) => {
                        let _ = events.send(Event::Failed(0, e.to_string()));
                        return;
                    }
                };
                let mut capture = desktop_host::create_platform_backend();
                let _ = events.send(Event::Ready);
                while let Ok(job) = requests.recv() {
                    if !enabled.load(Ordering::Acquire) {
                        break;
                    }
                    if !permission_valid(&permission, job.permission)
                        || job.submitted.elapsed() > Duration::from_secs(3)
                    {
                        let _ = events.send(Event::Skipped(job.id));
                        continue;
                    }
                    // Dedicated instance of the EXISTING capture adapter. It never
                    // changes overlay styles, capture affinity or user input state.
                    let old_sequence = capture
                        .capture_overlay_background(&window, job.region)
                        .map_or(0, |f| f.sequence);
                    let start = Instant::now();
                    let mut frame = None;
                    while start.elapsed() < Duration::from_millis(800)
                        && enabled.load(Ordering::Acquire)
                        && permission_valid(&permission, job.permission)
                    {
                        std::thread::sleep(Duration::from_millis(25));
                        if let Some(candidate) =
                            capture.capture_overlay_background(&window, job.region)
                            && candidate.sequence > old_sequence
                            && matches_region(&candidate, job.region)
                        {
                            frame = Some(candidate);
                            break;
                        }
                    }
                    let Some(frame) = frame else {
                        let _ = events.send(Event::Skipped(job.id));
                        continue;
                    };
                    if !enabled.load(Ordering::Acquire) {
                        break;
                    }
                    if !permission_valid(&permission, job.permission) {
                        let _ = events.send(Event::Skipped(job.id));
                        continue;
                    }
                    let Some(rgb) = masked_rgb(&frame, &job.masks) else {
                        let _ = events.send(Event::Skipped(job.id));
                        continue;
                    };
                    match engine.classify_with_cancellation(
                        frame.width,
                        frame.height,
                        rgb,
                        256,
                        Some((Arc::clone(&permission), job.permission)),
                    ) {
                        Ok(prediction) => {
                            let _ = events.send(Event::Observation(Observation {
                                id: job.id,
                                context: job.context,
                                prediction,
                                position: job.position,
                                submitted: job.submitted,
                                permission: job.permission,
                            }));
                        }
                        Err(e) => {
                            if permission_valid(&permission, job.permission) {
                                let _ = events.send(Event::Failed(job.id, e.to_string()));
                            } else {
                                let _ = events.send(Event::Skipped(job.id));
                            }
                        }
                    }
                }
                capture.shutdown();
            })
            .ok();
        if self.worker.is_some() {
            self.tx = Some(tx);
            self.rx = Some(rx);
        } else {
            self.state = "failed".into();
            self.last_error = Some("Cannot start vision worker".into());
        }
    }
    pub fn tick(&mut self, snapshot: &DesktopSnapshot, paused: bool, masks: [RectI; 2]) {
        self.poll_settings();
        self.ensure_worker();
        let now = Instant::now();
        // A short confirmation burst, then at most one budget token per 8s.
        // Rapid task switching cannot turn semantic sensing into a busy loop.
        self.query_tokens = (self.query_tokens
            + now.duration_since(self.budget_updated).as_secs_f32() / 8.0)
            .min(2.0);
        self.budget_updated = now;
        let category = snapshot
            .active_application
            .as_ref()
            .map_or(AppCategory::Unknown, |a| a.category);
        let context_bytes = format!(
            "{:?}:{:?}",
            snapshot.active_application, snapshot.active_window
        );
        let context = lifecore::stable_hash_bytes(context_bytes.as_bytes());
        let context_changed = context != self.context;
        if context_changed {
            self.context_since = now;
            self.current = None;
            self.attention = None;
            self.stable = 0;
            self.proposed = None;
            self.context = context;
        }
        let paused = paused
            || category == AppCategory::Communication
            || snapshot.user_idle_seconds_or_zero() > 180.0;
        update_permission(
            &self.permission,
            self.settings.enabled && !paused,
            context_changed,
        );
        if paused {
            self.current = None;
            self.attention = None;
            self.proposed = None;
            self.stable = 0;
        }
        let events: Vec<_> = self
            .rx
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Ready => {
                    self.state = "ready".into();
                    self.last_submit = now - Duration::from_secs(120);
                }
                Event::Skipped(id) => {
                    if id > self.last_processed {
                        self.last_processed = id;
                        self.skipped += 1;
                    }
                    self.busy = false;
                }
                Event::Failed(id, error) => {
                    self.last_processed = self.last_processed.max(id);
                    self.busy = false;
                    self.state = "failed".into();
                    self.last_error = Some(error);
                    self.last_submit = now;
                    if id == 0 {
                        self.tx.take();
                    }
                }
                Event::Observation(obs) => {
                    self.busy = false;
                    if obs.id <= self.last_processed {
                        continue;
                    }
                    self.last_processed = obs.id;
                    self.completed += 1;
                    self.last_latency_ms = obs.prediction.elapsed_ms;
                    if !self.settings.enabled
                        || paused
                        || obs.context != context
                        || !permission_valid(&self.permission, obs.permission)
                        || obs.submitted.elapsed() > Duration::from_secs(9)
                    {
                        self.skipped += 1;
                        continue;
                    }
                    if !obs.prediction.trustworthy() {
                        self.ambiguous += 1;
                        self.state = "uncertain".into();
                        self.proposed = None;
                        self.stable = 0;
                        self.current = None;
                        continue;
                    }
                    let proposal = (context, obs.prediction.kind);
                    self.stable = if self.proposed == Some(proposal) {
                        self.stable.saturating_add(1)
                    } else {
                        1
                    };
                    self.proposed = Some(proposal);
                    if self.stable < 2 {
                        self.state = "confirming".into();
                        continue;
                    }
                    self.accepted += 1;
                    self.state = "observing".into();
                    self.last_error = None;
                    let familiar = self.memory.observe(&obs.prediction);
                    if let Err(e) = atomic_json(&self.root.join("vision-memory.json"), &self.memory)
                    {
                        self.last_error = Some(e.to_string());
                    }
                    if matches!(
                        obs.prediction.kind,
                        SceneKind::Artwork | SceneKind::Media | SceneKind::Chart
                    ) && familiar < 0.70
                        && self.last_cue.elapsed() > Duration::from_secs(30)
                    {
                        self.last_cue = now;
                        self.attention = Some((
                            SemanticAttention {
                                position: obs.position,
                                novelty: 0.48 * (1.0 - familiar),
                            },
                            now,
                        ));
                    }
                    self.current = Some((context, obs.prediction, now));
                }
            }
        }
        let fast_confirmation = self.last_sampled_context != Some(context)
            || (self.stable == 1 && self.proposed.is_some_and(|(c, _)| c == context));
        let interval = if fast_confirmation {
            2
        } else {
            self.settings.interval_seconds
        };
        if self.settings.enabled
            && !paused
            && !self.busy
            && self.state != "loading"
            && self.query_tokens >= 1.0
            && self.context_since.elapsed() >= Duration::from_millis(650)
            && self.last_submit.elapsed() >= Duration::from_secs(u64::from(interval))
            && let Some(rect) = snapshot.active_window
            && rect.is_valid()
            && let Ok(origin) = self.window.outer_position()
        {
            let size = self.window.inner_size();
            let scale = Vec2::new(size.width.max(1) as f32, size.height.max(1) as f32);
            let offset = Vec2::new(origin.x as f32, origin.y as f32);
            let min = ((Vec2::new(rect.minimum.x as f32, rect.minimum.y as f32) - offset) / scale)
                .clamp(Vec2::ZERO, Vec2::ONE);
            let max = ((Vec2::new(rect.maximum.x as f32, rect.maximum.y as f32) - offset) / scale)
                .clamp(Vec2::ZERO, Vec2::ONE);
            if let Some(region) = DesktopBackgroundCaptureRegion::new(min, max, 256) {
                let job = Job {
                    id: self.next_id,
                    context,
                    region,
                    masks,
                    position: (min + max) * 0.5,
                    submitted: now,
                    permission: self.permission.load(Ordering::Acquire),
                };
                if self.tx.as_ref().is_some_and(|tx| tx.try_send(job).is_ok()) {
                    self.next_id = self.next_id.saturating_add(1);
                    self.busy = true;
                    self.last_sampled_context = Some(context);
                    self.query_tokens = (self.query_tokens - 1.0).max(0.0);
                    self.last_submit = now;
                }
            }
        }
        if self.last_status.elapsed() >= Duration::from_secs(1) {
            self.last_status = now;
            let _ = atomic_json(
                &self.root.join("vision-status.json"),
                &serde_json::json!({
                "schema_version":1,"enabled":self.settings.enabled,"state":self.state,"paused":paused,"inference_busy":self.busy,
                "model":"LFM2.5-VL-450M-ONNX","provider":"CPU, 2 threads","scope":"active window, local only",
                "completed":self.completed,"accepted":self.accepted,"ambiguous":self.ambiguous,"skipped":self.skipped,
                "query_budget_tokens":self.query_tokens,"confirmation_interval_seconds":2,"context_debounce_ms":650,
                "last_latency_ms":self.last_latency_ms,"scene":self.current.as_ref().map(|(_,p,_)|p.kind),"error":self.last_error,
                "memory_entries":self.memory.entries.len(),"screenshots_saved":false,"model_revision":pet_vision::MODEL_REVISION}),
            );
        }
    }
    pub fn category(&self) -> Option<AppCategory> {
        let (context, p, at) = self.current.as_ref()?;
        if !self.settings.enabled
            || *context != self.context
            || at.elapsed() > Duration::from_secs(20)
        {
            return None;
        }
        match p.kind {
            SceneKind::Code | SceneKind::Document | SceneKind::Chart => {
                Some(AppCategory::FocusedWork)
            }
            SceneKind::Artwork => Some(AppCategory::Creative),
            SceneKind::Media | SceneKind::Game => Some(AppCategory::Entertainment),
            SceneKind::Desktop => Some(AppCategory::System),
            _ => None,
        }
    }
    pub fn attention(&self) -> Option<SemanticAttention> {
        self.attention
            .filter(|(_, at)| self.settings.enabled && at.elapsed() < Duration::from_secs(4))
            .map(|(a, _)| a)
    }
}
impl Drop for VisionBridge {
    fn drop(&mut self) {
        update_permission(&self.permission, false, true);
        self.enabled.store(false, Ordering::Release);
        self.tx.take();
    }
}

// One atomic word carries both permission (odd) and context generation. A
// pause, revocation or context switch invalidates queued AND running work.
fn update_permission(epoch: &AtomicU64, allowed: bool, context_changed: bool) {
    let current = epoch.load(Ordering::Acquire);
    if context_changed || ((current & 1 != 0) != allowed) {
        let next = (current & !1).wrapping_add(2) | u64::from(allowed);
        epoch.store(next, Ordering::Release);
    }
}
fn permission_valid(epoch: &AtomicU64, token: u64) -> bool {
    token & 1 != 0 && epoch.load(Ordering::Acquire) == token
}

fn matches_region(frame: &DesktopBackgroundFrame, region: DesktopBackgroundCaptureRegion) -> bool {
    let expected = [
        region.minimum_normalized.x,
        region.minimum_normalized.y,
        region.maximum_normalized.x - region.minimum_normalized.x,
        region.maximum_normalized.y - region.minimum_normalized.y,
    ];
    frame
        .normalized_region
        .iter()
        .zip(expected)
        .all(|(a, b)| (*a - b).abs() < 0.003)
}
fn masked_rgb(frame: &DesktopBackgroundFrame, masks: &[RectI]) -> Option<Vec<u8>> {
    if frame.width == 0
        || frame.height == 0
        || frame.width > 512
        || frame.height > 512
        || frame.bytes_per_row < frame.width * 4
        || frame.bgra8.len() < frame.bytes_per_row as usize * frame.height as usize
    {
        return None;
    }
    let mut rgb = Vec::with_capacity(frame.width as usize * frame.height as usize * 3);
    for y in 0..frame.height {
        for x in 0..frame.width {
            let px = frame.physical_rect.minimum.x as f32
                + (x as f32 + 0.5) * frame.physical_rect.width() as f32 / frame.width as f32;
            let py = frame.physical_rect.minimum.y as f32
                + (y as f32 + 0.5) * frame.physical_rect.height() as f32 / frame.height as f32;
            let masked = masks.iter().any(|m| {
                px >= m.minimum.x as f32
                    && px <= m.maximum.x as f32
                    && py >= m.minimum.y as f32
                    && py <= m.maximum.y as f32
            });
            if masked {
                rgb.extend_from_slice(&[128, 128, 128]);
            } else {
                let i = y as usize * frame.bytes_per_row as usize + x as usize * 4;
                rgb.extend_from_slice(&[frame.bgra8[i + 2], frame.bgra8[i + 1], frame.bgra8[i]]);
            }
        }
    }
    Some(rgb)
}

trait IdleSeconds {
    fn user_idle_seconds_or_zero(&self) -> f32;
}
impl IdleSeconds for DesktopSnapshot {
    fn user_idle_seconds_or_zero(&self) -> f32 {
        self.idle_seconds.unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_revocation_and_context_switch_invalidate_old_capture_leases() {
        let gate = AtomicU64::new(0);
        assert!(!permission_valid(&gate, 0));
        update_permission(&gate, true, false);
        let first = gate.load(Ordering::Acquire);
        assert!(permission_valid(&gate, first));
        update_permission(&gate, true, false);
        assert!(permission_valid(&gate, first));
        update_permission(&gate, true, true);
        assert!(!permission_valid(&gate, first));
        let second = gate.load(Ordering::Acquire);
        assert!(permission_valid(&gate, second));
        update_permission(&gate, false, false);
        assert!(!permission_valid(&gate, second));
        update_permission(&gate, true, false);
        assert!(!permission_valid(&gate, second));
    }
    #[test]
    fn scene_memory_is_bounded_and_recognition_does_not_fabricate_rewards() {
        let mut memory = SceneMemory::default();
        let p = ScenePrediction {
            kind: SceneKind::Chart,
            support: 0.98,
            margin: 0.96,
            label_mass: 0.99,
            elapsed_ms: 10.0,
            image_tokens: 64,
            signature: 123,
        };
        assert_eq!(memory.observe(&p), 0.0);
        assert!(memory.observe(&p) > 0.0);
        let restored: SceneMemory =
            serde_json::from_str(&serde_json::to_string(&memory).unwrap()).unwrap();
        assert_eq!(restored.entries[0].observations, 2);
    }
    #[test]
    fn settings_default_to_no_capture_and_unknown_settings_fail_closed() {
        assert!(!Settings::default().enabled);
        assert!(serde_json::from_str::<Settings>(r#"{"enabled":true,"unbounded":true}"#).is_err());
    }
}

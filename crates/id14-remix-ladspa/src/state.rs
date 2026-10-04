//! remix-state-v1 publication. Only the publication worker touches the filesystem.
use crate::{dsp::LATENCY, engine::Status};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Telemetry {
    // Single audio writer: low four bits = status, upper bits = overload entries.
    output: AtomicU64,
    pub model: AtomicU32,
}
impl Telemetry {
    pub fn new() -> Self {
        Self {
            output: AtomicU64::new(0),
            model: AtomicU32::new(0),
        }
    }
    pub fn update(&self, status: Status) {
        let previous = self.output.load(Ordering::Relaxed);
        if previous & 15 != status as u64 {
            let count = (previous >> 4).saturating_add(u64::from(status == Status::Overloaded));
            self.output.store(
                (count.min(u64::MAX >> 4) << 4) | status as u64,
                Ordering::Release,
            );
        }
    }
    fn snapshot(&self) -> (Status, u64) {
        let value = self.output.load(Ordering::Acquire);
        let mut status = Status::from_u32((value & 15) as u32);
        if status == Status::Loading {
            let model = Status::from_u32(self.model.load(Ordering::Acquire));
            if model != Status::Loading && model != Status::Active {
                status = model;
            }
        }
        (status, value >> 4)
    }
}

pub(crate) struct Publisher {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Publisher {
    pub fn start(telemetry: Arc<Telemetry>, model: Option<PathBuf>) -> Self {
        Self::start_with_runtime(telemetry, model, || {
            std::env::var_os("XDG_RUNTIME_DIR")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
        })
    }
    fn start_with_runtime<F>(telemetry: Arc<Telemetry>, model: Option<PathBuf>, runtime: F) -> Self
    where
        F: FnOnce() -> Option<PathBuf> + Send + 'static,
    {
        let id = NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::Builder::new().name("id14-remix-state".into()).spawn(move || {
            let Some(runtime) = runtime() else {
                return;
            };
            let directory = PathBuf::from(runtime).join("id14-sr/remix-state");
            let pid = std::process::id();
            let path = directory.join(format!("{pid}-{id}.json"));
            let temporary = directory.join(format!(".{pid}-{id}.json.tmp"));
            let mut last = None;
            let mut written_at = Instant::now();
            let mut owns_file = false;
            while !stopping.load(Ordering::Acquire) {
                let current = telemetry.snapshot();
                if last != Some(current) || written_at.elapsed() >= Duration::from_secs(1) {
                    let updated = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
                    let value = serde_json::json!({
                        "contract": "remix-state-v1", "pid": pid, "instance": id,
                        "state": current.0 as u32, "state_name": current.0.name(),
                        "overloads": current.1, "model": model.as_ref().map(|p| p.to_string_lossy()),
                        "latency_frames": LATENCY, "updated_unix_ms": updated as u64,
                    });
                    // Best effort, deliberately silent. Failure never reaches audio.
                    if std::fs::create_dir_all(&directory).is_ok() {
                        if let Ok(mut file) = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary) {
                            let result = file.write_all(value.to_string().as_bytes());
                            drop(file);
                            if result.is_ok() && std::fs::rename(&temporary, &path).is_ok() {
                                owns_file = true;
                            } else {
                                let _ = std::fs::remove_file(&temporary);
                            }
                        }
                    }
                    last = Some(current);
                    written_at = Instant::now();
                }
                thread::sleep(Duration::from_millis(10));
            }
            if owns_file {
                let _ = std::fs::remove_file(path);
            }
        }).ok();
        Self { stop, worker }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Controls, Engine};

    fn directory() -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".build/state-tests")
            .join(uuid::Uuid::now_v7().to_string());
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn wait_for(root: &std::path::Path, state: Status) -> (PathBuf, serde_json::Value) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(files) = std::fs::read_dir(root.join("id14-sr/remix-state")) {
                for file in files.flatten() {
                    let path = file.path();
                    if path.extension().is_some_and(|s| s == "json") {
                        // Any partial JSON at a final filename is a failure.
                        let value: serde_json::Value =
                            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                        if value["state"] == state as u32 {
                            return (path, value);
                        }
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "state file not published: {state:?}"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn publication_updates_heartbeat_and_owned_cleanup() {
        let root = directory();
        let telemetry = Arc::new(Telemetry::new());
        let runtime = root.clone();
        let publisher =
            Publisher::start_with_runtime(telemetry.clone(), None, move || Some(runtime));
        let (path, first) = wait_for(&root, Status::Loading);
        assert_eq!(first["contract"], "remix-state-v1");
        assert_eq!(first["pid"], std::process::id());
        assert_eq!(first["latency_frames"], LATENCY);
        assert!(first["model"].is_null());
        let sibling = path.parent().unwrap().join("another-instance.json");
        std::fs::write(&sibling, b"{}").unwrap();
        thread::sleep(Duration::from_millis(100));
        let unchanged: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(first["updated_unix_ms"], unchanged["updated_unix_ms"]);
        telemetry.update(Status::Overloaded);
        telemetry.update(Status::Overloaded);
        let (_, overloaded) = wait_for(&root, Status::Overloaded);
        assert_eq!(overloaded["overloads"], 1);
        assert_eq!(overloaded["state_name"], "Overloaded");
        telemetry.update(Status::Off);
        wait_for(&root, Status::Off);
        telemetry.update(Status::Overloaded);
        let (_, second) = wait_for(&root, Status::Overloaded);
        assert_eq!(second["overloads"], 2);
        let timestamp = second["updated_unix_ms"].as_u64().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let heartbeat: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let elapsed = heartbeat["updated_unix_ms"].as_u64().unwrap() - timestamp;
            if elapsed > 0 {
                assert!(elapsed >= 1000);
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        drop(publisher);
        assert!(!path.exists());
        assert!(sibling.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unavailable_runtime_preserves_audio_bits() {
        let root = directory();
        let blocked = root.join("not-a-directory");
        std::fs::write(&blocked, b"blocked").unwrap();
        for runtime in [None, Some(blocked)] {
            let telemetry = Arc::new(Telemetry::new());
            let mut engine =
                Engine::with_telemetry(|| Err(Status::ModelMissing), telemetry.clone());
            let publisher = Publisher::start_with_runtime(telemetry, None, move || runtime);
            engine.set_controls(Controls {
                enabled: false,
                ..Controls::default()
            });
            let input: Vec<_> = (0..(LATENCY + 1024))
                .map(|n| [n as f32 / 10000.0, -0.0])
                .collect();
            let mut output = vec![[0.0; 2]; input.len()];
            engine.process(&input, &mut output);
            for (actual, expected) in output[LATENCY..].iter().zip(&input) {
                assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
            }
            drop(publisher);
        }
        assert!(!root.join("id14-sr").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn model_failure_visible_without_audio_callback() {
        let telemetry = Telemetry::new();
        telemetry
            .model
            .store(Status::ModelMissing as u32, Ordering::Release);
        assert_eq!(telemetry.snapshot(), (Status::ModelMissing, 0));
        for n in 0..=8 {
            let state = Status::from_u32(n);
            assert_eq!(state as u32, n);
            assert!(!state.name().is_empty());
        }
    }
}
impl Drop for Publisher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

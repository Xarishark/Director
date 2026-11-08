//! Bounded, asynchronous JSON-lines diagnostics. Never pass secrets or raw protocol payloads.
use serde_json::json;
use std::{
    cell::Cell,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const LIMIT: u64 = 1024 * 1024;
const FILES: usize = 5;
static SENDER: OnceLock<Option<SyncSender<Item>>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
static LOST: AtomicU64 = AtomicU64::new(0);
static WRITTEN: AtomicU64 = AtomicU64::new(0);
static ERROR: Mutex<Option<String>> = Mutex::new(None);
thread_local! { static OPERATION: Cell<u64> = const { Cell::new(0) }; }
enum Item {
    Line(Vec<u8>),
    Flush(mpsc::Sender<()>),
}
pub fn folder() -> Result<PathBuf, String> {
    crate::storage::settings_path("Logs")
}
pub fn written() -> u64 {
    WRITTEN.load(Ordering::Relaxed)
}
pub fn initialized() -> bool {
    SENDER.get().is_some_and(Option::is_some)
}
pub fn failure() -> Option<String> {
    ERROR.lock().ok().and_then(|e| e.clone())
}
pub fn init() {
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel(512);
        match std::thread::Builder::new()
            .name("director-log".into())
            .spawn(move || {
                let directory = match folder() {
                    Ok(path) => path,
                    Err(e) => {
                        if let Ok(mut error) = ERROR.lock() {
                            *error = Some(e);
                        }
                        return;
                    }
                };
                for item in rx {
                    match item {
                        Item::Line(bytes) => {
                            let result = append(&directory, &bytes, LIMIT, FILES);
                            if result.is_ok() {
                                WRITTEN.fetch_add(1, Ordering::Relaxed);
                            }
                            if let Ok(mut error) = ERROR.lock() {
                                *error = result.err().map(|e| e.to_string());
                            }
                        }
                        Item::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            }) {
            Ok(_) => Some(tx),
            Err(e) => {
                if let Ok(mut error) = ERROR.lock() {
                    *error = Some(e.to_string());
                }
                None
            }
        }
    });
}
fn bounded(text: &str) -> String {
    text.chars().take(2048).collect()
}
fn timestamp_utc() -> String {
    #[cfg(windows)]
    {
        let t = unsafe { windows::Win32::System::SystemInformation::GetSystemTime() };
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
        )
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

fn line(operation: u64, step: &str, outcome: &str, detail: &str) -> Vec<u8> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut data = serde_json::to_vec(&json!({
        "timestamp": timestamp_utc(), "timestamp_unix_ms": timestamp, "pid": std::process::id(),
        "operation": operation, "step": step, "outcome": outcome, "detail": bounded(detail)
    }))
    .unwrap_or_default();
    data.push(b'\n');
    data
}
pub fn event(step: &str, outcome: &str, detail: &str) {
    let Some(Some(sender)) = SENDER.get() else {
        return;
    };
    let operation = OPERATION.with(Cell::get);
    let lost = LOST.swap(0, Ordering::Relaxed);
    if lost > 0
        && sender
            .try_send(Item::Line(line(
                0,
                "logging.queue",
                "dropped",
                &lost.to_string(),
            )))
            .is_err()
    {
        LOST.fetch_add(lost, Ordering::Relaxed);
    }
    if sender
        .try_send(Item::Line(line(operation, step, outcome, detail)))
        .is_err()
    {
        LOST.fetch_add(1, Ordering::Relaxed);
    }
}
pub fn result<T>(step: &str, result: &Result<T, String>) {
    match result {
        Ok(_) => event(step, "ok", ""),
        Err(error) => event(step, "error", error),
    }
}
pub fn measure<T>(
    step: &str,
    detail: &str,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let start = Instant::now();
    event(step, "started", detail);
    let result = action();
    let outcome = match &result {
        Ok(_) => "ok",
        Err(_) => "error",
    };
    let error = result.as_ref().err().map(String::as_str).unwrap_or("");
    event(
        step,
        outcome,
        &format!("elapsed_ms={} {error}", start.elapsed().as_millis()),
    );
    result
}
pub struct Operation {
    id: u64,
    previous: u64,
    start: Instant,
    name: &'static str,
}
impl Operation {
    pub fn start(name: &'static str, device: Option<&str>) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let previous = OPERATION.with(|current| current.replace(id));
        event(name, "started", device.unwrap_or(""));
        Self {
            id,
            previous,
            start: Instant::now(),
            name,
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        OPERATION.with(|current| current.set(self.id));
        event(
            self.name,
            if std::thread::panicking() {
                "panicked"
            } else {
                "finished"
            },
            &format!("elapsed_ms={}", self.start.elapsed().as_millis()),
        );
        OPERATION.with(|current| current.set(self.previous));
    }
}
pub fn flush() {
    if let Some(Some(sender)) = SENDER.get() {
        let (tx, rx) = mpsc::channel();
        // Exit path only: ordinary UI logging never blocks.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut item = Item::Flush(tx);
        loop {
            match sender.try_send(item) {
                Ok(()) => {
                    let _ = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
                    break;
                }
                Err(mpsc::TrySendError::Full(pending)) if Instant::now() < deadline => {
                    item = pending;
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    }
}
fn append(directory: &Path, bytes: &[u8], limit: u64, files: usize) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("director.lock"))?;
    lock.lock()?;
    let active = directory.join("director.log");
    if fs::metadata(&active).is_ok_and(|m| m.len() + bytes.len() as u64 > limit) {
        let oldest = directory.join(format!("director.{}.log", files - 1));
        match fs::remove_file(oldest) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        for index in (1..files - 1).rev() {
            let from = directory.join(format!("director.{index}.log"));
            if from.exists() {
                fs::rename(from, directory.join(format!("director.{}.log", index + 1)))?;
            }
        }
        fs::rename(&active, directory.join("director.1.log"))?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(active)?;
    file.write_all(bytes)?;
    file.flush()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_is_bounded_and_parallel_writers_keep_complete_records() {
        let directory = std::env::temp_dir().join(format!(
            "director-logs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let workers: Vec<_> = (0..4)
            .map(|n| {
                let directory = directory.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        append(&directory, &line(n, "test", "ok", "line\nbreak"), 1400, 3).unwrap();
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let logs: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "log"))
            .collect();
        assert_eq!(logs.len(), 3);
        for path in logs {
            assert!(fs::metadata(&path).unwrap().len() <= 1400);
            for row in fs::read_to_string(path).unwrap().lines() {
                serde_json::from_str::<serde_json::Value>(row).unwrap();
            }
        }
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn diagnostic_details_are_bounded_and_control_characters_are_escaped() {
        let value: serde_json::Value =
            serde_json::from_slice(&line(7, "test", "error", &"\n".repeat(3000))).unwrap();
        assert_eq!(value["detail"].as_str().unwrap().chars().count(), 2048);
        assert_eq!(value["operation"], 7);
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct LastRun {
    pub name: String,
    pub completed_at: String,
    pub failed_stage: Option<String>,
    pub message: String,
    pub details: Vec<String>,
}

impl LastRun {
    pub fn new(name: &str, stage: &str, report: &crate::scene::scenes::Report) -> Self {
        Self {
            name: bounded(name),
            completed_at: timestamp_utc(),
            failed_stage: (!report.success).then(|| bounded(stage)),
            message: bounded(&report.message),
            details: report
                .details
                .iter()
                .take(64)
                .map(|detail| bounded(detail))
                .collect(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        crate::storage::write_document(&folder()?.join("last-run.json"), self)
    }

    pub fn read() -> Result<Option<Self>, String> {
        Self::read_file(&folder()?.join("last-run.json"))
    }

    pub(crate) fn read_file(path: &Path) -> Result<Option<Self>, String> {
        use std::io::Read;
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let mut bytes = Vec::new();
        file.take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > LIMIT {
            return Err("Last run record is too large.".into());
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| e.to_string())
    }
}

pub fn recent_text() -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL: u64 = 64 * 1024;
    let dir = folder()?;
    let mut logs = Vec::new();
    for name in ["director.1.log", "director.log"] {
        let mut file = match fs::File::open(dir.join(name)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let start = size.saturating_sub(TAIL);
        file.seek(SeekFrom::Start(start))
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(TAIL)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines().skip(usize::from(start != 0)) {
            if let Ok(record) = serde_json::from_str::<serde_json::Value>(line) {
                let field = |key: &str| record[key].as_str().unwrap_or_default();
                logs.push(format!(
                    "{}  {} · {}\n{}",
                    field("timestamp"),
                    field("step"),
                    field("outcome"),
                    field("detail")
                ));
            }
        }
    }
    let first = logs.len().saturating_sub(100);
    Ok(logs[first..]
        .iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n"))
}

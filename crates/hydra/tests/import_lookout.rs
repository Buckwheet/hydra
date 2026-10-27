//! `hydra import` and `hydra lookout seed` end to end, with synthetic data only.
//!
//! Every run uses a temp `HYDRA_DATA_DIR` and (for the lookout) a temp
//! `HYDRA_LOG_DIR`; nothing here touches a real game, a real account or the
//! user's own logs.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_hydra")
}

const WAIT: Duration = Duration::from_secs(30);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hydra-p7-{tag}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Finished {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str], data: &TempDir, log_dir: Option<&TempDir>) -> Finished {
    let mut child: Child = Command::new(binary())
        .args(args)
        .env("HYDRA_DATA_DIR", &data.0)
        .env("HYDRA_LICH_SETTINGS", data.0.join("no-lich.json"))
        .env(
            "HYDRA_LOG_DIR",
            log_dir.map(|dir| dir.0.clone()).unwrap_or_else(|| data.0.join("logs")),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the hydra binary starts");

    let deadline = Instant::now() + WAIT;
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Finished {
                    code: status.code(),
                    stdout,
                    stderr,
                };
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("hydra {args:?} did not finish within {WAIT:?}");
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

/// A synthetic recording in the recovered rotation-name shape:
/// `<prefix>_<name>-<YYYY-MM-DD_HH-MM-SS>-000.bytes`.
fn recording(dir: &TempDir, text: &str) -> PathBuf {
    let path = dir.0.join("GS3_Aelith-2026-10-27_10-00-00-000.bytes");
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn import_records_a_synthetic_recording_and_repeats_are_skipped() {
    let data = TempDir::new("import");
    let path = recording(&data, "<app game=\"GS3\"/>\n<settingsInfo instance=\"GS3\"/>\n");
    let path = path.to_string_lossy().into_owned();

    let first = run(&["import", &path], &data, None);
    assert_eq!(first.code, Some(0), "stderr: {}", first.stderr);
    assert_eq!(
        first.stdout.trim(),
        "1 sessions read, 0 refused: 0 hunts and 0 pulses added; 0 login rewards found."
    );
    assert!(data.0.join("hydra.db").is_file(), "the ledger was created");

    let second = run(&["import", &path], &data, None);
    assert_eq!(second.code, Some(0), "stderr: {}", second.stderr);
    assert!(
        second.stdout.contains("imported before; skipped."),
        "stdout: {}",
        second.stdout
    );
    assert!(
        second.stdout.contains("1 refused"),
        "stdout: {}",
        second.stdout
    );
}

#[test]
fn import_reports_a_recording_it_does_not_know() {
    let data = TempDir::new("import-unknown");
    let path = recording(&data, "no instance in this recording");
    let path = path.to_string_lossy().into_owned();
    let finished = run(&["import", &path], &data, None);
    assert_eq!(finished.code, Some(0), "stderr: {}", finished.stderr);
    assert!(
        finished
            .stdout
            .contains("Not characters Hydra knows, so not imported: Aelith"),
        "stdout: {}",
        finished.stdout
    );
}

#[test]
fn lookout_seed_reads_the_player_logs_and_refuses_a_running_hydra() {
    let data = TempDir::new("lookout-data");
    let logs = TempDir::new("lookout-logs");
    let character = logs.0.join("Aelith");
    fs::create_dir_all(&character).unwrap();
    fs::write(
        character.join("2026-10-27_Aelith.log"),
        "[10:00][speech] a synthetic line\n[10:01][speech] another synthetic line\n",
    )
    .unwrap();

    let finished = run(&["lookout", "seed"], &data, Some(&logs));
    assert_eq!(finished.code, Some(0), "stderr: {}", finished.stderr);
    assert!(
        finished.stdout.contains("Aelith 2026-10-27: 2 lines"),
        "stdout: {}",
        finished.stdout
    );

    // Another Hydra owns the data folder: the recovered sentence and status 1.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.0.join("hydra.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let finished = run(&["lookout", "seed"], &data, Some(&logs));
    assert_eq!(finished.code, Some(1), "stdout: {}", finished.stdout);
    assert!(
        finished.stderr.contains(
            "Hydra is running on this data folder: shut it down, then seed the lookout."
        ),
        "stderr: {}",
        finished.stderr
    );
}

#[test]
fn lookout_seed_with_no_logs_says_so() {
    let data = TempDir::new("lookout-empty-data");
    let logs = TempDir::new("lookout-empty-logs");
    let finished = run(&["lookout", "seed"], &data, Some(&logs));
    assert_eq!(finished.code, Some(0), "stderr: {}", finished.stderr);
    assert_eq!(
        finished.stdout.trim(),
        "No player logs to teach the lookout from."
    );
}

//! Download runner: spawns yt-dlp, streams its output on a background thread,
//! and forwards structured events over a channel to the UI.

use crate::engine::options::{DownloadOptions, PROGRESS_PREFIX};
use crossbeam_channel::Sender;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A single parsed progress sample.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub percent: f32,
    pub percent_str: String,
    pub speed: String,
    pub eta: String,
    pub downloaded: String,
    pub total: String,
    pub item_index: Option<usize>,
    pub item_count: Option<usize>,
    pub title: String,
}

impl Progress {
    /// Overall playlist/album fraction, including the current item's progress.
    pub fn overall_fraction(&self) -> Option<f32> {
        let index = self.item_index?;
        let count = self.item_count?;
        if index == 0 || count <= 1 || index > count {
            return None;
        }
        let current = (self.percent / 100.0).clamp(0.0, 1.0);
        Some((((index - 1) as f32 + current) / count as f32).clamp(0.0, 1.0))
    }
}

/// Events emitted during a job.
#[derive(Debug, Clone)]
pub enum Event {
    /// Parsed download progress for the current item.
    Progress(Progress),
    /// A raw log line from yt-dlp (postprocessing, warnings, etc.).
    Log(String),
    /// A yt-dlp line explicitly classified as an error.
    Error(String),
    /// The job finished. `success` reflects the process exit status.
    Finished { success: bool, code: Option<i32> },
    /// The process could not be started at all.
    SpawnError(String),
}

/// Handle to a running job. Dropping it does not kill the child; call
/// [`JobHandle::cancel`] for that.
pub struct JobHandle {
    child_id: u32,
    thread: Option<JoinHandle<()>>,
    kill: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl JobHandle {
    pub fn child_id(&self) -> u32 {
        self.child_id
    }

    /// Request cancellation: kill the yt-dlp process tree immediately.
    ///
    /// yt-dlp runs ffmpeg/deno as separate child processes; on Windows,
    /// killing only the parent would leave an orphaned ffmpeg writing into
    /// the output directory. Killing the whole tree also works while yt-dlp
    /// is silent (e.g. during postprocessing), when the reader thread's
    /// kill-flag check — kept as a backstop — would otherwise wait for the
    /// next output line.
    pub fn cancel(&self) {
        self.kill.store(true, std::sync::atomic::Ordering::SeqCst);
        if self.child_id != 0 {
            kill_process_tree(self.child_id);
        }
    }

    /// Block until the job's reader thread completes.
    pub fn join(&mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Parse a `CATCHYT_PROG|...` line into a [`Progress`]. Returns `None` for
/// non-progress lines.
pub fn parse_progress_line(line: &str) -> Option<Progress> {
    let rest = line.strip_prefix(PROGRESS_PREFIX)?.strip_prefix('|')?;
    let parts: Vec<&str> = rest.splitn(8, '|').collect();
    if parts.len() < 8 {
        return None;
    }
    let percent_str = parts[0].trim().to_string();
    let percent = percent_str
        .trim_end_matches('%')
        .trim()
        .parse::<f32>()
        .unwrap_or(0.0);
    Some(Progress {
        percent,
        percent_str,
        speed: parts[1].trim().to_string(),
        eta: parts[2].trim().to_string(),
        downloaded: parts[3].trim().to_string(),
        total: parts[4].trim().to_string(),
        item_index: parse_optional_usize(parts[5]),
        item_count: parse_optional_usize(parts[6]),
        title: parts[7].trim().to_string(),
    })
}

fn parse_optional_usize(value: &str) -> Option<usize> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("NA") || value.eq_ignore_ascii_case("unknown")
    {
        return None;
    }
    value.parse::<usize>().ok().filter(|number| *number > 0)
}

/// Spawn a download job. Returns a [`JobHandle`] immediately; all output is
/// delivered asynchronously through `tx`.
pub fn spawn(
    ytdlp: &Path,
    ffmpeg_dir: &Path,
    opts: &DownloadOptions,
    url: &str,
    tx: Sender<Event>,
) -> JobHandle {
    let mut args = opts.build_args(ffmpeg_dir);
    args.push(url.to_string());

    let mut cmd = Command::new(ytdlp);
    cmd.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let kill = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(Event::SpawnError(format!(
                "Failed to start yt-dlp ({}): {e}",
                ytdlp.display()
            )));
            let _ = tx.send(Event::Finished {
                success: false,
                code: None,
            });
            return JobHandle {
                child_id: 0,
                thread: None,
                kill,
            };
        }
    };

    let child_id = child.id();
    let kill_thread = kill.clone();

    let thread = std::thread::spawn(move || {
        read_child(child, tx, kill_thread);
    });

    JobHandle {
        child_id,
        thread: Some(thread),
        kill,
    }
}

fn read_child(
    mut child: std::process::Child,
    tx: Sender<Event>,
    kill: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    // Drain stderr on its own thread so it can't deadlock the pipe.
    let stderr = child.stderr.take();
    let tx_err = tx.clone();
    let err_thread = stderr.map(|err| {
        std::thread::spawn(move || {
            let _ = for_each_lossy_line(BufReader::new(err), |line| {
                send_output_line(&tx_err, line);
                true
            });
        })
    });

    if let Some(stdout) = child.stdout.take() {
        let _ = for_each_lossy_line(BufReader::new(stdout), |line| {
            if kill.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = child.kill();
                return false;
            }
            if let Some(p) = parse_progress_line(&line) {
                let _ = tx.send(Event::Progress(p));
            } else if !line.trim().is_empty() {
                send_output_line(&tx, line);
            }
            true
        });
    }

    if let Some(t) = err_thread {
        let _ = t.join();
    }

    let status = child.wait();
    let (success, code) = match status {
        Ok(s) => (s.success(), s.code()),
        Err(_) => (false, None),
    };
    let _ = tx.send(Event::Finished { success, code });
}

/// Read line-oriented subprocess output without assuming it is valid UTF-8.
/// yt-dlp may inherit a Windows code page despite being attached to a pipe;
/// lossy decoding keeps the pipe drained instead of silently closing it.
fn for_each_lossy_line<R, F>(mut reader: R, mut callback: F) -> std::io::Result<()>
where
    R: BufRead,
    F: FnMut(String) -> bool,
{
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        if reader.read_until(b'\n', &mut bytes)? == 0 {
            break;
        }
        while matches!(bytes.last(), Some(b'\n' | b'\r')) {
            bytes.pop();
        }
        if !callback(String::from_utf8_lossy(&bytes).into_owned()) {
            break;
        }
    }
    Ok(())
}

/// Terminate a process and all of its descendants.
#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    let mut cmd = Command::new("taskkill");
    cmd.args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.status();
}

#[cfg(not(windows))]
fn kill_process_tree(pid: u32) {
    // Best effort: yt-dlp forwards termination to its children on Unix.
    let _ = Command::new("kill")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn send_output_line(tx: &Sender<Event>, line: String) {
    let event = if line.trim_start().starts_with("ERROR:") {
        Event::Error(line)
    } else {
        Event::Log(line)
    };
    let _ = tx.send(event);
}

/// Convenience: where the runner expects the ffmpeg directory (the bin dir).
pub fn ffmpeg_dir_of(bin_dir: &Path) -> PathBuf {
    bin_dir.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_progress() {
        let line = "CATCHYT_PROG| 42.5%|1.20MiB/s|00:12|5.00MiB|11.8MiB|6|13|My Song";
        let p = parse_progress_line(line).expect("should parse");
        assert!((p.percent - 42.5).abs() < 0.001);
        assert_eq!(p.percent_str, "42.5%");
        assert_eq!(p.speed, "1.20MiB/s");
        assert_eq!(p.eta, "00:12");
        assert_eq!(p.total, "11.8MiB");
        assert_eq!(p.item_index, Some(6));
        assert_eq!(p.item_count, Some(13));
        assert_eq!(p.title, "My Song");
    }

    #[test]
    fn ignores_non_progress_lines() {
        assert!(parse_progress_line("[ExtractAudio] Destination: x.mp3").is_none());
        assert!(parse_progress_line("[download] 100% of 3MiB").is_none());
    }

    #[test]
    fn tolerates_unknown_percent() {
        // yt-dlp emits "NA" for percent on some streams.
        let line = "CATCHYT_PROG|NA|Unknown|Unknown|NA|NA|NA|NA|Live";
        let p = parse_progress_line(line).expect("should still parse");
        assert_eq!(p.percent, 0.0);
        assert_eq!(p.item_index, None);
        assert_eq!(p.item_count, None);
        assert_eq!(p.title, "Live");
    }

    #[test]
    fn title_may_contain_pipes() {
        let line = "CATCHYT_PROG|100%|0|0|3MiB|3MiB|2|4|Artist | Track | Live";
        let p = parse_progress_line(line).unwrap();
        assert_eq!(p.title, "Artist | Track | Live");
    }

    #[test]
    fn computes_overall_playlist_fraction() {
        let p = Progress {
            percent: 50.0,
            item_index: Some(3),
            item_count: Some(4),
            ..Progress::default()
        };
        assert!((p.overall_fraction().unwrap() - 0.625).abs() < 0.001);

        let single = Progress {
            item_index: Some(1),
            item_count: Some(1),
            ..Progress::default()
        };
        assert_eq!(single.overall_fraction(), None);
    }

    #[test]
    fn lossy_reader_keeps_draining_non_utf8_output() {
        let input = b"Destination: Caf\xe9.mp3\r\nnext line\n";
        let mut lines = Vec::new();
        for_each_lossy_line(&input[..], |line| {
            lines.push(line);
            true
        })
        .unwrap();

        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("Destination: Caf"));
        assert_eq!(lines[1], "next line");
    }

    #[test]
    fn cancel_after_failed_spawn_is_harmless() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let handle = spawn(
            Path::new("definitely-missing-yt-dlp-binary"),
            Path::new("."),
            &DownloadOptions::default(),
            "https://www.youtube.com/watch?v=x",
            tx,
        );
        // child_id is 0 when the process never started; cancel must not
        // taskkill PID 0 (which would target the system idle process tree).
        handle.cancel();
        assert!(matches!(rx.recv().unwrap(), Event::SpawnError(_)));
        assert!(matches!(
            rx.recv().unwrap(),
            Event::Finished { success: false, .. }
        ));
    }

    #[test]
    fn classifies_explicit_error_lines() {
        let (tx, rx) = crossbeam_channel::unbounded();
        send_output_line(&tx, "ERROR: invalid argument".to_string());
        assert!(matches!(rx.recv().unwrap(), Event::Error(_)));
    }
}

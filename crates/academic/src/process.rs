//! Running an external program with a time limit, a cancel flag and bounded output capture.
//!
//! TeX runs can loop forever (`\loop\repeat`) or wait for input, so every child gets a closed
//! standard input, a deadline, and is killed together with its own children (latexmk starts
//! pdflatex and biber) when the deadline passes or the user cancels.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The most output kept from each of a child's standard output and standard error. Anything past
/// it is read and dropped, so a chatty run can't exhaust memory or block on a full pipe.
pub const MAX_CAPTURE: usize = 4 * 1024 * 1024;

/// How often the child is polled while it runs.
const POLL: Duration = Duration::from_millis(50);

/// What a finished (or stopped) run produced.
#[derive(Debug, Clone)]
pub struct RunOutput {
    /// The exit status; `None` when the run was stopped (timeout or cancel) or its status is unknown.
    pub status: Option<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
    pub elapsed: Duration,
}

impl RunOutput {
    pub fn success(&self) -> bool {
        !self.timed_out && !self.cancelled && self.status.is_some_and(|s| s.success())
    }
}

/// Run `cmd` to completion, for at most `timeout`, stopping early when `cancel` is set.
/// Standard input is closed and both output streams are captured (up to [`MAX_CAPTURE`] each).
pub fn run(mut cmd: Command, timeout: Duration, cancel: Option<&Arc<AtomicBool>>) -> std::io::Result<RunOutput> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Its own process group on Unix, so a timeout can stop latexmk *and* the pdflatex it started.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let start = Instant::now();
    let mut child = cmd.spawn()?;
    let out = child.stdout.take().map(capture);
    let err = child.stderr.take().map(capture);
    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break Some(status),
            None => {
                if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                    cancelled = true;
                } else if start.elapsed() >= timeout {
                    timed_out = true;
                }
                if cancelled || timed_out {
                    kill_tree(&mut child);
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(POLL);
            }
        }
    };
    let stdout = out.map(join_capture).unwrap_or_default();
    let stderr = err.map(join_capture).unwrap_or_default();
    Ok(RunOutput { status, stdout, stderr, timed_out, cancelled, elapsed: start.elapsed() })
}

fn capture<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = MAX_CAPTURE.saturating_sub(kept.len());
                    if let Some(chunk) = buf.get(..n.min(room)) {
                        kept.extend_from_slice(chunk);
                    }
                }
            }
        }
        kept
    })
}

fn join_capture(handle: std::thread::JoinHandle<Vec<u8>>) -> String {
    handle.join().map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default()
}

/// Stop the child and everything it started. Best effort: falls back to killing the child alone.
fn kill_tree(child: &mut Child) {
    let pid = child.id();
    #[cfg(unix)]
    {
        // The child leads its own process group (`process_group(0)`), whose id is its pid.
        let group = format!("-{pid}");
        let _ = Command::new("kill").args(["-KILL", "--", &group]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(any(unix, windows)))]
    let _ = pid;
    let _ = child.kill();
}

/// The first line a program prints for `--version`, or `None` if it can't be run.
pub fn version_line(program: &Path) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.arg("--version");
    let out = run(cmd, Duration::from_secs(15), None).ok()?;
    if out.timed_out {
        return None;
    }
    let text = if out.stdout.trim().is_empty() { &out.stderr } else { &out.stdout };
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.chars().take(200).collect())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_status() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo out; echo err >&2; exit 3"]);
        let out = run(cmd, Duration::from_secs(10), None).unwrap();
        assert_eq!(out.stdout.trim(), "out");
        assert_eq!(out.stderr.trim(), "err");
        assert_eq!(out.status.and_then(|s| s.code()), Some(3));
        assert!(!out.success());
    }

    #[test]
    fn a_run_past_its_deadline_is_stopped_with_its_children() {
        let mut cmd = Command::new("sh");
        // The child shell starts a grandchild that would outlive a plain kill of the shell.
        cmd.args(["-c", "sleep 30 & wait"]);
        let start = Instant::now();
        let out = run(cmd, Duration::from_millis(300), None).unwrap();
        assert!(out.timed_out);
        assert!(!out.success());
        assert!(start.elapsed() < Duration::from_secs(10), "the run was not stopped: {:?}", start.elapsed());
    }

    #[test]
    fn cancel_stops_a_run() {
        let flag = Arc::new(AtomicBool::new(true));
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30"]);
        let out = run(cmd, Duration::from_secs(60), Some(&flag)).unwrap();
        assert!(out.cancelled);
        assert!(!out.timed_out);
    }

    #[test]
    fn standard_input_is_closed() {
        let mut cmd = Command::new("sh");
        // `read` fails at once on a closed stdin instead of waiting for the deadline.
        cmd.args(["-c", "read x; echo done"]);
        let out = run(cmd, Duration::from_secs(10), None).unwrap();
        assert!(!out.timed_out);
        assert_eq!(out.stdout.trim(), "done");
    }
}

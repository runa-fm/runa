//! Command runner for runa.
//!
//! Runs editor commands that do not need the terminal, so the TUI keeps running.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded, never, select};

use crate::core::workers::{CommandMode, CommandTask, RunnerCommand, WorkerResponse};

/// Bytes of stderr kept for the error message.
const MESSAGE_KEEP_BYTES: u64 = 4096;

/// Max lines shown in a non-modal message toast.
const MESSAGE_MAX_LINES: usize = 12;

// Max bytes kept per stream for the overlay output.
const OVERLAY_MAX_BYTES: u64 = 128 * 1024;

const CANCEL_POLL: Duration = Duration::from_millis(25);

type ActiveMap = Arc<Mutex<HashMap<u64, Sender<()>>>>;

struct ActiveCommandGuard {
    id: u64,
    active: ActiveMap,
}

impl Drop for ActiveCommandGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = self.active.lock() {
            map.remove(&self.id);
        }
    }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(super) fn start(cmd_rx: Receiver<RunnerCommand>, res_tx: Sender<WorkerResponse>) {
    thread::spawn(move || {
        let active: ActiveMap = Arc::new(Mutex::new(HashMap::new()));

        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                RunnerCommand::Run(CommandTask {
                    program,
                    args,
                    dir,
                    mode,
                }) => {
                    let name = program
                        .file_stem()
                        .unwrap_or(program.as_os_str())
                        .to_string_lossy()
                        .into_owned();

                    match spawn(&program, &args, dir.as_deref(), mode) {
                        Ok(proc) => {
                            let cancel = match mode {
                                CommandMode::Overlay { id, .. } => {
                                    let (cancel_tx, cancel_rx) = bounded(0);
                                    if let Ok(mut map) = active.lock() {
                                        map.insert(id, cancel_tx);
                                    }
                                    Some((
                                        cancel_rx,
                                        ActiveCommandGuard {
                                            id,
                                            active: Arc::clone(&active),
                                        },
                                    ))
                                }
                                CommandMode::Background { .. } | CommandMode::Message => None,
                            };
                            let res_tx = res_tx.clone();
                            thread::spawn(move || wait_for(proc, &name, mode, cancel, &res_tx));
                        }
                        Err(e) => send_spawn_error(&name, e, mode, &res_tx),
                    }
                }
                RunnerCommand::Cancel(id) => {
                    if let Ok(mut map) = active.lock() {
                        map.remove(&id);
                    }
                }
            }
        }

        if let Ok(mut map) = active.lock() {
            map.clear();
        }
    });
}

fn send_spawn_error(
    name: &str,
    err: io::Error,
    mode: CommandMode,
    res_tx: &Sender<WorkerResponse>,
) {
    let msg = format!("{name}: {err}");

    let response = match mode {
        CommandMode::Overlay { id, tab_id } => WorkerResponse::CommandOutput {
            id,
            lines: vec![msg],
            success: false,
            exit_code: None,
            truncated: false,
            tab_id,
        },
        CommandMode::Background { .. } | CommandMode::Message => WorkerResponse::Error(msg),
    };
    let _ = res_tx.send(response);
}

/// Spawns a non-terminal command with null stdin and configured stdio.
/// On Unix, child processes may still open `/dev/tty`, so common prompt env vars are disabled for output modes.
fn spawn(
    program: &Path,
    args: &[OsString],
    dir: Option<&Path>,
    mode: CommandMode,
) -> io::Result<Child> {
    let (stdout, stderr) = match mode {
        CommandMode::Background {
            check_status: false,
        } => (Stdio::null(), Stdio::null()),
        CommandMode::Background { check_status: true } => (Stdio::null(), Stdio::piped()),
        CommandMode::Message | CommandMode::Overlay { .. } => (Stdio::piped(), Stdio::piped()),
    };

    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);

    if let Some(dir) = dir {
        command.current_dir(dir);
    }

    if matches!(mode, CommandMode::Message | CommandMode::Overlay { .. }) {
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never");
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(unix)]
    if matches!(mode, CommandMode::Overlay { .. }) {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command.spawn()
}

fn cancel_tree(proc: &mut Child) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group};
        let _ = kill_process_group(Pid::from_child(proc), Signal::KILL);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &proc.id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }

    let _ = proc.kill();
    let _ = proc.wait();
}

/// Read up to limit bytes and drains the rest.
fn read_capped(mut reader: impl Read, limit: u64) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let _ = reader.by_ref().take(limit).read_to_end(&mut buf);
    let drained = io::copy(&mut reader, &mut io::sink()).unwrap_or(0);
    (buf, drained > 0)
}

fn first_non_empty_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn collect_message_lines(primary: &[u8], fallback: &[u8], truncated: bool) -> Option<String> {
    let source = if primary.iter().any(|b| !b.is_ascii_whitespace()) {
        primary
    } else {
        fallback
    };

    let text = String::from_utf8_lossy(source);
    let mut lines = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty());

    let mut kept: Vec<&str> = lines.by_ref().take(MESSAGE_MAX_LINES).collect();
    if kept.is_empty() {
        return None;
    }
    if truncated || lines.next().is_some() {
        kept.push("...");
    }
    Some(kept.join("\n"))
}

fn capture_streams(proc: &mut Child, limit: u64) -> (Vec<u8>, Vec<u8>, bool) {
    let stderr_handle = proc
        .stderr
        .take()
        .map(|stderr| thread::spawn(move || read_capped(stderr, limit)));

    let (stdout_buf, stdout_trunc) = proc
        .stdout
        .take()
        .map(|stdout| read_capped(stdout, limit))
        .unwrap_or_default();

    let (stderr_buf, stderr_trunc) = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    (stdout_buf, stderr_buf, stdout_trunc || stderr_trunc)
}

fn capture_streams_cancellable(
    proc: &mut Child,
    limit: u64,
    cancel: &Receiver<()>,
) -> Option<(Vec<u8>, Vec<u8>, bool)> {
    let out_rx = proc.stdout.take().map(|stdout| {
        let (tx, rx) = bounded(1);
        thread::spawn(move || {
            let _ = tx.send(read_capped(stdout, limit));
        });
        rx
    });
    let err_rx = proc.stderr.take().map(|stderr| {
        let (tx, rx) = bounded(1);
        thread::spawn(move || {
            let _ = tx.send(read_capped(stderr, limit));
        });
        rx
    });

    let idle = never::<(Vec<u8>, bool)>();
    let mut stdout = out_rx.is_none().then(Default::default);
    let mut stderr = err_rx.is_none().then(Default::default);

    while stdout.is_none() || stderr.is_none() {
        let out_sel = match (&out_rx, &stdout) {
            (Some(rx), None) => rx,
            _ => &idle,
        };
        let err_sel = match (&err_rx, &stderr) {
            (Some(rx), None) => rx,
            _ => &idle,
        };

        select! {
            recv(cancel) -> _ => {
                cancel_tree(proc);
                return None;
            }
            recv(out_sel) -> res => {
                stdout = Some(res.unwrap_or_default());
            }
            recv(err_sel) -> res => {
                stderr = Some(res.unwrap_or_default());
            }
        }
    }

    let (stdout_buf, stdout_trunc) = stdout.unwrap_or_default();
    let (stderr_buf, stderr_trunc) = stderr.unwrap_or_default();
    Some((stdout_buf, stderr_buf, stdout_trunc || stderr_trunc))
}

fn wait_cancellable(proc: &mut Child, cancel: &Receiver<()>) -> Option<io::Result<ExitStatus>> {
    loop {
        match proc.try_wait() {
            Ok(Some(status)) => return Some(Ok(status)),
            Ok(None) => {}
            Err(e) => return Some(Err(e)),
        }
        match cancel.recv_timeout(CANCEL_POLL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                cancel_tree(proc);
                return None;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn build_overlay_lines(stdout: &[u8], stderr: &[u8]) -> Vec<String> {
    let mut lines: Vec<String> = String::from_utf8_lossy(stdout)
        .lines()
        .map(str::to_owned)
        .collect();

    let stderr_lines: Vec<String> = String::from_utf8_lossy(stderr)
        .lines()
        .map(str::to_owned)
        .collect();

    if !stderr_lines.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }

        lines.extend(stderr_lines);
    }

    lines
}

/// Waits for the command and reports a failure with the first line of its stderr.
fn wait_for(
    mut proc: Child,
    name: &str,
    mode: CommandMode,
    cancel: Option<(Receiver<()>, ActiveCommandGuard)>,
    res_tx: &Sender<WorkerResponse>,
) {
    match mode {
        CommandMode::Background { check_status } => {
            let (stderr, _) = proc
                .stderr
                .take()
                .map(|s| read_capped(s, MESSAGE_KEEP_BYTES))
                .unwrap_or_default();

            let message = match proc.wait() {
                Ok(_) if !check_status => return,
                Ok(status) if status.success() => return,
                Ok(status) => match first_non_empty_line(&stderr) {
                    Some(line) => format!("{name}: {status}: {line}"),
                    None => format!("{name}: {status}"),
                },
                Err(e) => format!("{name}: {e}"),
            };

            let _ = res_tx.send(WorkerResponse::Error(message));
        }
        CommandMode::Message => {
            let (stdout, stderr, truncated) = capture_streams(&mut proc, MESSAGE_KEEP_BYTES);

            let response = match proc.wait() {
                Ok(status) if status.success() => {
                    let msg = match collect_message_lines(&stdout, &stderr, truncated) {
                        Some(text) if text.contains('\n') => format!("{name}:\n{text}"),
                        Some(line) => format!("{name}: {}", line.trim()),
                        None => format!("{name}: done"),
                    };
                    WorkerResponse::Message(msg)
                }
                Ok(status) => {
                    let msg = match collect_message_lines(&stderr, &stdout, truncated) {
                        Some(text) if text.contains('\n') => format!("{name}: {status}\n{text}"),
                        Some(line) => format!("{name}: {status}: {}", line.trim()),
                        None => format!("{name}: {status}"),
                    };
                    WorkerResponse::Error(msg)
                }
                Err(e) => WorkerResponse::Error(format!("{name}: {e}")),
            };
            let _ = res_tx.send(response);
        }
        CommandMode::Overlay { id, tab_id } => {
            let (stdout, stderr, truncated, status) = if let Some((cancel_rx, _guard)) = cancel {
                let Some((stdout, stderr, truncated)) =
                    capture_streams_cancellable(&mut proc, OVERLAY_MAX_BYTES, &cancel_rx)
                else {
                    return;
                };
                let Some(status) = wait_cancellable(&mut proc, &cancel_rx) else {
                    return;
                };
                (stdout, stderr, truncated, status)
            } else {
                let (stdout, stderr, truncated) = capture_streams(&mut proc, OVERLAY_MAX_BYTES);
                (stdout, stderr, truncated, proc.wait())
            };

            let (lines, success, exit_code) = match status {
                Ok(status) => (
                    build_overlay_lines(&stdout, &stderr),
                    status.success(),
                    status.code(),
                ),
                Err(e) => (vec![format!("{name}: {e}")], false, None),
            };

            let _ = res_tx.send(WorkerResponse::CommandOutput {
                id,
                lines,
                success,
                exit_code,
                truncated,
                tab_id,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use crossbeam_channel::unbounded;
    use tempfile::{TempDir, tempdir};

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn start_runner() -> (Sender<RunnerCommand>, Receiver<WorkerResponse>, TempDir) {
        let dir = tempdir().expect("temp dir is created");
        let (cmd_tx, cmd_rx) = unbounded();
        let (res_tx, res_rx) = unbounded();
        start(cmd_rx, res_tx);
        (cmd_tx, res_rx, dir)
    }

    fn make_task(program: &str, args: &[&str], dir: &Path, mode: CommandMode) -> CommandTask {
        CommandTask {
            program: PathBuf::from(program),
            args: args.iter().map(OsString::from).collect(),
            dir: Some(dir.to_path_buf()),
            mode,
        }
    }

    fn run_mode(
        program: &str,
        args: &[&str],
        mode: CommandMode,
    ) -> (Receiver<WorkerResponse>, TempDir) {
        let (cmd_tx, res_rx, dir) = start_runner();
        let task = make_task(program, args, dir.path(), mode);
        cmd_tx
            .send(RunnerCommand::Run(task))
            .expect("runner thread is running");
        (res_rx, dir)
    }

    fn run(
        program: &str,
        args: &[&str],
        check_status: bool,
    ) -> (Receiver<WorkerResponse>, TempDir) {
        run_mode(program, args, CommandMode::Background { check_status })
    }

    fn error_message(res_rx: &Receiver<WorkerResponse>) -> String {
        match res_rx.recv_timeout(TIMEOUT) {
            Ok(WorkerResponse::Error(message)) => message,
            other => panic!("expected an error, got {other:?}"),
        }
    }

    // Disconnected means every sender is gone without sending anything
    fn assert_nothing_sent(res_rx: &Receiver<WorkerResponse>) {
        assert_eq!(
            res_rx.recv_timeout(TIMEOUT).err(),
            Some(RecvTimeoutError::Disconnected)
        );
    }

    #[cfg(unix)]
    #[test]
    fn failure_reports_the_first_stderr_line() {
        let (res_rx, _dir) = run(
            "sh",
            &["-c", "echo first >&2; echo second >&2; exit 3"],
            true,
        );
        assert_eq!(error_message(&res_rx), "sh: exit status: 3: first");
    }

    #[cfg(windows)]
    #[test]
    fn failure_reports_the_first_stderr_line() {
        let (res_rx, _dir) = run(
            "cmd",
            &["/C", "echo first 1>&2 & echo second 1>&2 & exit /B 3"],
            true,
        );
        assert_eq!(error_message(&res_rx), "cmd: exit code: 3: first");
    }

    #[cfg(unix)]
    #[test]
    fn success_sends_nothing() {
        let (res_rx, _dir) = run("sh", &["-c", "echo ignored; exit 0"], true);
        assert_nothing_sent(&res_rx);
    }

    #[cfg(windows)]
    #[test]
    fn success_sends_nothing() {
        let (res_rx, _dir) = run("cmd", &["/C", "echo ignored & exit /B 0"], true);
        assert_nothing_sent(&res_rx);
    }

    #[cfg(unix)]
    #[test]
    fn unchecked_failure_sends_nothing() {
        let (res_rx, _dir) = run("sh", &["-c", "echo ignored >&2; exit 1"], false);
        assert_nothing_sent(&res_rx);
    }

    #[cfg(windows)]
    #[test]
    fn unchecked_failure_sends_nothing() {
        let (res_rx, _dir) = run("cmd", &["/C", "echo ignored 1>&2 & exit /B 1"], false);
        assert_nothing_sent(&res_rx);
    }

    #[cfg(unix)]
    #[test]
    fn chatty_stderr_does_not_kill_the_command() {
        let (res_rx, _dir) = run(
            "sh",
            &["-c", "yes runa | head -c 1000000 >&2 && exit 3"],
            true,
        );
        assert_eq!(error_message(&res_rx), "sh: exit status: 3: runa");
    }

    #[test]
    fn missing_program_reports_its_name() {
        let (res_rx, _dir) = run("runa-no-such-program", &[], false);
        assert!(error_message(&res_rx).starts_with("runa-no-such-program: "));
    }

    #[cfg(unix)]
    #[test]
    fn message_mode_reports_stdout_or_stderr() {
        let (res_rx, _dir) = run_mode("sh", &["-c", "echo pushed >&2"], CommandMode::Message);
        match res_rx.recv_timeout(TIMEOUT) {
            Ok(WorkerResponse::Message(msg)) => assert_eq!(msg, "sh: pushed"),
            other => panic!("expected message, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn message_mode_keeps_multiline_output_up_to_cap() {
        let (res_rx, _dir) = run_mode(
            "sh",
            &[
                "-c",
                "printf 'To remote\\n   abc..def  main -> main\\n' >&2",
            ],
            CommandMode::Message,
        );
        match res_rx.recv_timeout(TIMEOUT) {
            Ok(WorkerResponse::Message(msg)) => {
                assert_eq!(msg, "sh:\nTo remote\n   abc..def  main -> main");
            }
            other => panic!("expected message, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn overlay_mode_captures_stdout_and_stderr() {
        let (cmd_tx, res_rx, dir) = start_runner();
        let task = make_task(
            "sh",
            &["-c", "echo out; echo err >&2"],
            dir.path(),
            CommandMode::Overlay {
                id: 7,
                tab_id: Some(1),
            },
        );
        cmd_tx
            .send(RunnerCommand::Run(task))
            .expect("runner thread is running");

        match res_rx.recv_timeout(TIMEOUT) {
            Ok(WorkerResponse::CommandOutput {
                id,
                lines,
                success,
                exit_code,
                truncated,
                tab_id,
            }) => {
                assert_eq!(id, 7);
                assert_eq!(tab_id, Some(1));
                assert!(success);
                assert_eq!(exit_code, Some(0));
                assert!(!truncated);
                assert_eq!(lines, vec!["out", "", "err"]);
            }
            other => panic!("expected command output, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn overlay_mode_cancel_stops_running_command() {
        let (cmd_tx, res_rx, dir) = start_runner();
        let task = make_task(
            "sh",
            &["-c", "sleep 0.4 && echo survived"],
            dir.path(),
            CommandMode::Overlay {
                id: 42,
                tab_id: Some(0),
            },
        );
        cmd_tx
            .send(RunnerCommand::Run(task))
            .expect("runner thread is running");
        cmd_tx
            .send(RunnerCommand::Cancel(42))
            .expect("runner thread is running");

        assert_eq!(
            res_rx.recv_timeout(Duration::from_millis(600)).err(),
            Some(RecvTimeoutError::Timeout)
        );

        drop(cmd_tx);
        assert_nothing_sent(&res_rx);
    }

    #[test]
    fn overlay_mode_missing_program_still_responds() {
        let (res_rx, _dir) = run_mode(
            "runa-no-such-program",
            &[],
            CommandMode::Overlay {
                id: 9,
                tab_id: None,
            },
        );
        match res_rx.recv_timeout(TIMEOUT) {
            Ok(WorkerResponse::CommandOutput {
                id,
                lines,
                success,
                exit_code,
                ..
            }) => {
                assert_eq!(id, 9);
                assert!(!success);
                assert_eq!(exit_code, None);
                assert!(lines[0].starts_with("runa-no-such-program: "));
            }
            other => panic!("expected command output, got {other:?}"),
        }
    }
}

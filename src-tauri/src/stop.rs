//! Supervising child processes (design §9.1): stopping one with escalating
//! signals, and capturing a short-lived command's output with a time limit.
//! Platform-neutral; the actual signalling is behind `platform::ProcessCtl`.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::platform::Signal;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// How long to wait, after a command exits, for its output pipe to close
/// before settling for what was already read (a background process started
/// by a shell's rc file can hold the pipe open).
const OUTPUT_DRAIN_WAIT: Duration = Duration::from_millis(250);

/// Something that can be signalled and that eventually exits.
pub trait Stoppable {
    fn send(&self, signal: Signal);
    fn has_exited(&self) -> bool;
}

/// The harshest signal [`stop_gracefully`] needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    AlreadyExited,
    Interrupted,
    Terminated,
    Killed,
}

fn wait_for_exit(target: &impl Stoppable, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    loop {
        if target.has_exited() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Stops `target` as politely as it allows: an interrupt (which ends a Claude
/// turn cleanly), then after `grace` a terminate, then after another `grace`
/// a kill (design §9.1). Blocks the calling thread for at most `2 * grace`.
pub fn stop_gracefully(target: &impl Stoppable, grace: Duration) -> StopOutcome {
    if target.has_exited() {
        return StopOutcome::AlreadyExited;
    }
    target.send(Signal::Interrupt);
    if wait_for_exit(target, grace) {
        return StopOutcome::Interrupted;
    }
    target.send(Signal::Terminate);
    if wait_for_exit(target, grace) {
        return StopOutcome::Terminated;
    }
    target.send(Signal::Kill);
    StopOutcome::Killed
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub stdout: String,
    pub success: bool,
}

/// Runs `command` and returns what it printed, or `None` if it can't be
/// started or doesn't finish within `timeout` (it is killed then). stdin is
/// closed and stderr discarded.
pub fn capture_stdout(mut command: Command, timeout: Duration) -> Option<Captured> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let mut pipe = child.stdout.take()?;

    let output = Arc::new(Mutex::new(Vec::new()));
    let (closed_tx, closed_rx) = mpsc::channel();
    let sink = Arc::clone(&output);
    thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&chunk[..read]);
        }
        let _ = closed_tx.send(());
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let _ = closed_rx.recv_timeout(OUTPUT_DRAIN_WAIT);
    let bytes = output.lock().unwrap().clone();
    Some(Captured {
        stdout: String::from_utf8_lossy(&bytes).into_owned(),
        success: status.success(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    const GRACE: Duration = Duration::from_millis(40);

    /// Records the signals it gets and "exits" when it receives `exits_on`.
    struct FakeTarget {
        exits_on: Option<Signal>,
        sent: Mutex<Vec<Signal>>,
        exited: AtomicBool,
    }

    impl FakeTarget {
        fn new(exits_on: Option<Signal>) -> Self {
            FakeTarget {
                exits_on,
                sent: Mutex::new(Vec::new()),
                exited: AtomicBool::new(false),
            }
        }

        fn sent(&self) -> Vec<Signal> {
            self.sent.lock().unwrap().clone()
        }
    }

    impl Stoppable for FakeTarget {
        fn send(&self, signal: Signal) {
            self.sent.lock().unwrap().push(signal);
            if self.exits_on == Some(signal) {
                self.exited.store(true, Ordering::SeqCst);
            }
        }

        fn has_exited(&self) -> bool {
            self.exited.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn a_process_that_already_exited_gets_no_signals() {
        let target = FakeTarget::new(None);
        target.exited.store(true, Ordering::SeqCst);

        assert_eq!(stop_gracefully(&target, GRACE), StopOutcome::AlreadyExited);
        assert!(target.sent().is_empty());
    }

    #[test]
    fn an_interrupt_is_enough_for_a_well_behaved_process() {
        let target = FakeTarget::new(Some(Signal::Interrupt));

        assert_eq!(stop_gracefully(&target, GRACE), StopOutcome::Interrupted);
        assert_eq!(target.sent(), vec![Signal::Interrupt]);
    }

    #[test]
    fn it_terminates_a_process_that_ignores_the_interrupt() {
        let target = FakeTarget::new(Some(Signal::Terminate));

        assert_eq!(stop_gracefully(&target, GRACE), StopOutcome::Terminated);
        assert_eq!(target.sent(), vec![Signal::Interrupt, Signal::Terminate]);
    }

    #[test]
    fn it_kills_a_process_that_ignores_everything_else() {
        let target = FakeTarget::new(Some(Signal::Kill));
        let started = Instant::now();

        assert_eq!(stop_gracefully(&target, GRACE), StopOutcome::Killed);

        assert_eq!(
            target.sent(),
            vec![Signal::Interrupt, Signal::Terminate, Signal::Kill]
        );
        // One grace period per escalation step, and no more than that.
        assert!(started.elapsed() >= GRACE * 2);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn it_returns_as_soon_as_the_process_exits_within_the_grace_period() {
        let target = FakeTarget::new(None);
        let started = Instant::now();
        thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(30));
                target.exited.store(true, Ordering::SeqCst);
            });

            assert_eq!(
                stop_gracefully(&target, Duration::from_secs(5)),
                StopOutcome::Interrupted
            );
        });

        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(target.sent(), vec![Signal::Interrupt]);
    }

    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn capture_returns_stdout_and_success() {
        let captured = capture_stdout(shell("printf hello"), Duration::from_secs(5)).unwrap();

        assert_eq!(
            captured,
            Captured {
                stdout: "hello".into(),
                success: true
            }
        );
    }

    #[test]
    fn capture_reports_a_failing_exit_status_with_its_output() {
        let captured = capture_stdout(
            shell(r#"echo '{"loggedIn":false}'; exit 1"#),
            Duration::from_secs(5),
        )
        .unwrap();

        assert!(!captured.success);
        assert!(captured.stdout.contains("loggedIn"));
    }

    #[test]
    fn capture_gives_up_and_kills_a_command_that_hangs() {
        let started = Instant::now();

        assert_eq!(
            capture_stdout(shell("sleep 30"), Duration::from_millis(100)),
            None
        );

        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn capture_returns_none_when_the_command_cannot_start() {
        let command = Command::new("/definitely/not/a/real/program");

        assert_eq!(capture_stdout(command, Duration::from_secs(1)), None);
    }

    #[test]
    fn capture_is_not_held_up_by_a_background_process_keeping_the_pipe_open() {
        let started = Instant::now();

        let captured =
            capture_stdout(shell("echo hi; sleep 5 &"), Duration::from_secs(10)).unwrap();

        assert_eq!(captured.stdout, "hi\n");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn capture_tolerates_non_utf8_output() {
        let captured = capture_stdout(shell(r"printf 'a\377b'"), Duration::from_secs(5)).unwrap();

        assert!(captured.stdout.starts_with('a') && captured.stdout.ends_with('b'));
    }
}

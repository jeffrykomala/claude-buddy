//! Starting a child in its own process group and signalling the whole group
//! (design §9.1): stopping `claude` must also stop the shell commands it
//! started, so we signal the group, not just the one process.

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

use crate::platform::{ProcessCtl, Signal};

#[derive(Debug, Default)]
pub struct MacProcessCtl;

impl ProcessCtl for MacProcessCtl {
    fn spawn_group(&self, mut command: Command) -> io::Result<Child> {
        command.process_group(0);
        command.spawn()
    }

    fn signal_group(&self, pgid: u32, signal: Signal) {
        let signal = match signal {
            Signal::Interrupt => libc::SIGINT,
            Signal::Terminate => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        // SAFETY: killpg only takes integers. A group that is already gone
        // makes it fail with ESRCH, which is exactly "nothing left to stop".
        unsafe {
            libc::killpg(pgid as libc::pid_t, signal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    fn is_alive(pid: i32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    fn wait_until(deadline: Duration, mut done: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + deadline;
        while Instant::now() < end {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        done()
    }

    /// A shell that starts a background `sleep` and prints its pid.
    fn spawn_shell_with_background_sleep() -> (Child, i32) {
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 60 & echo $!; wait"])
            .stdout(Stdio::piped());
        let mut child = MacProcessCtl.spawn_group(command).unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        (child, line.trim().parse().unwrap())
    }

    #[test]
    fn the_child_leads_its_own_process_group() {
        let mut command = Command::new("sleep");
        command.arg("60");
        let mut child = MacProcessCtl.spawn_group(command).unwrap();

        // SAFETY: getpgid only reads.
        let group = unsafe { libc::getpgid(child.id() as i32) };

        assert_eq!(group, child.id() as i32);
        MacProcessCtl.signal_group(child.id(), Signal::Kill);
        child.wait().unwrap();
    }

    #[test]
    fn killing_the_group_also_stops_what_the_child_started() {
        let (mut child, background_pid) = spawn_shell_with_background_sleep();
        assert!(is_alive(background_pid));

        MacProcessCtl.signal_group(child.id(), Signal::Kill);
        child.wait().unwrap();

        assert!(
            wait_until(Duration::from_secs(2), || !is_alive(background_pid)),
            "the background sleep should have died with its group"
        );
    }

    #[test]
    fn an_interrupt_ends_a_waiting_shell() {
        let (mut child, background_pid) = spawn_shell_with_background_sleep();

        MacProcessCtl.signal_group(child.id(), Signal::Interrupt);
        let status = child.wait().unwrap();

        assert!(!status.success(), "the interrupt should end the shell");
        // A non-interactive shell's background jobs ignore SIGINT (POSIX), so
        // the sleep may survive an interrupt: that is exactly why stopping a
        // run escalates to terminate and kill. Clean it up the same way.
        MacProcessCtl.signal_group(child.id(), Signal::Kill);
        assert!(wait_until(Duration::from_secs(2), || !is_alive(
            background_pid
        )));
    }

    #[test]
    fn terminate_ends_the_group_too() {
        let (mut child, background_pid) = spawn_shell_with_background_sleep();

        MacProcessCtl.signal_group(child.id(), Signal::Terminate);
        child.wait().unwrap();

        assert!(wait_until(Duration::from_secs(2), || !is_alive(
            background_pid
        )));
    }

    #[test]
    fn signalling_a_group_that_is_gone_does_nothing() {
        let mut child = MacProcessCtl.spawn_group(Command::new("true")).unwrap();
        let pid = child.id();
        child.wait().unwrap();

        MacProcessCtl.signal_group(pid, Signal::Kill);
    }
}

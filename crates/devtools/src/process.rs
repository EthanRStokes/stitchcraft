//! A child process whose output arrives as lines, and which can be stopped
//! along with everything it started.
//!
//! Gradle starts the game as a grandchild, so stopping only the wrapper would
//! leave Minecraft running. On Unix the child leads its own process group and
//! the whole group is signalled; on Windows `taskkill /T` walks the tree.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

/// A running child and its output.
pub struct Job {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    stopping: bool,
}

impl Job {
    /// Starts `command` with piped output.
    pub fn spawn(mut command: Command) -> std::io::Result<Job> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        let (sender, lines) = channel();
        if let Some(out) = child.stdout.take() {
            pump(out, sender.clone());
        }
        if let Some(err) = child.stderr.take() {
            pump(err, sender);
        }
        let stdin = child.stdin.take();
        Ok(Job {
            child,
            stdin,
            lines,
            stopping: false,
        })
    }

    /// The lines printed since the last call.
    pub fn take_lines(&mut self) -> Vec<String> {
        self.lines.try_iter().collect()
    }

    /// The exit status once the process has ended.
    pub fn finished(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// True once [`Job::stop`] was called.
    pub fn stopping(&self) -> bool {
        self.stopping
    }

    /// Sends a line to the process's input, like typing a server command.
    pub fn send(&mut self, line: &str) -> std::io::Result<()> {
        match self.stdin.as_mut() {
            Some(stdin) => {
                stdin.write_all(line.as_bytes())?;
                stdin.write_all(b"\n")?;
                stdin.flush()
            }
            None => Ok(()),
        }
    }

    /// Asks the process and everything it started to stop. A second call
    /// stops them without asking.
    pub fn stop(&mut self) {
        let force = self.stopping;
        self.stopping = true;
        let pid = self.child.id();
        #[cfg(unix)]
        unsafe {
            // The child leads its own group, so -pid reaches the game too.
            libc::kill(-(pid as i32), if force { libc::SIGKILL } else { libc::SIGTERM });
        }
        #[cfg(windows)]
        {
            let _ = force;
            let _ = Command::new("taskkill")
                .args(["/T", "/F", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (force, pid);
            let _ = self.child.kill();
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if self.finished().is_none() {
            self.stop();
            self.stop();
        }
    }
}

/// Forwards `stream` line by line until it closes. Bytes that are not UTF-8
/// (a Windows console code page) are replaced rather than ending the stream.
fn pump(stream: impl Read + Send + 'static, sender: Sender<String>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buffer);
                    let line = line.trim_end_matches(['\n', '\r']).to_string();
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait(job: &mut Job) -> (Vec<String>, ExitStatus) {
        let start = Instant::now();
        let mut lines = Vec::new();
        loop {
            lines.extend(job.take_lines());
            if let Some(status) = job.finished() {
                std::thread::sleep(Duration::from_millis(50));
                lines.extend(job.take_lines());
                return (lines, status);
            }
            assert!(start.elapsed() < Duration::from_secs(10), "job hung");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn output_arrives_as_lines_from_both_streams() {
        let mut command = Command::new("sh");
        command.args(["-c", "echo one; echo two >&2; read x; echo got $x"]);
        let mut job = Job::spawn(command).unwrap();
        job.send("three").unwrap();
        let (mut lines, status) = wait(&mut job);
        lines.sort();
        assert!(status.success());
        assert_eq!(lines, ["got three", "one", "two"]);
    }

    #[test]
    fn stopping_reaches_grandchildren() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & sleep 30; echo late"]);
        let mut job = Job::spawn(command).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        job.stop();
        let (lines, status) = wait(&mut job);
        assert!(!status.success());
        assert!(lines.is_empty());
    }
}

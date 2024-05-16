use anyhow::bail;
use tracing::{info, warn, error, info_span, debug};
use strip_ansi_escapes;
use wait_timeout::ChildExt;
use std::os::fd::FromRawFd;
use std::time::Duration;

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use nix::Error;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};

use std::os::unix::io::AsRawFd;

/// A subprocess handle that will kill the subprocess when dropped.
pub struct ProcHandle {
    span: tracing::Span,
    name: String,
    child: std::process::Child,
    threads: Vec<std::thread::JoinHandle<()>>,
    terminate_flag: Arc<AtomicBool>,
}

/// Execute a shell command (pass to 'sh') in a subprocess,
/// and log its stdout and stderr.
///
/// Returns a handle that will kill the subprocess when dropped.
pub fn spawn_shell(cmd_str: &str, name: &str, span: tracing::Span) -> anyhow::Result<ProcHandle>
{
    let sp = span.clone();
    let _entered = sp.enter();
    let name = name.to_string();

    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(cmd_str)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    info!("Spawning (shell cmd): {:?}", cmd);
    let mut child = cmd.spawn()?;

    /// Streams logs from a file descriptor (stdout/stderr of childprocess) to the main log.
    ///
    /// This is a bit convoluted because we can't block on the stdin/stdout streams,
    /// otherwise the reader.read_line() would hang when the subprocess is done, and the thread would never exit.
    fn log_stream(span: tracing::Span, fd: i32, level: tracing::Level, name: &str, terminate: Arc<AtomicBool>) {
        debug!("Starting thread to read {}->log", name);
        let _entered = span.enter();
        use std::io::BufRead;

        let mut reader = std::io::BufReader::new(unsafe { std::fs::File::from_raw_fd(fd) });
        let mut buffer = String::new();
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(128);

        poll.registry().register(&mut SourceFd(&fd), Token(0), Interest::READABLE).unwrap();

        loop {
            if terminate.load(Ordering::Relaxed) { break; }

            let timeout = Some(Duration::from_millis(100));
            poll.poll(&mut events, timeout).or_else(|e| {
                if e.kind() != std::io::ErrorKind::Interrupted { tracing::error!("Poll error: {:?}", e); };
                Err(e)
            }).ok();

            for event in events.iter() {
                match event.token() {
                    Token(0) => {
                        match reader.read_line(&mut buffer) {
                            Ok(_size) => {
                                let line = strip_ansi_escapes::strip_str(buffer.clone()).trim_end().to_string(); // Remove terminal colors and trailing linefeeds from log lines (if any)
                                if line.is_empty() { continue; }
                                match line.split_once(" ") {
                                    Some((level_str, msg_str)) => {
                                        let (level_override, prepend) = match level_str {
                                            "DEBUG" => (tracing::Level::DEBUG, std::string::String::new()),
                                            "INFO" => (tracing::Level::INFO, std::string::String::new()),
                                            "WARN" | "WARNING" => (tracing::Level::WARN, std::string::String::new()),
                                            "ERROR" | "CRITICAL" | "FATAL" => (tracing::Level::ERROR, std::string::String::new()),
                                            _ => match level {
                                                tracing::Level::INFO => (tracing::Level::INFO, " ".to_string() + level_str),
                                                tracing::Level::ERROR => (tracing::Level::ERROR, " ".to_string() + level_str),
                                                _ => panic!("Unsupported log level"),
                                            },
                                        };
                                        match level_override {
                                            tracing::Level::DEBUG => debug!("[{}] {}{}", name, prepend, msg_str),
                                            tracing::Level::INFO => info!("[{}] {}{}", name, prepend, msg_str),
                                            tracing::Level::WARN => warn!("[{}] {}{}", name, prepend, msg_str),
                                            tracing::Level::ERROR => error!("[{}] {}{}", name, prepend, msg_str),
                                            _ => panic!("Unsupported log level"),
                                        }
                                    }
                                    None => info!("[{}] {}", name, line),
                                }
                            }
                            Err(e) => {
                                error!("Failed to read {}. Bailing. -- {:?}", name, e);
                                break;
                            }
                        }
                        buffer.clear();
                    }
                    _ => unreachable!(),
                }
            }
        }

        debug!("Thread to read {}->log exiting", name);
    }

    let terminate_flag = Arc::new(AtomicBool::new(false));

    // Spawn threads to log the subprocess's stdout and stderr
    let threads = vec![
        if let Some(stdout) = child.stdout.take() {
                let span = span.clone();
                let terminate_flag = terminate_flag.clone();
                std::thread::spawn(move || log_stream(span, stdout.as_raw_fd(), tracing::Level::INFO, "stdout", terminate_flag))
            } else { bail!("Failed to capture stdout"); },
        if let Some(stderr) = child.stderr.take() {
                let span = span.clone();
                let terminate_flag = terminate_flag.clone();
                std::thread::spawn(move || log_stream(span, stderr.as_raw_fd(), tracing::Level::ERROR, "stderr", terminate_flag))
            } else { bail!("Failed to capture stderr"); }
    ];

    debug!("Subprocess spawned");
    Ok(ProcHandle { span, name, child, threads, terminate_flag })
}


/// Terminate the subprocess when Handle is dropped.
impl Drop for ProcHandle {
    fn drop(&mut self) {
        self.span.in_scope(|| {
            let _s = info_span!("terminating").entered();
            let pid = Pid::from_raw(self.child.id() as i32);
            debug!("Sending SIGTERM to '{}' (pid {})...", self.name, pid);
            match kill(pid, Signal::SIGTERM) {
                Ok(_) => {
                    match self.child.wait_timeout(Duration::from_secs(5)) {
                        Ok(Some(status)) => info!("Process '{}' terminated with status: {}", self.name, status),
                        Err(e) => warn!("Failed to wait for child process: {:?}", e),
                        Ok(None) => {
                            warn!("Child process did not exit in time. Sending SIGKILL to pid {}.", pid);
                            match kill(pid, Signal::SIGKILL) {
                                Ok(_) => match self.child.wait() {
                                    Ok(status) => info!("Process '{}' killed with status: {}", self.name, status),
                                    Err(e) => warn!("Failed to wait for shell after SIGKILL: {:?}", e),
                                },
                                Err(Error::Sys(errno)) => { warn!("Failed to send SIGKILL: {}", errno); },
                                Err(e) => warn!("An unexpected error occurred: {}", e),
                }}}},
                Err(Error::Sys(errno)) => {
                    match errno {
                        nix::errno::Errno::EPERM => warn!("No permission to send signal to pid {}.", pid),
                        nix::errno::Errno::ESRCH => warn!("Process does not exist"),
                        _ => warn!("An unexpected error occurred: {}", errno),
                }},
                Err(e) => warn!("An unexpected error occurred: {}", e),
            };

            debug!("Joining logging threads...");
            self.terminate_flag.store(true, Ordering::Relaxed);
            while let Some(thread) = self.threads.pop() {
                if let Err(e) = thread.join() {
                    warn!("Failed to join logging thread: {:?}", e);
                }
            }
        });
    }
}

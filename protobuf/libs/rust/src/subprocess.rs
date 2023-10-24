use anyhow::bail;
use tracing::{info, warn, error, info_span, debug};
use strip_ansi_escapes;

/// A subprocess handle that will kill the subprocess when dropped.
pub struct ProcHandle {
    span: tracing::Span,
    name: String,
    child: std::process::Child,
    _threads: Vec<std::thread::JoinHandle<()>>,
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

    info!("Spawing (shell cmd): {:?}", cmd);
    let mut child = cmd.spawn()?;

    fn log_stream(span: tracing::Span, stream: Box<dyn std::io::Read>, level: tracing::Level, name: &str)
    {
        debug!("Starting thread to read {}->log", name);
        let _entered = span.enter();
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stream);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    // If the line from organizer starts with a log level, use it.
                    // Otherwise, use the default log level (INFO for stdout, ERROR for stderr).
                    let line = strip_ansi_escapes::strip_str(line); // Remove terminal colors from log lines (if any)
                    match line.split_once(" ") {
                        Some((level_str, msg_str)) => {
                            let level_override = match level_str {
                                "DEBUG" => tracing::Level::DEBUG,
                                "INFO" => tracing::Level::INFO,
                                "WARN" | "WARNING" => tracing::Level::WARN,
                                "ERROR" | "CRITICAL" | "FATAL" => tracing::Level::ERROR,
                                _ => match level {
                                    tracing::Level::INFO => tracing::Level::INFO,
                                    tracing::Level::ERROR => tracing::Level::ERROR,
                                    _ => panic!("Unsupported log level"),
                                }
                            };
                            match level_override {
                                tracing::Level::DEBUG => debug!("[{}] {}", name, msg_str),
                                tracing::Level::INFO => info!("[{}] {}", name, msg_str),
                                tracing::Level::WARN => warn!("[{}] {}", name, msg_str),
                                tracing::Level::ERROR => error!("[{}] {}", name, msg_str),
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
        }
        debug!("Thread to read {}->log exiting", name);
    }

    // Spawn threads to log the subprocess's stdout and stderr
    let threads = vec![
        if let Some(stdout) = child.stdout.take() {
                let span = span.clone();
                std::thread::spawn(move || log_stream(span, Box::new(stdout), tracing::Level::INFO, "stdout"))
            } else { bail!("Failed to capture stdout"); },
        if let Some(stderr) = child.stderr.take() {
            let span = span.clone();
            std::thread::spawn(move || log_stream(span, Box::new(stderr), tracing::Level::ERROR, "stderr"))
            } else { bail!("Failed to capture stderr"); }
    ];

    debug!("Subprocess spawned");
    Ok(ProcHandle { span, name, child, _threads: threads })
}

/// Terminate the subprocess when Handle is dropped.
impl Drop for ProcHandle
{
    fn drop(&mut self) {
        self.span.in_scope(|| {
            let _s = info_span!("terminating").entered();
            debug!("Sending SIGTERM to '{}'...", self.name);
            use libc::{kill, pid_t, SIGTERM};
            let pid: pid_t = self.child.id() as pid_t;
            let result = unsafe { kill(pid, SIGTERM) };
            if result == -1 {
                warn!("Failed to send signal. Killing it.");
                if let Err(e) = self.child.kill() {
                    warn!("Failed to kill child process, giving up: {:?}", e);
                }
            } else {
                debug!("Waiting for child to exit...");
                match  self.child.wait() {
                    Ok(status) => { info!("Shell exited with status: {}", status); }
                    Err(e) => { warn!("Failed to wait for shell: {:?}", e); }
                }
            }
        });
    }
}

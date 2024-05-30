use std::{
    fs::OpenOptions,
    io::{self, stdout, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};
use signal_hook::{consts::SIGUSR1, iterator::Signals};
use tracing::subscriber::set_global_default;
use tracing_subscriber::{fmt, EnvFilter, fmt::time::OffsetTime};

/// Custom logger with the ability to write to a file or stdout.
/// It supports transparent file reopen on SIGUSR1 (for `logrotate`),
/// and can be configured for JSON or plain text logging.
pub struct ClapshotLogger {
    pub log_writer: Arc<Mutex<Option<ReopenableFileWriter>>>,
    pub guard: tracing_appender::non_blocking::WorkerGuard,
}

impl ClapshotLogger
{
    /// Create a new Logger instance.
    /// - `time_offset`: Time offset for the log timestamps.
    /// - `level`: Tracing level to log.
    /// - `log_file`: Path to the log file or "-" for stdout.
    /// - `json_log`: Enable or disable JSON formatted logging.
    pub fn new(time_offset: time::UtcOffset, level: tracing::Level, log_file: &str, json_log: bool) -> anyhow::Result<Self> {
        let log_writer = Arc::new(Mutex::new(None));
        let log_to_stdout = log_file.is_empty() || log_file == "-";

        let (log_writer_impl, guard) = if log_to_stdout {
            tracing_appender::non_blocking(stdout())
        } else {
            let file = ReopenableFileWriter::new(PathBuf::from(log_file));
            *log_writer.lock().unwrap() = Some(file.clone());

            // Listen for SIGUSR1 to reopen the log file
            let mut signals = Signals::new(&[SIGUSR1])?;
            let log_writer_cloned = log_writer.clone();
            thread::spawn(move || {
                for _ in signals.forever() {
                    let mut log_writer = log_writer_cloned.lock().expect("Failed to lock log writer");
                    if let Some(file) = log_writer.as_mut() {
                        file.sync_and_reopen().expect("Failed to reopen log file");
                    }
                }
            });
            tracing_appender::non_blocking(file)
        };

        if std::env::var_os("RUST_LOG").is_none() {
            std::env::set_var(
                "RUST_LOG", match level {
                    tracing::Level::ERROR => "error",
                    tracing::Level::WARN => "warn",
                    tracing::Level::INFO => "info,clapshot_server=info",
                    tracing::Level::DEBUG => "debug,clapshot_server=debug,h2=info,hyper::proto::h1=info",
                    tracing::Level::TRACE => "trace,clapshot_server=trace,h2=debug,hyper::proto::h1=debug,async_io=debug",
                },
            );
        }

        let minute_offset = time_offset.whole_minutes() % 60;
        let iso_fmt = match (level>=tracing::Level::DEBUG, minute_offset != 0) {
            (false, false) => "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]",
            (false, true) => "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]",
            (true, _) => "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:4][offset_hour sign:mandatory]:[offset_minute]",
        };

        let time_format = time::format_description::parse(
            if json_log {
                "[unix_timestamp].[subsecond digits:4]]"
            } else {
                iso_fmt
            },
        )
        .expect("invalid time format");

        let timer = OffsetTime::new(time_offset, time_format);

        let log_subscriber = fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_timer(timer)
            .with_file(false)
            .with_line_number(false)
            .with_thread_ids(false)
            .with_target(false)
            .with_writer(log_writer_impl)
            .with_ansi(log_to_stdout);

        if json_log {
            set_global_default(log_subscriber.json().finish())
        } else {
            set_global_default(log_subscriber.finish())
        }
        .expect("tracing::subscriber::set_global_default failed");

        Ok(ClapshotLogger { log_writer, guard })
    }
}


/// ReopenableFileWriter provides functionality to write to a file
/// that can be reopened, allowing for log rotation without losing log entries.
pub struct ReopenableFileWriter {
    file: Arc<Mutex<Option<std::fs::File>>>,
    path: PathBuf,
}

impl ReopenableFileWriter {
    pub fn new(path: PathBuf) -> Self {
        let file = Arc::new(Mutex::new(Some(Self::open_file(&path).unwrap())));
        Self { file, path }
    }

    fn open_file(path: &PathBuf) -> io::Result<std::fs::File> {
        OpenOptions::new().create(true).write(true).append(true).open(path)
    }

    /// Sync the current log file to disk and reopen it under a new file descriptor.
    pub fn sync_and_reopen(&self) -> io::Result<()> {
        tracing::info!("Reopening log file: {:?}", self.path);
        let new_file = Self::open_file(&self.path)?;
        let mut file_lock = self.file.lock().unwrap();
        file_lock.as_mut().unwrap().sync_all()?;
        *file_lock = Some(new_file);
        Ok(())
    }
}

impl Clone for ReopenableFileWriter {
    fn clone(&self) -> Self {
        Self { file: self.file.clone(), path: self.path.clone() }
    }
}

impl Write for ReopenableFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut file_lock = self.file.lock().unwrap();
        if let Some(file) = file_lock.as_mut() { file.write(buf) } else { Ok(0) }
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut file_lock = self.file.lock().unwrap();
        if let Some(file) = file_lock.as_mut() { file.flush() } else { Ok(()) }
    }
}


#[test]
fn test_log_rotation_on_sigusr1() {
    use std::{
        fs::File,
        io::Read,
        sync::Arc,
        thread,
        time::Duration,
    };
    use assert_fs::TempDir;

    let log_dir = TempDir::new().expect("Failed to create temp dir");
    let log_file = log_dir.path().join("test_log.log");
    let log_file_backup = log_dir.path().join("test_log_backup.log");

    let time_offset = time::UtcOffset::from_whole_seconds(0).unwrap();
    let logger = Arc::new(ClapshotLogger::new(time_offset, tracing::Level::DEBUG, log_file.to_str().unwrap(), false).expect("Failed to setup logger"));

    tracing::info!("Logging before rotation");

    // Rename the log file to simulate log rotation
    std::fs::rename(&log_file, &log_file_backup).expect("Failed to rename log file");
    signal_hook::low_level::raise(SIGUSR1).expect("Failed to send SIGUSR1");
    thread::sleep(Duration::from_secs(1));

    tracing::info!("Logging after rotation");
    logger
        .log_writer
        .lock()
        .expect("Failed to lock log writer")
        .as_mut()
        .unwrap()
        .flush()
        .expect("Failed to flush log writer");

    thread::sleep(Duration::from_secs(1));

    let mut old_log_content = String::new();
    let mut old_log_file = File::open(&log_file_backup).expect("Failed to open old log file");
    old_log_file.read_to_string(&mut old_log_content).expect("Failed to read old log file");
    assert!(old_log_content.contains("Logging before rotation"), "Old log file does not contain the expected log entry");
    assert!(!old_log_content.contains("Logging after rotation"), "Old log file contains the second log entry");

    let mut new_log_content = String::new();
    let mut new_log_file = File::open(&log_file).expect("Failed to open new log file");
    new_log_file.read_to_string(&mut new_log_content).expect("Failed to read new log file");
    assert!(new_log_content.contains("Logging after rotation"), "New log file does not contain the expected log entry");
}

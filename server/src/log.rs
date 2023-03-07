use tracing::subscriber::set_global_default;
use tracing_appender::{non_blocking};
use tracing_subscriber::{fmt, EnvFilter};

pub fn setup_logging(debug: bool, log_file: &str, json_log: bool)
     -> anyhow::Result<non_blocking::WorkerGuard>
{
    let log_to_stdout = log_file == "" || log_file == "-";
    let (log_writer, guard) = if log_to_stdout {
            non_blocking(std::io::stdout())
        } else {
            let f = std::fs::OpenOptions::new().create(true).append(true).open(log_file)?;
            non_blocking(f)
        };

    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", if debug {"debug,clapshot_server=debug"} else {"info,clapshot_server=info"});
    };

    let log_sbsc = fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_file(false)
        .with_line_number(false)
        .with_thread_ids(false)
        .with_target(false)
        .with_writer(log_writer)
        .with_ansi(log_to_stdout);

    if json_log {
        set_global_default(log_sbsc.json().finish())
    } else {
        set_global_default(log_sbsc.finish())
    }.expect("tracing::subscriber::set_global_default failed");

    Ok(guard)
}

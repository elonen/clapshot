use tracing::subscriber::set_global_default;
use tracing_appender::{non_blocking};
use tracing_subscriber::{fmt, EnvFilter};
use tracing_subscriber::fmt::time::OffsetTime;


pub fn setup_logging(time_offset: time::UtcOffset, debug: bool, log_file: &str, json_log: bool)
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
        std::env::set_var("RUST_LOG", if debug {"debug,clapshot_server=debug,h2=info,hyper::proto::h1=info"} else {"info,clapshot_server=info"});
    };

    let minute_offset = time_offset.whole_minutes() % 60;
    let iso_fmt = match (debug, minute_offset!=0) {
        (false, false) => "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]",
        (false, true) => "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]",
        (true, _) => "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:4][offset_hour sign:mandatory]:[offset_minute]",
    };

    let time_format = time::format_description::parse(
        if json_log { "[unix_timestamp].[subsecond digits:4]]" } else { iso_fmt }
    ).expect("invalid time format");

    let timer = OffsetTime::new(time_offset, time_format);

    let log_sbsc = fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_timer(timer)
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

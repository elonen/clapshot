
/// Configure `tracing` logger
/// 
/// # Arguments
/// 
/// * `json_log` - If true, log in JSON format.
/// * `debug` - If true, enable debug logging.
pub(crate) fn setup_logging(json_log: bool, debug: bool) -> anyhow::Result<()>
{
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", if debug {"debug,simple_organizer=debug"} else {"info,simple_organizer=info"});
    };
    // Clapshot server, when spawhing this as a subprocess, will
    // merge plugin's stdout/stderr within its logs, so we'll need to
    // to be really bare-bones here. No timestamps, no target etc.
    let log_sbsc = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_file(false)
        .with_line_number(false)
        .with_thread_ids(false)
        .with_target(false)
        .without_time();

    if json_log {
        tracing::subscriber::set_global_default(log_sbsc.json().finish())
    } else {
        tracing::subscriber::set_global_default(log_sbsc.finish())
    }.expect("tracing::subscriber::set_global_default failed");
    Ok(())
}

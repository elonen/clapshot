use super::*;
use std::os::unix::fs::PermissionsExt;

fn ev(i: u32) -> NotificationEvent {
    NotificationEvent { kind: NotificationKind::CommentAdded, payload: json!({ "i": i }) }
}

#[test]
fn kind_strings_are_unique_and_stable() {
    let names: Vec<&str> = NotificationKind::ALL.iter().map(|k| k.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "event names must be unique");
    assert!(names.contains(&"comment_added"));
    assert!(names.contains(&"media_file_updated"));
}

#[test]
fn glob_matching() {
    assert!(glob_match("*", "comment_added"));
    assert!(glob_match("comment_added", "comment_added"));
    assert!(!glob_match("comment_added", "comment_edited"));
    assert!(glob_match("comment_*", "comment_added"));
    assert!(glob_match("comment_*", "comment_deleted"));
    assert!(!glob_match("comment_*", "media_file_added"));
    assert!(glob_match("media_file_*", "media_file_updated"));
    assert!(glob_match("*_added", "comment_added"));
    assert!(glob_match("*_added", "media_file_added"));
    assert!(!glob_match("*_added", "comment_edited"));
    assert!(glob_match("*added*", "media_file_added"));
}

#[test]
fn pattern_parsing_and_validation() {
    assert_eq!(parse_event_patterns("*"), vec!["*"]);
    assert_eq!(
        parse_event_patterns(" comment_*, media_file_added ,"),
        vec!["comment_*", "media_file_added"]
    );
    assert!(unknown_patterns(&parse_event_patterns("*")).is_empty());
    assert!(unknown_patterns(&parse_event_patterns("comment_*,media_file_added")).is_empty());
    assert_eq!(unknown_patterns(&parse_event_patterns("bogus_event")), vec!["bogus_event"]);
}

#[test]
fn disabled_sink_never_wants() {
    let sink = NotificationSink::disabled();
    for k in NotificationKind::ALL {
        assert!(!sink.wants(k));
    }
    // emit must not invoke the builder when disabled
    let mut called = false;
    sink.emit(NotificationKind::CommentAdded, || {
        called = true;
        ev(0)
    });
    assert!(!called);
}

#[test]
fn allowlist_gates_emit_and_skips_builder() {
    let (sink, rx) = NotificationSink::new(parse_event_patterns("comment_*"));
    assert!(sink.wants(NotificationKind::CommentAdded));
    assert!(!sink.wants(NotificationKind::MediaFileAdded));

    let mut media_built = false;
    sink.emit(NotificationKind::MediaFileAdded, || {
        media_built = true;
        ev(0)
    });
    assert!(!media_built, "builder must not run for filtered-out events");
    assert!(rx.try_recv().is_err());

    sink.emit(NotificationKind::CommentAdded, || ev(42));
    let got = rx.try_recv().expect("comment event should be queued");
    assert_eq!(got.payload["i"], 42);
}

#[test]
fn drop_oldest_under_flood() {
    let (sink, rx) = NotificationSink::new(parse_event_patterns("*"));
    let total = QUEUE_CAPACITY as u32 + 10;
    for i in 0..total {
        sink.emit(NotificationKind::CommentAdded, || ev(i));
    }
    let mut drained = Vec::new();
    while let Ok(e) = rx.try_recv() {
        drained.push(e.payload["i"].as_u64().unwrap() as u32);
    }
    assert_eq!(drained.len(), QUEUE_CAPACITY, "queue is capped at capacity");
    // The oldest were dropped, so the newest event must have survived.
    assert!(drained.contains(&(total - 1)), "newest event must be kept");
    assert!(!drained.contains(&0), "oldest event must be dropped");
}

/// Write a throwaway executable script and return its path.
fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[test]
fn executor_passes_event_env_and_json_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let sink_file = dir.path().join("out.txt");
    let script = write_script(
        dir.path(),
        "record.sh",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$CLAPSHOT_NOTIFICATION_EVENT\" >> '{0}'\ncat >> '{0}'\n",
            sink_file.display()
        ),
    );

    let e = NotificationEvent {
        kind: NotificationKind::CommentDeleted,
        payload: json!({ "comment": { "text": "multi\nline" } }),
    };
    run_one(&script, &e, SCRIPT_TIMEOUT);

    let recorded = std::fs::read_to_string(&sink_file).unwrap();
    assert!(recorded.starts_with("comment_deleted\n"), "env var passed: {recorded:?}");
    assert!(recorded.contains("\"text\":\"multi\\nline\""), "json on stdin: {recorded:?}");
}

#[test]
fn executor_survives_nonzero_exit_and_missing_script() {
    let dir = tempfile::tempdir().unwrap();
    let failing = write_script(dir.path(), "fail.sh", "#!/bin/sh\necho boom >&2\nexit 3\n");
    // Must not panic on non-zero exit.
    run_one(&failing, &ev(1), SCRIPT_TIMEOUT);
    // Must not panic when the script does not exist.
    run_one(&dir.path().join("does-not-exist"), &ev(2), SCRIPT_TIMEOUT);
}

#[test]
fn executor_kills_on_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let slow = write_script(dir.path(), "slow.sh", "#!/bin/sh\nsleep 10\n");
    let start = Instant::now();
    run_one(&slow, &ev(1), Duration::from_millis(200));
    assert!(start.elapsed() < Duration::from_secs(3), "timed-out script should be killed promptly");
}

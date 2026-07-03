//! External notification hook.
//!
//! Invokes a user-configured script on domain events (comments, persisted user
//! messages, media files). The script receives the event type in the
//! `CLAPSHOT_NOTIFICATION_EVENT` environment variable and a single JSON payload
//! object on stdin. It is meant for email/Slack/etc. integrations.
//!
//! The whole feature is OFF unless a script is configured: in that case
//! [`NotificationSink::disabled`] is used and every call site is a cheap no-op.
//!
//! Execution is decoupled from the request path: events are pushed to a bounded
//! queue (drop-oldest under flood) and run serially by a dedicated worker thread,
//! so a slow or broken script can never block or fail commenting/uploads.
//!
//! See `doc/notification-hook.md` for the user-facing payload reference.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use serde_json::{json, Value};

use crate::database::models;

/// Max events buffered before the worker; under flood the oldest are dropped.
pub const QUEUE_CAPACITY: usize = 100;

/// A single script invocation is killed if it runs longer than this.
pub const SCRIPT_TIMEOUT: Duration = Duration::from_secs(30);

/// Payload schema version, bumped on incompatible changes to the JSON shape.
const SCHEMA_VERSION: u32 = 1;

/// Canonical notification event types.
///
/// The string forms ([`NotificationKind::as_str`]) are the stable public
/// identifiers used in the JSON payload `event` field, the
/// `CLAPSHOT_NOTIFICATION_EVENT` environment variable, and the
/// `--notification-events` allowlist.
///
/// Example: `comment_*, media_file_added`
///
/// THIS ENUM IS THE SOURCE OF TRUTH for event names; docs link here instead of
/// re-listing them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    CommentAdded,
    CommentEdited,
    CommentDeleted,
    MessagePersisted,
    MediaFileAdded,
    MediaFileUpdated,
}

impl NotificationKind {
    pub const ALL: [NotificationKind; 6] = [
        Self::CommentAdded,
        Self::CommentEdited,
        Self::CommentDeleted,
        Self::MessagePersisted,
        Self::MediaFileAdded,
        Self::MediaFileUpdated,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CommentAdded => "comment_added",
            Self::CommentEdited => "comment_edited",
            Self::CommentDeleted => "comment_deleted",
            Self::MessagePersisted => "message_persisted",
            Self::MediaFileAdded => "media_file_added",
            Self::MediaFileUpdated => "media_file_updated",
        }
    }
}

/// Where a persisted user message originated, so notification scripts can filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageOrigin {
    /// Server-internal message (via the `send_user_*` macros).
    Server,
    /// Pushed by an Organizer plugin via `client_show_user_message`.
    Organizer,
    /// Emitted by the media processing pipeline (transcode/ingest relay).
    Pipeline,
}

impl MessageOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Organizer => "organizer",
            Self::Pipeline => "pipeline",
        }
    }
}

/// A ready-to-emit notification: its kind plus the full JSON payload object.
#[derive(Debug, Clone)]
pub struct NotificationEvent {
    pub kind: NotificationKind,
    pub payload: Value,
}

/// Acting user that triggered an event (set for comment events; None otherwise).
pub struct ActorRef<'a> {
    pub user_id: &'a str,
    pub username: &'a str,
    pub is_admin: bool,
}

struct Inner {
    tx: Sender<NotificationEvent>,
    /// A clone of the receiver, used only to discard the oldest event when full.
    drop_rx: Receiver<NotificationEvent>,
    allow: Vec<String>,
}

/// Handle stored in `ServerState`; cheap to clone and always present.
///
/// When the feature is disabled (no script configured) all methods are no-ops.
#[derive(Clone)]
pub struct NotificationSink {
    inner: Option<Arc<Inner>>,
}

impl NotificationSink {
    /// A sink that does nothing (feature off).
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    /// Build a sink and the matching receiver for a worker thread.
    ///
    /// `allow` is the list of glob patterns from `--notification-events`.
    pub fn new(allow: Vec<String>) -> (Self, Receiver<NotificationEvent>) {
        let (tx, rx) = bounded(QUEUE_CAPACITY);
        let sink = Self {
            inner: Some(Arc::new(Inner { tx, drop_rx: rx.clone(), allow })),
        };
        (sink, rx)
    }

    /// True if this kind is enabled and matches the allowlist. Call sites use
    /// this implicitly via [`emit`](Self::emit); exposed mainly for tests.
    pub fn wants(&self, kind: NotificationKind) -> bool {
        match &self.inner {
            None => false,
            Some(i) => i.allow.iter().any(|p| glob_match(p, kind.as_str())),
        }
    }

    /// Emit an event. The `build` closure is invoked only when the event is
    /// enabled and allowlisted, so call sites pay no cost (e.g. extra DB
    /// lookups) for filtered-out events.
    pub fn emit(&self, kind: NotificationKind, build: impl FnOnce() -> NotificationEvent) {
        if !self.wants(kind) {
            return;
        }
        if let Some(i) = &self.inner {
            send_drop_oldest(&i.tx, &i.drop_rx, build());
        }
    }
}

/// Non-blocking enqueue: on a full queue, discard the oldest event(s) and warn.
fn send_drop_oldest(
    tx: &Sender<NotificationEvent>,
    drop_rx: &Receiver<NotificationEvent>,
    ev: NotificationEvent,
) {
    let mut ev = ev;
    let mut dropped = 0u32;
    loop {
        match tx.try_send(ev) {
            Ok(()) => {
                if dropped > 0 {
                    tracing::warn!(dropped, "Notification queue full; dropped oldest event(s).");
                }
                return;
            }
            Err(TrySendError::Full(returned)) => {
                let _ = drop_rx.try_recv(); // drop oldest
                dropped += 1;
                ev = returned;
            }
            Err(TrySendError::Disconnected(_)) => {
                tracing::warn!("Notification worker gone; dropping event.");
                return;
            }
        }
    }
}

/// Consume events and run the script serially. Returns when the channel closes.
pub fn run_forever(rx: Receiver<NotificationEvent>, script: PathBuf) {
    let _span = tracing::info_span!("NOTIFICATION").entered();
    tracing::debug!(script = %script.display(), "Starting notification hook worker.");
    while let Ok(ev) = rx.recv() {
        run_one(&script, &ev, SCRIPT_TIMEOUT);
    }
    tracing::debug!("Notification hook worker exiting.");
}

/// Invoke the script once for a single event. Never panics; logs and returns.
fn run_one(script: &Path, ev: &NotificationEvent, timeout: Duration) {
    let payload = match serde_json::to_vec(&ev.payload) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(err = %e, "Failed to serialize notification payload.");
            return;
        }
    };

    let mut child = match Command::new(script)
        .env("CLAPSHOT_NOTIFICATION_EVENT", ev.kind.as_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Own process group (pgid == child pid) so a timeout can kill the whole
        // tree, not just the shell. Otherwise a script whose grandchild outlives
        // it (e.g. dash forks `sleep` instead of exec'ing it) keeps the stdout/
        // stderr pipes open, and draining them below blocks until that orphan
        // exits — defeating the timeout.
        .process_group(0)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(script = %script.display(), err = %e, "Failed to spawn notification script.");
            return;
        }
    };

    // Feed stdin from a separate thread so a large payload can't deadlock against
    // a script that is slow to read (or against our own stdout/stderr draining).
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = stdin.write_all(&payload); // dropping stdin closes it -> EOF
        });
    }
    // Drain stdout/stderr concurrently to avoid pipe-buffer deadlocks.
    let out = child.stdout.take().map(drain_to_string);
    let err = child.stderr.take().map(drain_to_string);

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    // Kill the whole process group (negative pid), not just the
                    // direct child, so any forked grandchildren die too and release
                    // the stdout/stderr pipes we drain below.
                    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL); }
                    let _ = child.wait();
                    tracing::warn!(
                        event = ev.kind.as_str(),
                        secs = timeout.as_secs(),
                        "Notification script timed out; killed. Slow transports should hand off to their own queue."
                    );
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                tracing::error!(err = %e, "Error waiting for notification script.");
                break None;
            }
        }
    };

    let stderr = err.and_then(|h| h.join().ok()).unwrap_or_default();
    let stdout = out.and_then(|h| h.join().ok()).unwrap_or_default();

    match status {
        Some(s) if s.success() => {
            tracing::info!(event = ev.kind.as_str(), "Notification script ok.");
            // The script's own output is only interesting when debugging; surface
            // it at DEBUG so a script that "succeeded but did nothing" is visible.
            if !stdout.trim().is_empty() || !stderr.trim().is_empty() {
                tracing::debug!(
                    event = ev.kind.as_str(),
                    stdout = %abbrev(&stdout),
                    stderr = %abbrev(&stderr),
                    "Notification script output."
                );
            }
        }
        Some(s) => {
            tracing::warn!(
                event = ev.kind.as_str(),
                code = ?s.code(),
                stdout = %abbrev(&stdout),
                stderr = %abbrev(&stderr),
                "Notification script exited with error."
            );
        }
        None => {}
    }
}

fn drain_to_string<R: std::io::Read + Send + 'static>(mut r: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut r, &mut s);
        s
    })
}

fn abbrev(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 300 {
        let cut: String = s.chars().take(300).collect();
        format!("{cut} (...)")
    } else {
        s.to_string()
    }
}

// ----------------------------------------------------------------------------
// Allowlist glob matching
// ----------------------------------------------------------------------------

/// Match a shell-style glob (supporting `*` wildcards) against an event name.
/// Event names and patterns are ASCII, so byte indexing is safe.
fn glob_match(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == name;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0usize;
    // First segment must be a prefix.
    if !name.starts_with(parts[0]) {
        return false;
    }
    pos += parts[0].len();
    for (i, part) in parts.iter().enumerate().skip(1) {
        if part.is_empty() {
            continue; // trailing '*' or consecutive '*'
        }
        if i == parts.len() - 1 {
            // Last non-empty segment must be a suffix of the remainder.
            return name[pos..].ends_with(part);
        }
        match name[pos..].find(part) {
            Some(idx) => pos += idx + part.len(),
            None => return false,
        }
    }
    true // pattern ended with '*'
}

/// Parse a comma-separated `--notification-events` value into glob patterns.
pub fn parse_event_patterns(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Return any patterns that match no known event kind (likely typos).
pub fn unknown_patterns(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .filter(|p| !NotificationKind::ALL.iter().any(|k| glob_match(p, k.as_str())))
        .cloned()
        .collect()
}

/// Comma-separated list of all canonical event names (for help/log output).
pub fn all_event_names() -> String {
    NotificationKind::ALL
        .iter()
        .map(|k| k.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

// ----------------------------------------------------------------------------
// Payload builders (single envelope helper + thin per-event functions)
// ----------------------------------------------------------------------------

fn rfc3339(dt: &chrono::NaiveDateTime) -> String {
    use chrono::TimeZone;
    chrono::Utc.from_utc_datetime(dt).to_rfc3339()
}

fn rfc3339_opt(dt: &Option<chrono::NaiveDateTime>) -> Value {
    dt.as_ref().map(|d| json!(rfc3339(d))).unwrap_or(Value::Null)
}

/// Common envelope shared by every event payload.
fn envelope(kind: NotificationKind, url_base: &str, actor: Option<ActorRef>) -> serde_json::Map<String, Value> {
    let actor_json = match actor {
        Some(a) => json!({ "user_id": a.user_id, "username": a.username, "is_admin": a.is_admin }),
        None => Value::Null,
    };
    let mut m = serde_json::Map::new();
    m.insert("event".into(), json!(kind.as_str()));
    m.insert("schema_version".into(), json!(SCHEMA_VERSION));
    m.insert("timestamp".into(), json!(chrono::Utc::now().to_rfc3339()));
    m.insert("server".into(), json!({ "url_base": url_base }));
    m.insert("actor".into(), actor_json);
    m
}

/// Build a `comment_*` event payload.
#[allow(clippy::too_many_arguments)]
pub fn build_comment_notification_event(
    kind: NotificationKind,
    comment: &models::Comment,
    media_owner_user_id: Option<&str>,
    media_title: Option<&str>,
    drawing_abs_path: Option<String>,
    actor: Option<ActorRef>,
    previous_text: Option<&str>,
    url_base: &str,
) -> NotificationEvent {
    let mut m = envelope(kind, url_base, actor);
    m.insert(
        "comment".into(),
        json!({
            "id": comment.id.to_string(),
            "media_file_id": comment.media_file_id,
            "parent_id": comment.parent_id.map(|i| i.to_string()),
            "author": {
                "user_id": comment.user_id,
                "username": comment.username_ifnull,
            },
            "text": comment.comment,
            "timecode": comment.timecode,
            "drawing": drawing_abs_path,
            "created": rfc3339(&comment.created),
            "edited": rfc3339_opt(&comment.edited),
            "previous_text": previous_text,
        }),
    );
    m.insert(
        "media_file".into(),
        json!({
            "id": comment.media_file_id,
            "owner_user_id": media_owner_user_id,
            "title": media_title,
        }),
    );
    m.insert(
        "url".into(),
        json!(format!("{}/?vid={}#comment_{}", url_base, comment.media_file_id, comment.id)),
    );
    NotificationEvent { kind, payload: Value::Object(m) }
}

/// Build a `message_persisted` event payload.
pub fn build_message_notification_event(msg: &models::Message, origin: MessageOrigin, url_base: &str) -> NotificationEvent {
    let kind = NotificationKind::MessagePersisted;
    let mut m = envelope(kind, url_base, None);
    m.insert("origin".into(), json!(origin.as_str()));
    m.insert(
        "message".into(),
        json!({
            "id": msg.id.to_string(),
            "recipient_user_id": msg.user_id,
            "event_name": msg.event_name,
            "text": msg.message,
            "details": msg.details,
            "seen": msg.seen,
            "created": rfc3339(&msg.created),
            "refs": {
                "media_file_id": msg.media_file_id,
                "comment_id": msg.comment_id.map(|i| i.to_string()),
                "subtitle_id": msg.subtitle_id.map(|i| i.to_string()),
            },
        }),
    );
    NotificationEvent { kind, payload: Value::Object(m) }
}

/// Build a `media_file_added` / `media_file_updated` event payload.
pub fn build_media_file_notification_event(
    kind: NotificationKind,
    mf: &models::MediaFile,
    change_message: Option<&str>,
    url_base: &str,
) -> NotificationEvent {
    let mut m = envelope(kind, url_base, None);
    m.insert(
        "media_file".into(),
        json!({
            "id": mf.id,
            "owner_user_id": mf.user_id,
            "title": mf.title,
            "orig_filename": mf.orig_filename,
            "media_type": mf.media_type,
            "duration": mf.duration,
            "has_thumbnail": mf.has_thumbnail,
            "added_time": rfc3339(&mf.added_time),
            "recompression_done": rfc3339_opt(&mf.recompression_done),
            "thumbs_done": rfc3339_opt(&mf.thumbs_done),
        }),
    );
    if let Some(text) = change_message {
        m.insert("message".into(), json!(text));
    }
    m.insert("url".into(), json!(format!("{}/?vid={}", url_base, mf.id)));
    NotificationEvent { kind, payload: Value::Object(m) }
}

#[cfg(test)]
mod tests;

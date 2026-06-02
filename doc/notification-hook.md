# Notification Hook

Clapshot can call an external **notification script** when things happen on the
server — comments are added/edited/deleted, a user message is stored, or a media
file is ingested or updated. The script is meant for integrations such as e‑mail
or Slack notifications.

The feature is **off by default**: if you don't configure a script, nothing is
launched and there is no overhead. It is also organizer-agnostic — it works with
or without an Organizer plugin.

> A ready-to-adapt example with e‑mail / Slack / file drafts and event filtering
> ships at `/usr/share/clapshot-server/scripts/clapshot-notification.example`
> (in the source tree: [`server/scripts/clapshot-notification.example`](../server/scripts/clapshot-notification.example)).

## Enabling

In `clapshot-server.conf`:

```ini
notification-script = /usr/local/bin/my-clapshot-notification
# Optional glob allowlist; default '*' = all events:
notification-events = comment_*,media_file_added
```

or on the command line:

```sh
clapshot-server --notification-script=/usr/local/bin/my-clapshot-notification \
                --notification-events='comment_*,media_file_added' ...
```

The script must exist and be executable; the server checks this at startup.

## How the script is called

* The **event type** is passed in the `CLAPSHOT_NOTIFICATION_EVENT` environment
  variable (e.g. `comment_added`).
* A single **JSON object** describing the event is written to the script's
  **stdin**.
* The script is run directly (no shell), inheriting the server's environment plus
  `CLAPSHOT_NOTIFICATION_EVENT`.
* The **exit code is logged** but otherwise ignored — a failing or missing script
  never affects commenting, uploads, or anything else.

### Execution model and limits

Events are placed on a small in-memory queue and run **one at a time** by a
dedicated worker thread, so the script never blocks request handling.

* **Fire-and-forget / at-most-once.** Events are not persisted; any still queued
  when the server stops are lost. This is not a delivery-guaranteed audit log.
* **Bounded queue (100).** Under a flood, the **oldest** queued events are dropped
  (with a warning in the log).
* **30‑second timeout.** A script that runs longer is killed. If your transport
  can be slow (SMTP, webhooks), **do not block** in the script — write the job to
  your own spool/queue and return immediately, delivering from a separate worker.
* **Untrusted content.** Fields such as comment text are user-supplied. Because
  data is passed as a JSON document on stdin (not as shell arguments), there is no
  shell-injection surface, but treat the values as untrusted when you render them
  (e.g. into HTML e‑mail).

## Filtering

There are two layers:

1. **Server-side allowlist** (`--notification-events`): a comma-separated list of
   shell-style globs. Events that don't match are never queued and the script is
   never spawned for them. Examples: `*` (all, the default), `comment_*`,
   `media_file_*`, `comment_added,media_file_added`.
2. **In-script filtering**: inspect `CLAPSHOT_NOTIFICATION_EVENT` and the JSON
   payload (e.g. only act on `message.event_name == "error"`, or only for a
   particular media-file owner).

## Event types

The canonical, authoritative list of event names is the `NotificationKind` enum in
the server source:
[`server/src/notification/mod.rs`](../server/src/notification/mod.rs).

| Event | When |
|-------|------|
| `comment_added` | a comment was created |
| `comment_edited` | a comment's text was changed |
| `comment_deleted` | a comment was deleted |
| `message_persisted` | a user/inbox message was stored in the database |
| `media_file_added` | an uploaded media file finished ingesting |
| `media_file_updated` | a media file changed (e.g. after transcoding or thumbnailing) |

## Payload reference

`schema_version` is currently `1`. All timestamps are RFC 3339 UTC strings.

### Common envelope (every event)

```jsonc
{
  "event": "comment_added",
  "schema_version": 1,
  "timestamp": "2026-06-03T08:15:00+00:00",   // when the event fired
  "server": { "url_base": "https://clapshot.example.com" },
  "actor": {                                   // who triggered it; null if none
    "user_id": "alice",
    "username": "Alice",
    "is_admin": false
  }
}
```

`actor` is set for the **comment** events (it may differ from the comment's author
— e.g. an admin editing or deleting someone else's comment). It is `null` for
`message_persisted`, `media_file_added`, and `media_file_updated`, which are not
triggered by a specific interactive user.

### `comment_added` / `comment_edited` / `comment_deleted`

```jsonc
{
  // ...envelope (actor set)...
  "comment": {
    "id": "123",
    "media_file_id": "HASH0",
    "parent_id": null,                 // string id of the parent comment, or null
    "author": { "user_id": "alice", "username": "Alice" },  // user_id may be null
    "text": "Looks great at 0:42",
    "timecode": "0:42",                // or null
    "drawing": "/var/lib/clapshot/data/videos/HASH0/drawings/ab12...webp",
                                       // absolute path to the drawing image, or null.
                                       // The file is guaranteed written before this fires.
    "created": "2026-06-03T08:15:00+00:00",
    "edited":  null,                   // RFC3339 on edited comments, else null
    "previous_text": null              // the prior text on comment_edited, else null
  },
  "media_file": {
    "id": "HASH0",
    "owner_user_id": "bob",            // who owns the media file (may be null)
    "title": "clip.mp4"                // may be null
  },
  "url": "https://clapshot.example.com/?vid=HASH0#comment_123"
}
```

On `comment_deleted`, `comment` reflects the row **as it was just before deletion**.

### `message_persisted`

Fires whenever a user/inbox message row is stored, from any source. Use `origin`
and `message.event_name` to filter.

```jsonc
{
  // ...envelope (actor null)...
  "origin": "server",                  // "server" | "organizer" | "pipeline"
  "message": {
    "id": "456",
    "recipient_user_id": "bob",
    "event_name": "error",             // e.g. "ok" or "error"
    "text": "Transcoding failed",
    "details": "ffmpeg exited with code 1",
    "seen": false,
    "created": "2026-06-03T08:15:00+00:00",
    "refs": {
      "media_file_id": "HASH0",        // any of these may be null
      "comment_id": null,
      "subtitle_id": null
    }
  }
}
```

### `media_file_added` / `media_file_updated`

```jsonc
{
  // ...envelope (actor null)...
  "media_file": {
    "id": "HASH0",
    "owner_user_id": "alice",
    "title": "clip.mp4",               // may be null
    "orig_filename": "clip.mp4",       // may be null
    "media_type": "video",             // "video" | "audio" | "image" | null
    "duration": 123.4,                 // seconds, or null
    "has_thumbnail": true,             // or null
    "added_time": "2026-06-03T08:10:00+00:00",
    "recompression_done": null,        // RFC3339 once transcoded, else null
    "thumbs_done": null                // RFC3339 once thumbnailed, else null
  },
  "message": "Transcoding done",       // only on media_file_updated; describes the change
  "url": "https://clapshot.example.com/?vid=HASH0"
}
```

`media_file_updated` can fire **more than once** per file (for example after
transcoding and again after thumbnailing). Use the `message` field
(e.g. `"Transcoding done"` vs `"Media thumbnail generated"`) and/or the
`recompression_done`/`thumbs_done` timestamps to tell the changes apart, or
deduplicate by `media_file.id` if you only want one notification.

## See also

* Example script: [`server/scripts/clapshot-notification.example`](../server/scripts/clapshot-notification.example)
* Event-name source of truth: [`server/src/notification/mod.rs`](../server/src/notification/mod.rs)
* [Sysadmin Guide](sysadmin-guide.md)

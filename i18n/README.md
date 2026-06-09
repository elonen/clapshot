# Clapshot translations (i18n)

Central, gettext-based catalog shared by all three codebases (Rust **server**, Python **organizer**,
Svelte **client**). One message model everywhere:

- **`msgid` = the English source string**. A missing translation falls back to the source, so English needs no catalog.
- **`msgctxt`** disambiguates identical sources with different meanings.
- Translatable locales live in `po/<locale>.po`. `en` is the source language and has no PO.

## Marking strings (what the extractors look for)

| Platform | mark a string | with placeholders | with gettext context |
|---|---|---|---|
| Client (`.svelte`/`.ts`) | `$t("Source")` | `$t("Hi {name}", { name })` | `$t("Source", { context: "...", comment: "..." })` |
| Organizer (Python) | `_("Source")` | `_("Hi {name}").format(name=...)` | `pgettext("ctx", "Source")` |
| Server (Rust) | `server.tr_user(uid, "Source")` | `server.tr_user_fmt(uid, "Hi {name}", &[("name", &v)])` | — (server strings aren't contexted) |

Placeholders are **named** (`{name}`) and interpolated at the call site. Never concatenate translatable fragments.

**Translator hints** (extracted as a gettext `#.` comment): on the client pass a `comment` key in the
options — `$t("Title", { context: "subtitle", comment: "the subtitle's name field (a noun)" })`; in
Python put a `# TRANSLATORS: …` comment on the line directly above the call (`xgettext --add-comments`).
(`xtr`/server has no comment extraction.)

## Workflow

```
make extract   # AST-harvest msgids from all 3 codebases -> templates/*.pot -> templates/messages.pot
make update    # msgmerge messages.pot into each po/<locale>.po (msginit if new)
make compile   # build runtime catalogs into each component
make stats     # per-locale coverage
make all       # extract + update + compile
```

Extraction is AST-based (reliable, not regex): client via `client/scripts/i18n-extract.mjs`
(Svelte compiler + TypeScript compiler), Python via `xgettext --language=Python`, Rust via `xtr`.

## Layout

```
i18n/
  po/<locale>.po
  templates/*.pot       # generated (gitignored)
  Makefile  README.md
```

The user's UI locale is chosen client-side and sent to the backends (client→server `SetLanguage`,
forwarded to Organizer as `UserSessionData.language`), so each backend localizes its own output
to the recipient's language.

`make compile` builds the runtime catalogs:
- **client** → `client/src/i18n/locales/<loc>.json` (committed; bundled by Vite).
- **organizer** → `organizer/.../locale/<loc>/LC_MESSAGES/clapshot.mo` (gitignored; loaded at runtime via `gettext`).
- **server** needs no `compile` step — `server/build.rs` compiles `po/*.po` straight into the binary.

## Activating a new locale in the client

The server and organizer pick up `po/*.po` automatically (at build/runtime). The client is the exception:
Vite bundles catalogs via static imports, so after `make compile` you must wire the new locale into
`client/src/i18n/index.ts` by hand — three edits:

1. **Import** the compiled catalog: `import xx from './locales/xx.json';`
2. **Register** it in the `CATALOGS` map: `const CATALOGS = { fi, zh, xx };`
3. **List** it in `availableLocales` so it appears in the language selector: `{ id: 'xx', label: '...' }`

(`en` stays in `availableLocales` but has no import/catalog — it's the source language.)

Known gap: `server/src/video_pipeline` emits transcode/ingest status messages from worker threads
with no session context, so those remain English for now.

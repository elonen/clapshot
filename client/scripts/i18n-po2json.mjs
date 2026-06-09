#!/usr/bin/env node
/*
 * Compile a gettext PO file to the JSON map the Clapshot client runtime loads.
 *
 * Output: { "<msgctxt><msgid>" | "<msgid>": "<translation>", ... }
 * Entries with an empty translation are omitted, so the runtime falls back to the msgid (English source).
 *
 * Usage:  node client/scripts/i18n-po2json.mjs i18n/po/fi.po > client/src/i18n/locales/fi.json
 */
import fs from 'node:fs';

const CTX_SEP = '\u0004';   // gettext's msgctxt/msgid separator convention

function unquote(line) {
  // Join the quoted-string payload of a PO line (handles "..." with escapes).
  const m = line.match(/"((?:[^"\\]|\\.)*)"/);
  if (!m) return '';
  return m[1].replace(/\\n/g, '\n').replace(/\\t/g, '\t').replace(/\\"/g, '"').replace(/\\\\/g, '\\');
}

const src = fs.readFileSync(process.argv[2], 'utf8');
const lines = src.split('\n');
const out = {};
let cur = null;   // { ctx, id, str } accumulator
let field = null; // 'ctx' | 'id' | 'str'
let fuzzy = false; // current entry flagged `#, fuzzy` (unverified guess from msgmerge)

function flush() {
  if (cur && cur.id !== '' && cur.str && !fuzzy) {    // skip header (empty id), untranslated, and fuzzy
    out[(cur.ctx ? cur.ctx + CTX_SEP : '') + cur.id] = cur.str;
  }
  cur = null; field = null; fuzzy = false;
}

for (const raw of lines) {
  const line = raw.trim();
  if (line === '' || line.startsWith('#')) {
    if (line.startsWith('#,') && line.includes('fuzzy')) fuzzy = true;
    if (line === '') flush();
    continue;
  }
  if (line.startsWith('msgctxt')) { cur = cur || { ctx: '', id: '', str: '' }; cur.ctx = unquote(line); field = 'ctx'; }
  else if (line.startsWith('msgid_plural')) { field = null; }      // plurals: not handled in client runtime (YAGNI)
  else if (line.startsWith('msgid')) { cur = cur || { ctx: '', id: '', str: '' }; cur.id = unquote(line); field = 'id'; }
  else if (line.startsWith('msgstr')) { cur = cur || { ctx: '', id: '', str: '' }; cur.str = unquote(line); field = 'str'; }
  else if (line.startsWith('"') && cur && field) { cur[field] += unquote(line); }  // continuation line
}
flush();

process.stdout.write(JSON.stringify(out, null, 0) + '\n');
process.stderr.write(`# ${Object.keys(out).length} translated message(s)\n`);

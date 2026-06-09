#!/usr/bin/env node
/*
 * Clapshot client gettext extractor.
 *
 * Scans .svelte and .ts/.js sources for translation markers and emits a gettext PO/POT to stdout.
 * Reliable (AST-based, never regex): .svelte via Svelte's own compiler, .ts/.js via the TypeScript
 * compiler. Lives under client/ so `svelte/compiler` + `typescript` resolve from client/node_modules.
 *
 * Markers (msgid = the English SOURCE string; context disambiguates identical sources):
 *   t("source"[, params])         -> msgid="source"
 *   tc("ctx", "source"[, params]) -> msgctxt="ctx", msgid="source"
 * (Also recognizes the reactive store forms $t / $tc used in Svelte markup.)
 *
 * Usage:  node client/scripts/i18n-extract.mjs <root-dir> [...more] > out.pot
 */
import { parse } from 'svelte/compiler';
import ts from 'typescript';
import fs from 'node:fs';
import path from 'node:path';

const MARKERS = { t: true, $t: true };  // translation markers; msgid is arg0, optional { context } is arg1

// (msgctxt  msgid) -> { msgid, msgctxt, refs:Set, dynamic:bool }
const entries = new Map();
function record(msgctxt, msgid, ref, dynamic = false, comment = null) {
  const key = (msgctxt ?? '') + '\u0004' + msgid;
  let e = entries.get(key);
  if (!e) { e = { msgid, msgctxt, refs: new Set(), dynamic, comment: null }; entries.set(key, e); }
  e.refs.add(ref);
  if (dynamic) e.dynamic = true;
  if (comment && !e.comment) e.comment = comment;
}

// literalOf: pull a static string out of an AST string/template node, or null if dynamic.
function literalOf(node, kind /* 'estree' | 'ts' */) {
  if (!node) return null;
  if (kind === 'estree') {
    if (node.type === 'Literal' && typeof node.value === 'string') return node.value;
    if (node.type === 'TemplateLiteral' && node.expressions.length === 0) return node.quasis[0].cooked;
  } else {
    if (ts.isStringLiteralLike(node)) return node.text;
  }
  return null;
}

// Read literal `context:` and `comment:` properties from a t() options-object argument.
// `comment` becomes a gettext `#.` translator hint; `context` becomes the msgctxt.
function optsOf(node, kind) {
  const out = { context: null, comment: null };
  if (!node) return out;
  const props = kind === 'estree'
    ? (node.type === 'ObjectExpression' ? node.properties : [])
    : (ts.isObjectLiteralExpression(node) ? node.properties : []);
  for (const p of props) {
    let key, valNode;
    if (kind === 'estree') {
      if (p.type !== 'Property' || p.computed) continue;
      key = p.key.name ?? p.key.value; valNode = p.value;
    } else {
      if (!ts.isPropertyAssignment(p) || !p.name) continue;
      key = p.name.text; valNode = p.initializer;
    }
    if (key === 'context') out.context = literalOf(valNode, kind);
    else if (key === 'comment') out.comment = literalOf(valNode, kind);
  }
  return out;
}

function handleCall(name, argNode0, argNode1, kind, ref, warn) {
  if (!(name in MARKERS)) return;
  const msgid = literalOf(argNode0, kind);
  const { context, comment } = optsOf(argNode1, kind);
  if (msgid === null) { record(context, `<DYNAMIC at ${ref}>`, ref, true); warn(ref, name); return; }
  record(context, msgid, ref, false, comment);
}

function lineOf(src, pos) { return src.slice(0, pos).split('\n').length; }
const warnings = [];
const warn = (ref, name) => warnings.push(`  ${ref}: ${name}() with a non-literal argument (skipped)`);

function extractSvelte(file, src) {
  const ast = parse(src, { modern: true, filename: file });
  const seen = new WeakSet();
  (function walk(n) {
    if (!n || typeof n !== 'object') return;
    if (seen.has(n)) return; seen.add(n);
    if (Array.isArray(n)) { for (const x of n) walk(x); return; }
    if (n.type === 'CallExpression' && n.callee?.type === 'Identifier' && n.callee.name in MARKERS) {
      handleCall(n.callee.name, n.arguments?.[0], n.arguments?.[1], 'estree', `${rel(file)}:${lineOf(src, n.start)}`, warn);
    }
    for (const k in n) if (k !== 'start' && k !== 'end' && k !== 'loc') walk(n[k]);
  })(ast);
}

function extractTs(file, src) {
  const sf = ts.createSourceFile(file, src, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  (function visit(node) {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text in MARKERS) {
      const line = sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
      handleCall(node.expression.text, node.arguments[0], node.arguments[1], 'ts', `${rel(file)}:${line}`, warn);
    }
    ts.forEachChild(node, visit);
  })(sf);
}

let ROOT;
const rel = (f) => path.relative(ROOT, f);

function walkFiles(dir, cb) {
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      if (['node_modules', 'dist', '.local-tmp', '__tests__', '.svelte-kit'].includes(ent.name)) continue;
      walkFiles(full, cb);
    } else if (/\.(svelte|ts|js)$/.test(ent.name) && !/\.(test|spec)\./.test(ent.name)) {
      cb(full);
    }
  }
}

function poEscape(s) { return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\t/g, '\\t'); }

// --- main ---
const roots = process.argv.slice(2);
if (!roots.length) { console.error('usage: i18n-extract.mjs <dir> [...]'); process.exit(2); }
ROOT = process.cwd();
for (const root of roots) {
  walkFiles(root, (file) => {
    const src = fs.readFileSync(file, 'utf8');
    try { file.endsWith('.svelte') ? extractSvelte(file, src) : extractTs(file, src); }
    catch (e) { console.error(`# WARN: failed to parse ${rel(file)}: ${e.message}`); }
  });
}

// POT header
let out = 'msgid ""\nmsgstr ""\n"Content-Type: text/plain; charset=UTF-8\\n"\n"Language: \\n"\n\n';
const sorted = [...entries.values()].filter(e => !e.dynamic).sort((a, b) =>
  (a.msgctxt || '').localeCompare(b.msgctxt || '') || a.msgid.localeCompare(b.msgid));
for (const e of sorted) {
  if (e.comment) out += `#. ${e.comment}\n`;       // translator hint (#. extracted comment)
  for (const r of [...e.refs].sort()) out += `#: ${r}\n`;
  if (e.msgctxt) out += `msgctxt "${poEscape(e.msgctxt)}"\n`;
  out += `msgid "${poEscape(e.msgid)}"\nmsgstr ""\n\n`;
}
process.stdout.write(out);
console.error(`# extracted ${sorted.length} unique message(s) from client`);
if (warnings.length) console.error(`# ${warnings.length} dynamic call(s) skipped:\n${warnings.join('\n')}`);

// Compile the central gettext PO catalogs (i18n/po/<locale>.po) into a static Rust lookup, so the
// server can localize its own user-facing messages with no runtime files or deployment paths.
// Untranslated/fuzzy/empty entries are omitted, so the caller falls back to the English source.
use std::{env, fs, path::Path};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let po_dir = Path::new(&manifest).join("../i18n/po");
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("i18n_catalog.rs");

    let mut arms = String::new();
    if let Ok(entries) = fs::read_dir(&po_dir) {
        for ent in entries.flatten() {
            let path = ent.path();
            if path.extension().and_then(|s| s.to_str()) != Some("po") { continue; }
            let locale = path.file_stem().unwrap().to_str().unwrap().to_string();
            println!("cargo:rerun-if-changed={}", path.display());
            for (msgid, msgstr) in parse_po(&fs::read_to_string(&path).unwrap_or_default()) {
                if msgid.is_empty() || msgstr.is_empty() { continue; }
                // {:?} emits a correctly-escaped Rust string literal.
                arms.push_str(&format!("        ({:?}, {:?}) => Some({:?}),\n", locale, msgid, msgstr));
            }
        }
    }
    println!("cargo:rerun-if-changed={}", po_dir.display());
    println!("cargo:rerun-if-changed=build.rs");

    let code = format!(
        "/// (locale, msgid) -> translation. Generated from i18n/po/*.po by build.rs.\n\
         pub fn translate(locale: &str, msgid: &str) -> Option<&'static str> {{\n\
         \x20   match (locale, msgid) {{\n{arms}        _ => None,\n    }}\n}}\n");
    fs::write(&out, code).unwrap();
}

/// Minimal PO parser: yields (msgid, msgstr) for non-fuzzy entries. Handles continuation lines
/// and the standard escapes. Skips the header (empty msgid) via the caller's empty-msgid guard.
fn parse_po(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (mut id, mut s, mut field, mut fuzzy, mut has_ctx) = (String::new(), String::new(), ' ', false, false);
    let flush = |out: &mut Vec<(String, String)>, id: &mut String, s: &mut String, fuzzy: &mut bool, has_ctx: &mut bool| {
        // Skip fuzzy and msgctxt entries: the server never looks up contexted strings (tr_user has no
        // context arg), and contexted msgids would otherwise collide into duplicate match arms.
        if !*fuzzy && !*has_ctx { out.push((id.clone(), s.clone())); }
        id.clear(); s.clear(); *fuzzy = false; *has_ctx = false;
    };
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut out, &mut id, &mut s, &mut fuzzy, &mut has_ctx);
            field = ' ';
        } else if line.starts_with('#') {
            if line.starts_with("#,") && line.contains("fuzzy") { fuzzy = true; }
        } else if let Some(rest) = line.strip_prefix("msgid ") {
            id = unquote(rest); field = 'i';
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            s = unquote(rest); field = 's';
        } else if line.starts_with("msgctxt") {
            has_ctx = true; field = ' ';   // contexted entry -> skipped (server is context-free)
        } else if line.starts_with("msgid_plural") {
            field = ' ';   // plurals not used by server messages
        } else if line.starts_with('"') {
            match field { 'i' => id.push_str(&unquote(line)), 's' => s.push_str(&unquote(line)), _ => {} }
        }
    }
    flush(&mut out, &mut id, &mut s, &mut fuzzy, &mut has_ctx);
    out
}

fn unquote(s: &str) -> String {
    let inner = s.trim().trim_start_matches('"').trim_end_matches('"');
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else { out.push(c); }
    }
    out
}

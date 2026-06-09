/*
 * Internationalization (i18n) module for Clapshot Client.
 *
 * Copyright (c) 2025 Mike-Solar
 * Copyright (c) 2025 Jarno Elonen
 *
 * This file is free software: you may copy, redistribute and/or modify it
 * under the terms of the GNU General Public License as published by the
 * Free Software Foundation, either version 2 of the License, or (at your
 * option) any later version.
 *
 * This file is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * General Public License for more details.
 */

/*
 * gettext-style runtime: the message id IS the English source string (no lookup keys).
 * Translations come from compiled catalogs in ./locales/<locale>.json, produced from the
 * central PO files by `make -C i18n compile` (see i18n/README.md). English is the source
 * language, so it has no catalog and a missing translation falls back to the source string.
 *
 * Mark strings with the reactive `t` store in markup/components (the source string is always first):
 *     {$t("Source string")}                              -> translated, reactive to locale changes
 *     {$t("Hello, {name}!", { name })}                   -> with named placeholders
 *     {$t("Source string", { context: "menu" })}         -> with gettext msgctxt to disambiguate
 */
import { derived, writable, get } from 'svelte/store';
import fi from './locales/fi.json';
import zh from './locales/zh.json';

const STORAGE_KEY = 'clapshot_locale';
const CTX_GLUE = '\u0004';  // gettext's msgctxt<->msgid separator

// Compiled catalogs: { "<msgid>" | "<ctx><msgid>": "translation" }.
// English is the source language (msgid === text), so it has no catalog.
const CATALOGS: Record<string, Record<string, string>> = { fi, zh };

export type Locale = string;

export const availableLocales: { id: string; label: string }[] = [
    { id: 'en', label: 'English' },
    { id: 'fi', label: 'Suomi' },
    { id: 'zh', label: '中文' },
];

export const SUPPORTED_LOCALES: string[] = availableLocales.map((l) => l.id);

export const locale = writable<string>('en');

/**
 * Options for `t()`. Reserved keys: `context` (gettext msgctxt) and `comment` (a hint for translators,
 * extracted into the catalog as a `#.` comment — ignored at runtime). Every other key is a `{named}`
 * placeholder.
 */
const RESERVED = new Set(['context', 'comment']);
export type TOptions = { context?: string; comment?: string } & Record<string, string | number>;

function interpolate(s: string, opts?: TOptions): string {
    if (!opts) return s;
    return s.replace(/\{(\w+)\}/g, (m, k) => (!RESERVED.has(k) && k in opts ? String(opts[k]) : m));
}

function lookup(loc: string, msgid: string, ctx?: string): string {
    const cat = CATALOGS[loc];
    const hit = cat ? cat[ctx ? ctx + CTX_GLUE + msgid : msgid] : undefined;
    return hit ?? msgid;  // fall back to the English source
}

/**
 * Reactive translator. The English source string is always first; pass named placeholders and an
 * optional gettext `context` in the options object:
 *     $t("Connecting server...")
 *     $t("Uploading: {filename}...", { filename })
 *     $t("About", { context: "menu" })
 *     $t("Session ID is {id}", { context: "collab", id })
 *     $t("Title", { context: "subtitle", comment: "the subtitle's name field (a noun)" })  // hint for translators
 */
export const t = derived(locale, ($loc) =>
    (msgid: string, opts?: TOptions) => interpolate(lookup($loc, msgid, opts?.context), opts));

export function setLocale(lang: string) {
    const normalized = SUPPORTED_LOCALES.includes(lang) ? lang : 'en';
    locale.set(normalized);
    localStorage.setItem(STORAGE_KEY, normalized);
    if (typeof document !== 'undefined') {
        document.documentElement.lang = normalized;
    }
}

export function initLocale(configDefault?: string | null, allowed?: string[] | null) {
    const stored = localStorage.getItem(STORAGE_KEY);
    const browser = typeof navigator !== 'undefined' ? navigator.language : 'en';
    const normalizedAllowed = allowed && allowed.length > 0 ? allowed : SUPPORTED_LOCALES;

    // Check candidates in priority order: stored > configDefault > browser
    const checkMatch = (candidate: string | null | undefined): string | null => {
        if (!candidate) return null;
        const matched = normalizedAllowed.find((allowedLocale) =>
            candidate.toLowerCase().startsWith(allowedLocale.toLowerCase())
        );
        return matched || null;
    };

    const selected =
        checkMatch(stored) ??
        checkMatch(configDefault) ??
        checkMatch(browser) ??
        normalizedAllowed[0];

    setLocale(selected);
}

export function currentLocale(): string {
    return get(locale);
}

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

import { derived, writable, get } from 'svelte/store';

const STORAGE_KEY = 'clapshot_locale';

const translations = {
    en: {
        'status.connecting': 'Connecting server...',
        'status.viewConnectionErrors': 'View connection errors',
        'status.latestMessages': 'Latest messages',
        'status.collabActiveTitle': 'Collaborative viewing session active.',
        'status.collabSessionId': 'Session ID is {id}',
        'status.collabActionsMirrored': 'Actions like seek, play and draw are mirrored to all participants.',
        'status.collabInvite': 'To invite people, copy browser URL and send it to them.',
        'status.collabExit': 'Exit by clicking the green icon in header.',
        'status.collabUnderstood': 'Understood',
        'status.reloadToLogin': 'Reload page to log in again.',
        'status.subtitles': 'Subtitles',
        'status.dropInstruction': 'Drop video, audio and image files here to upload',

        'nav.shareToLoggedInUsers': 'Share to logged in users',
        'nav.downloadOriginal': 'Download original',
        'nav.leaveCollab': 'Leave collaborative Session',
        'nav.startCollab': 'Start Collaborative Session',
        'nav.experimentalTools': 'Experimental tools',
        'nav.importEdl': 'Import EDL as Comments',
        'nav.about': 'About',
        'nav.logout': 'Logout',
        'nav.language': 'Language',

        'upload.uploading': 'Uploading: {filename}...',
        'upload.progress': '{percent}% uploaded... please wait',
        'upload.failed': 'Upload Failed',
        'upload.aborted': 'Upload Aborted',
        'upload.rejected': 'Drop rejected. Only video files are allowed.',
        'upload.complete': 'Upload complete',

        'comments.placeholderTimed': 'Add a comment - at current time...',
        'comments.placeholderUntimed': 'Add a comment...',
        'comments.timedToggleTitle': 'Comment is time specific?',
        'comments.drawOnVideo': 'Draw on video',
        'comments.send': 'Send',
        'comments.undo': 'Undo',
        'comments.redo': 'Redo',
        'comments.deleteConfirm': 'Delete comment?',
        'comments.copyLink': 'Copy link',
        'comments.linkCopied': 'Link copied to clipboard',
        'comments.reply': 'Reply',
        'comments.edit': 'Edit',
        'comments.deleteShort': 'Del',
        'comments.yourReply': 'Your reply...',
        'comments.editedSuffix': '(edited)',

        'subtitles.edit': 'Edit subtitle',
        'subtitles.label': 'Title',
        'subtitles.languageCode': 'Language code',
        'subtitles.timeOffset': 'Time offset (sec)',
        'subtitles.defaultSubtitle': 'Default Subtitle',
        'subtitles.save': 'Save',
        'subtitles.download': 'Download',
        'subtitles.delete': 'Del',

        'general.ok': 'OK',
    },
    zh: {
        'status.connecting': '正在连接服务器...',
        'status.viewConnectionErrors': '查看连接错误',
        'status.latestMessages': '最新消息',
        'status.collabActiveTitle': '协同观看会话已启用。',
        'status.collabSessionId': '会话 ID：{id}',
        'status.collabActionsMirrored': '寻址、播放、绘图等操作会同步给所有参与者。',
        'status.collabInvite': '邀请他人时，复制浏览器链接并发送给他们。',
        'status.collabExit': '点击标题栏中的绿色图标退出。',
        'status.collabUnderstood': '知道了',
        'status.reloadToLogin': '请重新加载页面以再次登录。',
        'status.subtitles': '字幕',
        'status.dropInstruction': '将视频、音频或图片文件拖到此处上传',

        'nav.shareToLoggedInUsers': '分享给已登录用户',
        'nav.downloadOriginal': '下载原文件',
        'nav.leaveCollab': '离开协同会话',
        'nav.startCollab': '开始协同会话',
        'nav.experimentalTools': '实验工具',
        'nav.importEdl': '导入 EDL 为评论',
        'nav.about': '关于',
        'nav.logout': '退出',
        'nav.language': '语言',

        'upload.uploading': '正在上传：{filename}...',
        'upload.progress': '已上传 {percent}% ... 请稍候',
        'upload.failed': '上传失败',
        'upload.aborted': '上传已中止',
        'upload.rejected': '不支持此文件类型。仅支持视频、音频和图片。',
        'upload.complete': '上传完成',

        'comments.placeholderTimed': '添加评论 —— 使用当前时间点...',
        'comments.placeholderUntimed': '添加评论...',
        'comments.timedToggleTitle': '这条评论包含时间码？',
        'comments.drawOnVideo': '在视频上绘制',
        'comments.send': '发送',
        'comments.undo': '撤销',
        'comments.redo': '重做',
        'comments.deleteConfirm': '删除评论？',
        'comments.copyLink': '复制链接',
        'comments.linkCopied': '链接已复制',
        'comments.reply': '回复',
        'comments.edit': '编辑',
        'comments.deleteShort': '删除',
        'comments.yourReply': '请输入回复...',
        'comments.editedSuffix': '（已编辑）',

        'subtitles.edit': '编辑字幕',
        'subtitles.label': '标题',
        'subtitles.languageCode': '语言代码',
        'subtitles.timeOffset': '时间偏移（秒）',
        'subtitles.defaultSubtitle': '默认字幕',
        'subtitles.save': '保存',
        'subtitles.download': '下载',
        'subtitles.delete': '删除',

        'general.ok': '确定',
    },
    fi: {
        'status.connecting': 'Yhdistetään palvelimeen...',
        'status.viewConnectionErrors': 'Näytä yhteysvirheet',
        'status.latestMessages': 'Viimeisimmät viestit',
        'status.collabActiveTitle': 'Yhteiskatseluistunto aktiivinen.',
        'status.collabSessionId': 'Istunnon tunnus on {id}',
        'status.collabActionsMirrored': 'Toiminnot kuten kelaus, toisto ja piirtäminen peilataan kaikille osallistujille.',
        'status.collabInvite': 'Kutsuaksesi muita, kopioi selaimen osoite ja lähetä se heille.',
        'status.collabExit': 'Poistu napsauttamalla vihreää kuvaketta ylätunnisteessa.',
        'status.collabUnderstood': 'Selvä',
        'status.reloadToLogin': 'Lataa sivu uudelleen kirjautuaksesi.',
        'status.subtitles': 'Tekstitykset',
        'status.dropInstruction': 'Pudota video-, ääni- ja kuvatiedostoja tähän ladataksesi',

        'nav.shareToLoggedInUsers': 'Jaa kirjautuneille käyttäjille',
        'nav.downloadOriginal': 'Lataa alkuperäinen',
        'nav.leaveCollab': 'Poistu yhteisistunnosta',
        'nav.startCollab': 'Aloita yhteisistunto',
        'nav.experimentalTools': 'Kokeelliset työkalut',
        'nav.importEdl': 'Tuo EDL kommentteina',
        'nav.about': 'Tietoja',
        'nav.logout': 'Kirjaudu ulos',
        'nav.language': 'Kieli',

        'upload.uploading': 'Ladataan: {filename}...',
        'upload.progress': '{percent}% ladattu... odota hetki',
        'upload.failed': 'Lataus epäonnistui',
        'upload.aborted': 'Lataus keskeytetty',
        'upload.rejected': 'Tiedostotyyppiä ei tueta. Sallitut: video, kuva ja ääni.',
        'upload.complete': 'Lataus valmis',

        'comments.placeholderTimed': 'Lisää kommentti (nykyiseen kohtaan)...',
        'comments.placeholderUntimed': 'Lisää kommentti...',
        'comments.timedToggleTitle': 'Kommentti on aikakohtainen?',
        'comments.drawOnVideo': 'Piirrä videoon',
        'comments.send': 'Lähetä',
        'comments.undo': 'Kumoa',
        'comments.redo': 'Tee uudelleen',
        'comments.deleteConfirm': 'Poista kommentti?',
        'comments.copyLink': 'Kopioi linkki',
        'comments.linkCopied': 'Linkki kopioitu leikepöydälle',
        'comments.reply': 'Vastaa',
        'comments.edit': 'Muokkaa',
        'comments.deleteShort': 'Poista',
        'comments.yourReply': 'Vastauksesi...',
        'comments.editedSuffix': '(muokattu)',

        'subtitles.edit': 'Muokkaa tekstitystä',
        'subtitles.label': 'Otsikko',
        'subtitles.languageCode': 'Kielikoodi',
        'subtitles.timeOffset': 'Aikasiirtymä (sek)',
        'subtitles.defaultSubtitle': 'Oletustekstitys',
        'subtitles.save': 'Tallenna',
        'subtitles.download': 'Lataa',
        'subtitles.delete': 'Poista',

        'general.ok': 'OK',
    },
};

export type Locale = keyof typeof translations;
export type TranslationKey = keyof typeof translations.en;

export const SUPPORTED_LOCALES = Object.keys(translations) as Locale[];

export const locale = writable<Locale>('en');

function format(template: string, vars?: Record<string, string | number>): string {
    if (!vars) return template;
    return template.replace(/\{(\w+)\}/g, (_match, key) => (key in vars ? String(vars[key]) : ''));
}

export const t = derived(locale, ($locale) => {
    return (key: TranslationKey, vars?: Record<string, string | number>) => {
        const lang = translations[$locale] ? $locale : 'en';
        const langTranslations = translations[lang];
        const msg = langTranslations[key] ?? translations.en[key] ?? key;
        return format(msg, vars);
    };
});

export const availableLocales: { id: Locale; label: string }[] = [
    { id: 'en', label: 'English' },
    { id: 'fi', label: 'Suomi' },
    { id: 'zh', label: '中文' },
];

export function setLocale(lang: string) {
    const normalized = SUPPORTED_LOCALES.includes(lang as Locale) ? (lang as Locale) : 'en';
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
    // stored: user's previously saved choice from localStorage
    // configDefault: server-provided default_locale from config file
    // browser: browser's language setting
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

export function currentLocale(): Locale {
    return get(locale);
}

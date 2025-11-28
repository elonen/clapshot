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
        'upload.rejected': '拖拽被拒绝。只允许上传视频、图片或音频文件。',
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
};

export type Locale = keyof typeof translations;
export type TranslationKey = keyof typeof translations.en;

export const locale = writable<Locale>('en');

function format(template: string, vars?: Record<string, string | number>): string {
    if (!vars) return template;
    return template.replace(/\{(\w+)\}/g, (_match, key) => (key in vars ? String(vars[key]) : ''));
}

export const t = derived(locale, ($locale) => {
    return (key: TranslationKey, vars?: Record<string, string | number>) => {
        const lang = translations[$locale] ? $locale : 'en';
        const msg = (translations as any)[lang]?.[key] ?? (translations as any).en?.[key] ?? key;
        return format(msg, vars);
    };
});

export const availableLocales: { id: Locale; label: string }[] = [
    { id: 'en', label: 'English' },
    { id: 'zh', label: '中文' },
];

export function setLocale(lang: string) {
    const normalized = (['en', 'zh'] as string[]).includes(lang) ? (lang as Locale) : 'en';
    locale.set(normalized);
    localStorage.setItem(STORAGE_KEY, normalized);
    if (typeof document !== 'undefined') {
        document.documentElement.lang = normalized;
    }
}

export function initLocale(preferred?: string | null, allowed?: string[] | null) {
    const stored = localStorage.getItem(STORAGE_KEY);
    const browser = typeof navigator !== 'undefined' ? navigator.language : 'en';
    const candidates = [stored, preferred, browser].filter(Boolean) as string[];
    const normalizedAllowed = allowed && allowed.length > 0 ? allowed : ['en', 'zh'];

    const selected =
        candidates.find((c) =>
            normalizedAllowed.some((allowedLocale) => c?.toLowerCase().startsWith(allowedLocale.toLowerCase()))
        ) ?? normalizedAllowed[0];

    setLocale(selected);
}

export function currentLocale(): Locale {
    return get(locale);
}

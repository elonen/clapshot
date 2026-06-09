/**
 * Tests for i18n module
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import { locale, setLocale, initLocale, t, availableLocales, SUPPORTED_LOCALES } from '@/i18n';

describe('i18n', () => {
    beforeEach(() => {
        // Reset locale to default before each test
        setLocale('en');
        // Clear localStorage
        localStorage.clear();
        // Clear any document.documentElement.lang
        if (typeof document !== 'undefined') {
            document.documentElement.lang = '';
        }
    });

    describe('setLocale', () => {
        it('should set a valid locale', () => {
            setLocale('zh');
            expect(get(locale)).toBe('zh');
        });

        it('should fall back to "en" for invalid locale', () => {
            setLocale('invalid-locale');
            expect(get(locale)).toBe('en');
        });

        it('should persist locale to localStorage', () => {
            setLocale('zh');
            expect(localStorage.getItem('clapshot_locale')).toBe('zh');
        });

        it('should set document.documentElement.lang', () => {
            setLocale('zh');
            if (typeof document !== 'undefined') {
                expect(document.documentElement.lang).toBe('zh');
            }
        });

        it('should support all locales defined in translations', () => {
            // Test that all locales work
            setLocale('en');
            expect(get(locale)).toBe('en');

            setLocale('fi');
            expect(get(locale)).toBe('fi');

            setLocale('zh');
            expect(get(locale)).toBe('zh');
        });
    });

    describe('initLocale', () => {
        it('should use stored locale when available', () => {
            localStorage.setItem('clapshot_locale', 'zh');
            initLocale('en', ['en', 'zh']);
            expect(get(locale)).toBe('zh');
        });

        it('should use config default when no stored locale', () => {
            initLocale('zh', ['en', 'zh']);
            expect(get(locale)).toBe('zh');
        });

        it('should fall back to browser language when no stored or config default', () => {
            // Mock navigator.language
            Object.defineProperty(window.navigator, 'language', {
                writable: true,
                configurable: true,
                value: 'zh-CN'
            });

            initLocale(null, ['en', 'zh']);
            expect(get(locale)).toBe('zh');
        });

        it('should respect allowed locales list', () => {
            localStorage.setItem('clapshot_locale', 'zh');
            initLocale(null, ['en']); // Only allow 'en'
            expect(get(locale)).toBe('en'); // Should fall back since 'zh' not allowed
        });

        it('should use first allowed locale as ultimate fallback', () => {
            // Mock browser language to something that doesn't match any allowed locale
            Object.defineProperty(window.navigator, 'language', {
                writable: true,
                configurable: true,
                value: 'ab-CD'
            });

            // Use 'fi' as the first locale to ensure we're actually picking the first one,
            // not just defaulting to English
            initLocale(null, ['fi', 'en', 'zh']);
            expect(get(locale)).toBe('fi');
        });

        it('should handle browser language with region codes', () => {
            Object.defineProperty(window.navigator, 'language', {
                writable: true,
                configurable: true,
                value: 'zh-TW'
            });

            initLocale(null, ['en', 'zh']);
            expect(get(locale)).toBe('zh'); // Should match 'zh' from 'zh-TW'
        });
    });

    describe('translation function', () => {
        // msgid IS the English source string; a missing translation falls back to it.
        it('should return the source string for English (the source language)', () => {
            setLocale('en');
            const $t = get(t);
            expect($t('Connecting server...')).toBe('Connecting server...');
        });

        it('should return Chinese translation when locale is zh', () => {
            setLocale('zh');
            const $t = get(t);
            expect($t('Connecting server...')).toBe('正在连接服务器...');
        });

        it('should return Finnish translation when locale is fi', () => {
            setLocale('fi');
            const $t = get(t);
            expect($t('Connecting server...')).toBe('Yhdistetään palvelimeen...');
        });

        it('should support parameterized translations', () => {
            setLocale('en');
            const $t = get(t);
            expect($t('Uploading: {filename}...', { filename: 'test.mp4' })).toBe('Uploading: test.mp4...');
        });

        it('should support parameterized Chinese translations', () => {
            setLocale('zh');
            const $t = get(t);
            expect($t('Uploading: {filename}...', { filename: 'test.mp4' })).toBe('正在上传：test.mp4...');
        });

        it('should support parameterized Finnish translations', () => {
            setLocale('fi');
            const $t = get(t);
            expect($t('Uploading: {filename}...', { filename: 'test.mp4' })).toBe('Ladataan: test.mp4...');
        });

        it('should support multiple parameters', () => {
            setLocale('en');
            const $t = get(t);
            expect($t('{percent}% uploaded... please wait', { percent: 50 })).toBe('50% uploaded... please wait');
        });

        it('should translate a context-scoped string in the target locale', () => {
            setLocale('zh');
            const $t = get(t);
            expect($t('About', { context: 'menu' })).toBe('关于');
        });

        it('should fall back to the source string when a translation is missing', () => {
            setLocale('zh');
            const $t = get(t);
            expect($t('This source has no translation anywhere')).toBe('This source has no translation anywhere');
        });
    });

    describe('availableLocales', () => {
        it('should have an entry for every supported translation', () => {
            const availableIds = availableLocales.map(l => l.id).sort();
            const supportedIds = [...SUPPORTED_LOCALES].sort();
            expect(availableIds).toEqual(supportedIds);
        });
    });
});

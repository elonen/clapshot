import { describe, it, expect, beforeEach, vi } from 'vitest';
import Cookies from '@/cookies';

describe('LocalStorageCookies', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
  });

  describe('set / get', () => {
    it('stores and retrieves a cookie value', () => {
      const now = Date.now();
      Cookies.set('session', 'abc123', now + 10000);
      expect(Cookies.get('session')).toBe('abc123');
    });

    it('returns null for a missing key', () => {
      expect(Cookies.get('missing')).toBeNull();
    });

    it('uses a default 12-hour expiration when none is provided', () => {
      const before = Date.now();
      Cookies.set('session', 'abc123', null as any);
      const stored = JSON.parse(localStorage.setItem.mock.calls.at(-1)[1]);
      const expiration = stored['session'].expiration;
      expect(expiration).toBeGreaterThanOrEqual(before + 60 * 60 * 12);
      expect(expiration).toBeLessThanOrEqual(Date.now() + 60 * 60 * 12 + 1000);
    });

    it('stores an empty value as an empty string', () => {
      Cookies.set('session', 'abc123', Date.now() + 10000);
      expect(Cookies.get('session')).toBe('abc123');
      Cookies.set('session', '', Date.now() + 10000);
      expect(Cookies.get('session')).toBe('');
    });

    it('does not store a cookie with an empty key', () => {
      const before = localStorage.setItem.mock.calls.length;
      Cookies.set('', 'value', Date.now() + 10000);
      // Empty key branch still calls delete and setItem, but no key is added
      const stored = JSON.parse(localStorage.setItem.mock.calls.at(-1)[1]);
      expect(stored).toEqual({});
    });
  });

  describe('expiration', () => {
    it('returns null and removes expired cookies on get', () => {
      const now = Date.now();
      Cookies.set('session', 'abc123', now - 1);
      expect(Cookies.get('session')).toBeNull();
      const stored = JSON.parse(localStorage.setItem.mock.calls.at(-1)[1]);
      expect(stored).not.toHaveProperty('session');
    });

    it('filters out expired cookies in getAllNonExpired', () => {
      const now = Date.now();
      Cookies.set('fresh', 'yes', now + 10000);
      Cookies.set('expired', 'no', now - 1);
      const all = Cookies.getAllNonExpired();
      expect(all).toHaveProperty('fresh', 'yes');
      expect(all).not.toHaveProperty('expired');
      const stored = JSON.parse(localStorage.setItem.mock.calls.at(-1)[1]);
      expect(stored).not.toHaveProperty('expired');
    });

    it('returns an empty object when no cookies exist', () => {
      expect(Cookies.getAllNonExpired()).toEqual({});
    });

    it('returns only non-expired values keyed by name', () => {
      const now = Date.now();
      Cookies.set('a', 'one', now + 10000);
      Cookies.set('b', 'two', now + 20000);
      const all = Cookies.getAllNonExpired();
      expect(Object.keys(all)).toEqual(['a', 'b']);
      expect(all['a']).toBe('one');
      expect(all['b']).toBe('two');
    });
  });

  describe('persistence', () => {
    it('survives a fresh instance read from localStorage', () => {
      Cookies.set('session', 'abc123', Date.now() + 10000);
      // Re-importing the module is not practical because it is cached,
      // but reading localStorage directly confirms the serialized shape.
      const raw = localStorage.getItem('clapshot_state');
      const parsed = JSON.parse(raw!);
      expect(parsed).toHaveProperty('session');
      expect(parsed['session']).toMatchObject({ value: 'abc123' });
    });
  });
});

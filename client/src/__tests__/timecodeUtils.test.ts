import { describe, it, expect } from 'vitest';
import { timecodeToSeconds, timecodeToSecondsOrNull } from '@/lib/timecodeUtils';

describe('timecodeToSeconds', () => {
  it('parses HH:MM:SS.mmm format', () => {
    expect(timecodeToSeconds('00:00:00.000')).toBe(0);
    expect(timecodeToSeconds('00:01:30.500')).toBeCloseTo(90.5);
    expect(timecodeToSeconds('01:00:00.000')).toBe(3600);
    expect(timecodeToSeconds('12:34:56.789')).toBeCloseTo(12 * 3600 + 34 * 60 + 56.789);
  });

  it('parses HH:MM:SS:FF format assuming 25fps', () => {
    expect(timecodeToSeconds('00:00:00:00')).toBe(0);
    expect(timecodeToSeconds('00:01:30:00')).toBe(90);
    expect(timecodeToSeconds('00:00:01:12')).toBeCloseTo(1 + 12 / 25);
    expect(timecodeToSeconds('01:00:00:25')).toBe(3601);
  });

  it('returns 0 for empty or falsy input', () => {
    expect(timecodeToSeconds('')).toBe(0);
    // @ts-expect-error testing undefined handling
    expect(timecodeToSeconds(undefined)).toBe(0);
    // @ts-expect-error testing null handling
    expect(timecodeToSeconds(null)).toBe(0);
  });

  it('returns 0 for unparseable strings', () => {
    expect(timecodeToSeconds('invalid')).toBe(0);
    expect(timecodeToSeconds('30.5s')).toBe(0);
    expect(timecodeToSeconds('00:01:30')).toBe(0);
    expect(timecodeToSeconds('00:01:30:FF')).toBe(0);
  });
});

describe('timecodeToSecondsOrNull', () => {
  it('returns null for undefined/empty/null', () => {
    expect(timecodeToSecondsOrNull(undefined)).toBeNull();
    expect(timecodeToSecondsOrNull('')).toBeNull();
    // @ts-expect-error testing null handling
    expect(timecodeToSecondsOrNull(null)).toBeNull();
  });

  it('returns 0 only for genuine 00:00:00.000 timecodes', () => {
    expect(timecodeToSecondsOrNull('00:00:00.000')).toBe(0);
    expect(timecodeToSecondsOrNull('00:00:00:00')).toBe(0);
  });

  it('returns null for unparseable strings', () => {
    expect(timecodeToSecondsOrNull('invalid')).toBeNull();
    expect(timecodeToSecondsOrNull('30.5s')).toBeNull();
    expect(timecodeToSecondsOrNull('00:01:30')).toBeNull();
  });

  it('returns seconds for valid timecodes', () => {
    expect(timecodeToSecondsOrNull('00:01:30.500')).toBeCloseTo(90.5);
    expect(timecodeToSecondsOrNull('00:01:30:00')).toBe(90);
    expect(timecodeToSecondsOrNull('01:00:00.000')).toBe(3600);
  });
});

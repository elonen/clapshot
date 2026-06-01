import { describe, it, expect } from 'vitest';
import { TimecodeUtils } from '@/lib/player_view/video-decoder/timecode';

// Regression tests for the audio fps=0 crash.
//
// Audio files used to be stored with fps=0, which reached the player as frameRate=0.
// frameToSMPTE(_, 0) then divided by zero and produced "NaN:NaN:NaN:NaN", which was
// fed back into smpteToFrame() and threw, unwinding the Svelte reactive chain and
// taking down the whole player UI. Timecode math must degrade gracefully instead:
// never emit NaN, never throw.
describe('TimecodeUtils with an invalid frame rate', () => {

  const INVALID_RATES = [0, NaN, -1, Infinity];

  describe('frameToSMPTE / timeToSMPTE never emit NaN', () => {
    for (const fps of INVALID_RATES) {
      it(`returns 00:00:00:00 for fps=${fps}`, () => {
        expect(TimecodeUtils.frameToSMPTE(0, fps)).toBe('00:00:00:00');
        expect(TimecodeUtils.frameToSMPTE(123, fps)).toBe('00:00:00:00');
        expect(TimecodeUtils.timeToSMPTE(10, fps)).toBe('00:00:00:00');
      });
    }
  });

  it('still formats correctly for a valid frame rate', () => {
    expect(TimecodeUtils.frameToSMPTE(0, 60)).toBe('00:00:00:00');
    expect(TimecodeUtils.frameToSMPTE(90, 60)).toBe('00:00:01:30');
    expect(TimecodeUtils.timeToSMPTE(1.5, 60)).toBe('00:00:01:30');
  });

  describe('smpteToFrame / smpteToTime never throw on bad input', () => {
    it('returns 0 for a NaN timecode string (could be persisted while fps was 0)', () => {
      expect(TimecodeUtils.smpteToFrame('NaN:NaN:NaN:NaN', 60)).toBe(0);
      expect(TimecodeUtils.smpteToTime('NaN:NaN:NaN:NaN', 60)).toBe(0);
      expect(TimecodeUtils.smpteToMilliseconds('NaN:NaN:NaN:NaN', 60)).toBe(0);
    });

    it('returns 0 for a malformed timecode string', () => {
      expect(TimecodeUtils.smpteToFrame('garbage', 60)).toBe(0);
      expect(TimecodeUtils.smpteToFrame('', 60)).toBe(0);
    });

    it('returns 0 when the frame rate itself is invalid', () => {
      expect(TimecodeUtils.smpteToFrame('00:00:01:00', NaN)).toBe(0);
      expect(TimecodeUtils.smpteToFrame('00:00:01:00', 0)).toBe(0);
    });

    it('still parses a valid timecode', () => {
      expect(TimecodeUtils.smpteToFrame('00:00:01:30', 60)).toBe(90);
      expect(TimecodeUtils.smpteToTime('00:00:01:30', 60)).toBe(1.5);
    });
  });
});

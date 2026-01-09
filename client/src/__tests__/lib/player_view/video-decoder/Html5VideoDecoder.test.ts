/**
 * Tests for Html5VideoDecoder - IVideoDecoder implementation with inlined timecode logic
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { Html5VideoDecoder } from '@/lib/player_view/video-decoder/Html5VideoDecoder';
import { TimecodeUtils } from '@/lib/player_view/video-decoder/timecode';
import type { VideoDecoderConfig } from '@/lib/player_view/video-decoder/types';

const createMockVideoElement = (currentTime: number = 0): HTMLVideoElement => {
  return {
    currentTime,
    paused: true,
    ended: false,
    pause: vi.fn(),
    style: { visibility: '' },
  } as any;
};

describe('TimecodeUtils', () => {
  describe('frameToSMPTE', () => {
    it('should convert frame 0 to 00:00:00:00', () => {
      expect(TimecodeUtils.frameToSMPTE(0, 24)).toBe('00:00:00:00');
    });

    it('should convert 24 frames at 24fps to 00:00:01:00', () => {
      expect(TimecodeUtils.frameToSMPTE(24, 24)).toBe('00:00:01:00');
    });

    it('should convert 30 frames at 30fps to 00:00:01:00', () => {
      expect(TimecodeUtils.frameToSMPTE(30, 30)).toBe('00:00:01:00');
    });

    it('should handle hours correctly', () => {
      // 1 hour at 24fps = 24 * 60 * 60 = 86400 frames
      expect(TimecodeUtils.frameToSMPTE(86400, 24)).toBe('01:00:00:00');
    });

    it('should handle complex timecodes', () => {
      // 1h23m45s12f at 24fps
      const frames = 1 * 86400 + 23 * 1440 + 45 * 24 + 12;
      expect(TimecodeUtils.frameToSMPTE(frames, 24)).toBe('01:23:45:12');
    });
  });

  describe('smpteToFrame', () => {
    it('should convert 00:00:00:00 to frame 0', () => {
      expect(TimecodeUtils.smpteToFrame('00:00:00:00', 24)).toBe(0);
    });

    it('should convert 00:00:01:00 at 24fps to frame 24', () => {
      expect(TimecodeUtils.smpteToFrame('00:00:01:00', 24)).toBe(24);
    });

    it('should convert 00:00:01:12 at 24fps to frame 36', () => {
      expect(TimecodeUtils.smpteToFrame('00:00:01:12', 24)).toBe(36);
    });

    it('should handle HH:MM:SS format (no frames)', () => {
      expect(TimecodeUtils.smpteToFrame('00:01:00', 24)).toBe(1440);
    });

    it('should throw on invalid timecode', () => {
      expect(() => TimecodeUtils.smpteToFrame('invalid', 24)).toThrow();
    });
  });

  describe('smpteToTime', () => {
    it('should convert 00:00:01:00 at 24fps to 1.0 seconds', () => {
      expect(TimecodeUtils.smpteToTime('00:00:01:00', 24)).toBe(1);
    });

    it('should convert 00:00:01:12 at 24fps to 1.5 seconds', () => {
      expect(TimecodeUtils.smpteToTime('00:00:01:12', 24)).toBe(1.5);
    });
  });

  describe('smpteToMilliseconds', () => {
    it('should convert 00:00:01:00 to 1000ms', () => {
      expect(TimecodeUtils.smpteToMilliseconds('00:00:01:00', 24)).toBe(1000);
    });

    it('should convert 00:00:01:12 at 24fps to 1500ms', () => {
      expect(TimecodeUtils.smpteToMilliseconds('00:00:01:12', 24)).toBe(1500);
    });
  });

  describe('timeToFrame', () => {
    it('should convert 0 seconds to frame 0', () => {
      expect(TimecodeUtils.timeToFrame(0, 24)).toBe(0);
    });

    it('should convert 1 second at 24fps to frame 24', () => {
      expect(TimecodeUtils.timeToFrame(1, 24)).toBe(24);
    });

    it('should convert 2.5 seconds at 24fps to frame 60', () => {
      expect(TimecodeUtils.timeToFrame(2.5, 24)).toBe(60);
    });
  });

  describe('roundtrip conversions', () => {
    it('should roundtrip frame -> SMPTE -> frame', () => {
      const originalFrame = 12345;
      const smpte = TimecodeUtils.frameToSMPTE(originalFrame, 30);
      const backToFrame = TimecodeUtils.smpteToFrame(smpte, 30);
      expect(backToFrame).toBe(originalFrame);
    });

    it('should roundtrip time -> frame -> time (within frame precision)', () => {
      const originalTime = 123.456;
      const frame = TimecodeUtils.timeToFrame(originalTime, 24);
      const backToTime = TimecodeUtils.frameToTime(frame, 24);
      // Should be within one frame of precision
      expect(Math.abs(backToTime - originalTime)).toBeLessThan(1 / 24);
    });
  });
});

describe('Html5VideoDecoder', () => {
  let mockVideo: HTMLVideoElement;
  let decoder: Html5VideoDecoder;
  const defaultConfig: VideoDecoderConfig = {
    frameRate: 24,
    duration: 120,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockVideo = createMockVideoElement();
    decoder = new Html5VideoDecoder(mockVideo);
  });

  describe('init()', () => {
    it('should initialize successfully', async () => {
      const result = await decoder.init(defaultConfig);
      expect(result).toBe(true);
    });

    it('should set frameRate from config', async () => {
      await decoder.init({ frameRate: 30, duration: 60 });
      expect(decoder.frameRate).toBe(30);
    });

    it('should default to 24fps before init', () => {
      expect(decoder.frameRate).toBe(24);
    });
  });

  describe('getPosition()', () => {
    it('should return zero position before init', () => {
      const pos = decoder.getPosition();
      expect(pos.timestamp).toBe(0);
      expect(pos.frame).toBe(0);
      expect(pos.timecode).toBe('00:00:00:00');
    });

    it('should return correct position after init', async () => {
      await decoder.init(defaultConfig);
      mockVideo.currentTime = 1;
      const pos = decoder.getPosition();
      expect(pos.timestamp).toBe(1);
      expect(pos.frame).toBe(24);
      expect(pos.timecode).toBe('00:00:01:00');
    });

    it('should handle fractional seconds', async () => {
      await decoder.init(defaultConfig);
      mockVideo.currentTime = 2.5;
      const pos = decoder.getPosition();
      expect(pos.frame).toBe(60);
    });
  });

  describe('stepFrame()', () => {
    beforeEach(async () => {
      await decoder.init(defaultConfig);
    });

    it('should throw if not initialized', async () => {
      const uninitDecoder = new Html5VideoDecoder(mockVideo);
      await expect(uninitDecoder.stepFrame(1)).rejects.toThrow('not initialized');
    });

    it('should step forward one frame', async () => {
      mockVideo.currentTime = 1; // frame 24
      const pos = await decoder.stepFrame(1);
      expect(pos.frame).toBe(25);
      // Seeks to middle of frame for browser tolerance
      expect(mockVideo.currentTime).toBeCloseTo(25.5 / 24, 5);
    });

    it('should step backward one frame', async () => {
      mockVideo.currentTime = 1; // frame 24
      const pos = await decoder.stepFrame(-1);
      expect(pos.frame).toBe(23);
      expect(mockVideo.currentTime).toBeCloseTo(23.5 / 24, 5);
    });

    it('should step multiple frames forward', async () => {
      mockVideo.currentTime = 0;
      const pos = await decoder.stepFrame(1, 5);
      expect(pos.frame).toBe(5);
      expect(mockVideo.currentTime).toBeCloseTo(5.5 / 24, 5);
    });

    it('should not go below 0', async () => {
      mockVideo.currentTime = 0;
      const pos = await decoder.stepFrame(-1, 5);
      expect(pos.frame).toBe(0);
      // Frame 0 seeks to 0.5/24 (middle of frame 0)
      expect(mockVideo.currentTime).toBeCloseTo(0.5 / 24, 5);
    });

    it('should pause video if playing', async () => {
      (mockVideo as any).paused = false;
      mockVideo.currentTime = 1;
      await decoder.stepFrame(1);
      expect(mockVideo.pause).toHaveBeenCalled();
    });
  });

  describe('seekToTime()', () => {
    beforeEach(async () => {
      await decoder.init(defaultConfig);
    });

    it('should seek to exact timestamp', async () => {
      const pos = await decoder.seekToTime(5.5);
      expect(mockVideo.currentTime).toBe(5.5);
      expect(pos.timestamp).toBe(5.5);
    });

    it('should not seek to negative time', async () => {
      await decoder.seekToTime(-5);
      expect(mockVideo.currentTime).toBe(0);
    });
  });

  describe('seekToFrame()', () => {
    beforeEach(async () => {
      await decoder.init(defaultConfig);
    });

    it('should throw if not initialized', async () => {
      const uninitDecoder = new Html5VideoDecoder(mockVideo);
      await expect(uninitDecoder.seekToFrame(100)).rejects.toThrow('not initialized');
    });

    it('should seek to frame 0', async () => {
      const pos = await decoder.seekToFrame(0);
      expect(pos.frame).toBe(0);
      // Seeks to middle of frame for browser tolerance
      expect(mockVideo.currentTime).toBeCloseTo(0.5 / 24, 5);
    });

    it('should seek to frame 24 (1 second at 24fps)', async () => {
      const pos = await decoder.seekToFrame(24);
      expect(pos.frame).toBe(24);
      expect(mockVideo.currentTime).toBeCloseTo(24.5 / 24, 5);
    });

    it('should seek to frame 36 (1.5 seconds at 24fps)', async () => {
      const pos = await decoder.seekToFrame(36);
      expect(pos.frame).toBe(36);
      expect(mockVideo.currentTime).toBeCloseTo(36.5 / 24, 5);
    });
  });

  describe('activate()', () => {
    it('should set video visibility to visible', async () => {
      await decoder.init(defaultConfig);
      mockVideo.style.visibility = 'hidden';
      decoder.activate();
      expect(mockVideo.style.visibility).toBe('visible');
    });
  });

  describe('captureFrame()', () => {
    it('should call drawImage on the provided context', async () => {
      await decoder.init(defaultConfig);
      // Mock context since jsdom doesn't provide full canvas support
      const mockCtx = { drawImage: vi.fn() } as unknown as CanvasRenderingContext2D;
      decoder.captureFrame(mockCtx);
      expect(mockCtx.drawImage).toHaveBeenCalledWith(mockVideo, 0, 0);
    });
  });

  describe('deactivate() and dispose()', () => {
    it('should allow deactivate without errors', async () => {
      await decoder.init(defaultConfig);
      expect(() => decoder.deactivate()).not.toThrow();
    });

    it('should allow dispose without errors', async () => {
      await decoder.init(defaultConfig);
      expect(() => decoder.dispose()).not.toThrow();
    });

    it('should return zero position after dispose', async () => {
      await decoder.init(defaultConfig);
      mockVideo.currentTime = 5;
      decoder.dispose();
      const pos = decoder.getPosition();
      expect(pos.frame).toBe(0);
    });
  });
});

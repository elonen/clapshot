/**
 * Tests for HybridVideoDecoder - orchestrator that manages Html5/Mediabunny switching
 * and ensures videoElement.currentTime stays in sync for UI bindings.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { HybridVideoDecoder } from '@/lib/player_view/video-decoder/HybridVideoDecoder';
import type { IVideoDecoder, FramePosition, VideoDecoderConfig } from '@/lib/player_view/video-decoder/types';

// Mock mediabunny module to prevent WebCodecs dependency
vi.mock('mediabunny', () => ({
  Input: vi.fn(),
  ALL_FORMATS: [],
  UrlSource: vi.fn(),
  BlobSource: vi.fn(),
  VideoSampleSink: vi.fn(),
}));

const createMockVideoElement = (currentTime = 0): HTMLVideoElement => {
  return {
    currentTime,
    paused: true,
    ended: false,
    duration: 120,
    videoWidth: 1920,
    videoHeight: 1080,
    pause: vi.fn(),
    load: vi.fn(),
    style: { visibility: '' },
  } as unknown as HTMLVideoElement;
};

const createMockContainer = (): HTMLElement => {
  return {
    appendChild: vi.fn(),
    getBoundingClientRect: () => ({ width: 800, height: 600 }),
  } as unknown as HTMLElement;
};

describe('HybridVideoDecoder', () => {
  let mockVideo: HTMLVideoElement;
  let mockContainer: HTMLElement;
  let decoder: HybridVideoDecoder;
  const defaultConfig: VideoDecoderConfig = {
    frameRate: 24,
    duration: 120,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockVideo = createMockVideoElement();
    mockContainer = createMockContainer();

    // Disable Mediabunny (WebCodecs not available in test env)
    decoder = new HybridVideoDecoder({
      videoElement: mockVideo,
      videoSource: 'test-video.mp4',
      container: mockContainer,
      frameRate: 24,
      duration: 120,
      enableMediabunny: false,
    });
  });

  describe('videoElement.currentTime synchronization', () => {
    beforeEach(async () => {
      await decoder.init(defaultConfig);
    });

    it('should sync videoElement.currentTime after seekToTime', async () => {
      mockVideo.currentTime = 0;

      const pos = await decoder.seekToTime(5.5);

      expect(pos.timestamp).toBe(5.5);
      expect(mockVideo.currentTime).toBe(5.5);
    });

    it('should sync videoElement.currentTime after seekToFrame', async () => {
      mockVideo.currentTime = 0;

      const pos = await decoder.seekToFrame(48); // 2 seconds at 24fps

      expect(pos.frame).toBe(48);
      // Html5VideoDecoder seeks to middle of frame: (48 + 0.5) / 24 = 2.02083...
      expect(mockVideo.currentTime).toBeCloseTo(48.5 / 24, 5);
    });

    it('should update videoElement.currentTime to match returned position', async () => {
      // This ensures UI bound to videoElement.currentTime stays in sync
      mockVideo.currentTime = 10; // Start at different position

      await decoder.seekToTime(2.0);

      // videoElement should now reflect the seek target
      expect(mockVideo.currentTime).toBe(2.0);
    });
  });

  describe('bidirectional timeline interaction', () => {
    beforeEach(async () => {
      await decoder.init(defaultConfig);
    });

    it('timeline drag (seekToTime) should update decoder position', async () => {
      // Simulate timeline drag by calling seekToTime
      await decoder.seekToTime(30.0);

      const pos = decoder.getPosition();
      expect(pos.timestamp).toBe(30.0);
      expect(pos.frame).toBe(30 * 24); // 720 frames
    });

    it('timeline click (seekToFrame) should update decoder position', async () => {
      // Simulate clicking a specific frame on timeline
      await decoder.seekToFrame(100);

      const pos = decoder.getPosition();
      expect(pos.frame).toBe(100);
    });

    it('decoder position should reflect in videoElement for UI binding', async () => {
      // When decoder seeks, videoElement.currentTime must update
      // so that bind:currentTime in Svelte reflects the change
      mockVideo.currentTime = 0;

      await decoder.seekToTime(15.0);

      // UI would read this via bind:currentTime={time}
      expect(mockVideo.currentTime).toBe(15.0);
    });

    it('external videoElement.currentTime change should reflect in getPosition', async () => {
      // When video plays or user drags native controls,
      // videoElement.currentTime changes directly
      mockVideo.currentTime = 42.5;

      // Decoder should report this position (Html5VideoDecoder reads from video.currentTime)
      const pos = decoder.getPosition();
      expect(pos.timestamp).toBe(42.5);
      expect(pos.frame).toBe(Math.floor(42.5 * 24)); // 1020 frames
    });
  });

  describe('delegation to active decoder', () => {
    beforeEach(async () => {
      await decoder.init(defaultConfig);
    });

    it('should delegate seekToTime to Html5VideoDecoder when Mediabunny disabled', async () => {
      const pos = await decoder.seekToTime(10.0);

      // Html5VideoDecoder sets video.currentTime directly
      expect(mockVideo.currentTime).toBe(10.0);
      expect(pos.timestamp).toBe(10.0);
    });

    it('should delegate seekToFrame to Html5VideoDecoder when Mediabunny disabled', async () => {
      const pos = await decoder.seekToFrame(72); // 3 seconds at 24fps

      // Html5VideoDecoder seeks to middle of frame
      const expectedTime = 72.5 / 24;
      expect(mockVideo.currentTime).toBeCloseTo(expectedTime, 5);
      expect(pos.frame).toBe(72);
    });

    it('should delegate getPosition to active decoder', () => {
      mockVideo.currentTime = 5.0;

      const pos = decoder.getPosition();

      expect(pos.timestamp).toBe(5.0);
      expect(pos.frame).toBe(5 * 24);
      expect(pos.timecode).toBe('00:00:05:00');
    });
  });

  describe('frameRate property', () => {
    it('should return frameRate from active decoder', async () => {
      await decoder.init({ frameRate: 30, duration: 60 });

      expect(decoder.frameRate).toBe(30);
    });

    it('should default to 24fps before init', () => {
      const uninitDecoder = new HybridVideoDecoder({
        videoElement: mockVideo,
        videoSource: 'test.mp4',
        container: mockContainer,
        frameRate: 24,
        duration: 120,
        enableMediabunny: false,
      });

      expect(uninitDecoder.frameRate).toBe(24);
    });
  });
});

describe('HybridVideoDecoder with mocked MediabunnyDecoder', () => {
  /**
   * These tests verify that when MediabunnyDecoder is active,
   * HybridVideoDecoder still syncs videoElement.currentTime.
   *
   * We mock the internal decoder to simulate MediabunnyDecoder behavior
   * since actual WebCodecs aren't available in jsdom.
   */

  let mockVideo: HTMLVideoElement;
  let mockContainer: HTMLElement;

  beforeEach(() => {
    vi.clearAllMocks();
    mockVideo = createMockVideoElement();
    mockContainer = createMockContainer();
  });

  it('should sync videoElement.currentTime even when delegate does not', async () => {
    // Create a mock decoder that doesn't update videoElement
    const mockMediabunnyDecoder: IVideoDecoder = {
      frameRate: 24,
      init: vi.fn().mockResolvedValue(true),
      activate: vi.fn(),
      deactivate: vi.fn(),
      dispose: vi.fn(),
      stepFrame: vi.fn().mockResolvedValue({ timestamp: 1.0, frame: 24, timecode: '00:00:01:00' }),
      seekToTime: vi.fn().mockImplementation((seconds: number) => {
        // Simulate MediabunnyDecoder: returns position but doesn't touch videoElement
        const frame = Math.floor(seconds * 24);
        return Promise.resolve({
          timestamp: seconds,
          frame,
          timecode: `00:00:${String(Math.floor(seconds)).padStart(2, '0')}:${String(frame % 24).padStart(2, '0')}`,
        });
      }),
      seekToFrame: vi.fn().mockImplementation((frame: number) => {
        const timestamp = frame / 24;
        return Promise.resolve({
          timestamp,
          frame,
          timecode: '00:00:00:00',
        });
      }),
      getPosition: vi.fn().mockReturnValue({ timestamp: 0, frame: 0, timecode: '00:00:00:00' }),
      captureFrame: vi.fn(),
    };

    // Create HybridVideoDecoder and inject mock as active decoder
    const decoder = new HybridVideoDecoder({
      videoElement: mockVideo,
      videoSource: 'test.mp4',
      container: mockContainer,
      frameRate: 24,
      duration: 120,
      enableMediabunny: false,
    });

    await decoder.init({ frameRate: 24, duration: 120 });

    // Manually set the active decoder to our mock (simulating Mediabunny being active)
    // This is a bit of a hack but necessary to test the orchestration logic
    (decoder as any).active = mockMediabunnyDecoder;

    // Verify videoElement starts at 0
    expect(mockVideo.currentTime).toBe(0);

    // Seek via HybridVideoDecoder
    const pos = await decoder.seekToTime(7.5);

    // The mock decoder was called
    expect(mockMediabunnyDecoder.seekToTime).toHaveBeenCalledWith(7.5);

    // HybridVideoDecoder should have synced videoElement.currentTime
    expect(mockVideo.currentTime).toBe(7.5);
    expect(pos.timestamp).toBe(7.5);
  });

  it('should sync videoElement.currentTime after seekToFrame with mock decoder', async () => {
    const mockMediabunnyDecoder: IVideoDecoder = {
      frameRate: 30,
      init: vi.fn().mockResolvedValue(true),
      activate: vi.fn(),
      deactivate: vi.fn(),
      dispose: vi.fn(),
      stepFrame: vi.fn(),
      seekToTime: vi.fn(),
      seekToFrame: vi.fn().mockImplementation((frame: number) => {
        // MediabunnyDecoder calculates timestamp from frame
        const timestamp = frame / 30;
        return Promise.resolve({ timestamp, frame, timecode: '00:00:00:00' });
      }),
      getPosition: vi.fn().mockReturnValue({ timestamp: 0, frame: 0, timecode: '00:00:00:00' }),
      captureFrame: vi.fn(),
    };

    const decoder = new HybridVideoDecoder({
      videoElement: mockVideo,
      videoSource: 'test.mp4',
      container: mockContainer,
      frameRate: 30,
      duration: 120,
      enableMediabunny: false,
    });

    await decoder.init({ frameRate: 30, duration: 120 });
    (decoder as any).active = mockMediabunnyDecoder;

    mockVideo.currentTime = 0;

    // Seek to frame 90 (3 seconds at 30fps)
    const pos = await decoder.seekToFrame(90);

    expect(mockMediabunnyDecoder.seekToFrame).toHaveBeenCalledWith(90);
    expect(pos.frame).toBe(90);
    expect(pos.timestamp).toBe(3.0);

    // videoElement should be synced
    expect(mockVideo.currentTime).toBe(3.0);
  });
});

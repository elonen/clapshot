/**
 * Reproduces the "player gets stuck after scrubbing" bug:
 *
 * handleMove() (progress bar scrub) awaits videoDecoder.seekToTime() and only
 * afterwards writes `time` and `paused = true`. When the seek resolves late
 * (e.g. Mediabunny decode queue), those stale writes land AFTER the user has
 * restarted playback. The `paused = true` write makes Svelte's bind:paused
 * effect call media.pause(), killing the new playback with
 * "AbortError: The play() request was interrupted by a call to pause()".
 *
 * The decoder is mocked with manually-resolvable seek promises so the race
 * is deterministic.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import VideoPlayer from '@/lib/player_view/VideoPlayer.svelte';

// Manually resolvable pending seeks, one per seekToTime() call
let pendingSeeks: Array<(pos: { timestamp: number, frame: number, timecode: string }) => void> = [];

vi.mock('@/lib/player_view/video-decoder/HybridVideoDecoder', () => ({
  HybridVideoDecoder: class {
    frameRate = 30;
    async init() { return true; }
    seekToTime = vi.fn(() => new Promise((resolve) => { pendingSeeks.push(resolve); }));
    seekToFrame = vi.fn();
    stepFrame = vi.fn();
    getPosition() { return { timestamp: 0, frame: 0, timecode: '00:00:00:00' }; }
    prepareForPlayback = vi.fn();
    activate() {}
    deactivate() {}
    dispose() {}
    captureFrame() {}
  }
}));

vi.mock('@tadashi/svelte-notification', () => ({ acts: { add: vi.fn() } }));

vi.mock('simple-drawing-board', () => ({
  create: vi.fn(() => ({
    setLineSize: vi.fn(), setLineColor: vi.fn(), clear: vi.fn(),
    undo: vi.fn(), redo: vi.fn(), destroy: vi.fn(),
    fillImageByDataURL: vi.fn().mockResolvedValue(undefined)
  }))
}));

vi.mock('@/cookies', () => ({ default: { get: vi.fn(() => '100'), set: vi.fn() } }));

vi.mock('@/stores', () => {
  const store = (value: any) => ({
    subscribe: (cb: Function) => { cb(value); return () => {}; },
    set: vi.fn(), update: vi.fn()
  });
  return {
    videoIsReady: store(false),
    curVideo: store({ id: 'video-123', duration: { fps: '30' }, mediaType: 'video/mp4', subtitles: [] }),
    curSubtitle: store(null),
    allComments: store([]),
    collabId: store(null),
    clientConfig: store({ enable_mediabunny: true }),
  };
});

// Keep the component's loop-monitor interval and rAF loop from leaking
global.requestAnimationFrame = vi.fn((cb: any) => setTimeout(cb, 16) as any);
global.cancelAnimationFrame = vi.fn();
global.setInterval = vi.fn((cb: any) => setTimeout(cb, 500) as any) as any;
global.clearInterval = vi.fn();

/** Emulate real HTMLMediaElement play/pause semantics (happy-dom lacks them) */
function stubMediaElement(video: HTMLVideoElement) {
  let mediaPaused = true;
  Object.defineProperty(video, 'paused', { get: () => mediaPaused, configurable: true });
  Object.defineProperty(video, 'videoWidth', { get: () => 1920, configurable: true });
  Object.defineProperty(video, 'videoHeight', { get: () => 1080, configurable: true });
  video.play = vi.fn(() => {
    mediaPaused = false;
    video.dispatchEvent(new Event('play'));
    return Promise.resolve();
  });
  video.pause = vi.fn(() => {
    if (!mediaPaused) {
      mediaPaused = true;
      video.dispatchEvent(new Event('pause'));
    }
  });
  video.load = vi.fn();
  video.focus = vi.fn();
  return { isMediaPaused: () => mediaPaused };
}

async function flushAsync() {
  await tick();
  await new Promise((r) => setTimeout(r, 0));
  await tick();
}

describe('VideoPlayer scrub/play race', () => {
  beforeEach(() => { pendingSeeks = []; });
  afterEach(() => { cleanup(); vi.clearAllMocks(); });

  it('playback survives a slow scrub-seek that resolves after play() was requested', async () => {
    const { component } = render(VideoPlayer, { props: { src: 'test-video.mp4' } });
    const video = document.querySelector('video') as HTMLVideoElement;
    const media = stubMediaElement(video);

    // Triggers prepare_drawing() -> creates the (mocked) HybridVideoDecoder
    await fireEvent(video, new Event('loadedmetadata'));
    await flushAsync();

    // 1. Start playback
    component.setPlayback(true, 'test');
    await flushAsync();
    expect(media.isMediaPaused()).toBe(false);

    // 2. Scrub the progress bar while playing: pauses the video and starts
    //    a decoder seek that stays pending (slow decode)
    const progress = document.querySelector('progress') as HTMLProgressElement;
    progress.getBoundingClientRect = () => ({ left: 0, right: 100 } as DOMRect);
    await fireEvent.mouseMove(progress, { buttons: 1, clientX: 50 });
    await flushAsync();
    expect(media.isMediaPaused()).toBe(true);
    expect(pendingSeeks.length).toBe(1);

    // 3. User presses play again while the scrub-seek is still in flight
    component.setPlayback(true, 'test');
    await flushAsync();
    expect(media.isMediaPaused()).toBe(false);

    // 4. The stale scrub-seek finally resolves. Its continuation must NOT
    //    kill the playback the user started in the meantime.
    pendingSeeks.shift()!({ timestamp: 60, frame: 1800, timecode: '00:01:00:00' });
    await flushAsync();

    expect(media.isMediaPaused()).toBe(false);   // BUG: stale `paused = true` pauses via bind:paused
    expect(component.isPaused()).toBe(false);
  });
});

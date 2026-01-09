/**
 * Mediabunny video decoder - IVideoDecoder implementation using WebCodecs.
 *
 * - Provides frame-accurate seeking via Mediabunny's VideoSampleSink.
 * - Manages its own canvas for rendering frames.
 * - Falls back gracefully if WebCodecs is unavailable or codec unsupported.
 * - No audio support (use HTML5 video element for audio playback).
 *
 * Known issue: Frames may appear slightly darker than HTML5 video due to
 * WebCodecs not applying color space conversion (BT.709 -> sRGB).
 * See: https://issues.chromium.org/issues/40061457
 */

import type { IVideoDecoder, FramePosition, VideoDecoderConfig } from './types';
import { TimecodeUtils } from './timecode';
import {
  Input, ALL_FORMATS, UrlSource, BlobSource,
  VideoSampleSink, type VideoSample
} from 'mediabunny';

export class MediabunnyDecoder implements IVideoDecoder {
  private videoSource: string | Blob;
  private videoElement: HTMLVideoElement;
  private container: HTMLElement;
  private onclick?: (event: MouseEvent) => void;

  private input: Input | null = null;
  private sink: VideoSampleSink | null = null;
  private currentSample: VideoSample | null = null;
  private config: VideoDecoderConfig | null = null;

  private frameCanvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private resizeObserver: ResizeObserver | null = null;

  // Serializes async seek operations to prevent sample leaks
  private seekQueue: Promise<void> = Promise.resolve();

  constructor(
    videoSource: string | Blob,
    videoElement: HTMLVideoElement,
    container: HTMLElement,
    onclick?: (event: MouseEvent) => void
  ) {
    this.videoSource = videoSource;
    this.videoElement = videoElement;
    this.container = container;
    this.onclick = onclick;
  }

  async init(config: VideoDecoderConfig): Promise<boolean> {
    this.config = config;

    try {
      const source = typeof this.videoSource === 'string'
        ? new UrlSource(this.videoSource)
        : new BlobSource(this.videoSource);

      this.input = new Input({ source, formats: ALL_FORMATS });

      const videoTrack = await this.input.getPrimaryVideoTrack();
      if (!videoTrack) {
        console.warn('MediabunnyDecoder: No video track found');
        return false;
      }

      if (!(await videoTrack.canDecode())) {
        console.warn('MediabunnyDecoder: Codec not supported by WebCodecs');
        return false;
      }

      // Get actual values from file (may differ from HTML5 video metadata)
      const stats = await videoTrack.computePacketStats(50);
      this.config.frameRate = stats.averagePacketRate;
      this.config.firstTimestamp = await videoTrack.getFirstTimestamp();

      this.sink = new VideoSampleSink(videoTrack);

      // Create canvas using HTML5 video element dimensions (correctly handles PAR/SAR)
      this.createCanvas(this.videoElement.videoWidth, this.videoElement.videoHeight);

      return true;
    } catch (err) {
      console.error('MediabunnyDecoder init failed:', err);
      return false;
    }
  }

  private createCanvas(width: number, height: number): void {
    this.frameCanvas = document.createElement('canvas');
    this.frameCanvas.width = width;
    this.frameCanvas.height = height;
    this.frameCanvas.classList.add('absolute', 'z-[50]');
    this.frameCanvas.style.cssText =
      'display: none; left: 50%; top: 50%; transform: translate(-50%, -50%);';
    this.container.appendChild(this.frameCanvas);
    this.ctx = this.frameCanvas.getContext('2d');

    if (this.onclick) {
      this.frameCanvas.addEventListener('click', this.onclick);
    }

    this.resizeObserver = new ResizeObserver(() => this.updateCanvasSize());
    this.resizeObserver.observe(this.container);
  }

  private updateCanvasSize(): void {
    if (!this.frameCanvas) return;

    // Use canvas intrinsic dimensions for aspect ratio (set from videoTrack.displayWidth/Height)
    const videoAspect = this.frameCanvas.width / this.frameCanvas.height;

    // Get container dimensions
    const containerRect = this.container.getBoundingClientRect();
    const containerAspect = containerRect.width / containerRect.height;

    let displayWidth: number, displayHeight: number;
    if (containerAspect > videoAspect) {
      // Container is wider than video - height limited
      displayHeight = containerRect.height;
      displayWidth = displayHeight * videoAspect;
    } else {
      // Container is taller than video - width limited
      displayWidth = containerRect.width;
      displayHeight = displayWidth / videoAspect;
    }

    this.frameCanvas.style.width = `${displayWidth}px`;
    this.frameCanvas.style.height = `${displayHeight}px`;
  }

  get frameRate(): number {
    return this.config?.frameRate ?? 24;
  }

  activate(): void {
    if (!this.frameCanvas) return;

    this.updateCanvasSize();
    this.frameCanvas.style.display = 'block';
    this.videoElement.style.visibility = 'hidden';

    if (this.currentSample && this.ctx && this.frameCanvas) {
      // Draw to fill canvas (canvas is sized to videoElement dimensions)
      this.currentSample.draw(this.ctx, 0, 0, this.frameCanvas.width, this.frameCanvas.height);
    }
  }

  deactivate(): void {
    this.currentSample?.close();
    this.currentSample = null;

    if (this.frameCanvas) {
      this.frameCanvas.style.display = 'none';
    }
    this.videoElement.style.visibility = 'visible';
  }

  dispose(): void {
    this.currentSample?.close();
    this.input?.dispose();
    this.resizeObserver?.disconnect();
    this.frameCanvas?.remove();

    this.input = null;
    this.sink = null;
    this.currentSample = null;
    this.frameCanvas = null;
    this.ctx = null;
    this.resizeObserver = null;
  }

  async stepFrame(direction: 1 | -1, count = 1): Promise<FramePosition> {
    if (!this.currentSample || !this.config) {
      throw new Error('MediabunnyDecoder: No current position (call seekToTime first)');
    }

    // Use sample's actual duration for VFR support
    const frameDuration = this.currentSample.duration || (1 / this.config.frameRate);
    const firstTs = this.config.firstTimestamp ?? 0;

    let newTime = this.currentSample.timestamp + (direction * count * frameDuration);
    newTime = Math.max(firstTs, newTime);

    return this.seekToTime(newTime);
  }

  async seekToTime(seconds: number): Promise<FramePosition> {
    if (!this.sink) throw new Error('MediabunnyDecoder not initialized');

    // Serialize seeks to prevent race conditions that leak samples
    const seekOp = this.seekQueue.then(async () => {
      // Get new sample first
      const newSample = await this.sink!.getSample(seconds);

      // Close old sample only after new one is ready
      this.currentSample?.close();
      this.currentSample = newSample;

      if (this.currentSample && this.ctx && this.frameCanvas) {
        this.currentSample.draw(this.ctx, 0, 0, this.frameCanvas.width, this.frameCanvas.height);
      }
    });

    // Chain for next operation, but don't propagate errors to queue
    this.seekQueue = seekOp.catch(() => {});

    await seekOp;
    return this.getPosition();
  }

  async seekToFrame(frame: number): Promise<FramePosition> {
    if (!this.config) throw new Error('MediabunnyDecoder not initialized');

    const firstTs = this.config.firstTimestamp ?? 0;
    const timestamp = firstTs + (frame / this.config.frameRate);
    return this.seekToTime(timestamp);
  }

  getPosition(): FramePosition {
    if (!this.currentSample || !this.config) {
      return { timestamp: 0, frame: 0, timecode: '00:00:00:00' };
    }

    const timestamp = this.currentSample.timestamp;
    const firstTs = this.config.firstTimestamp ?? 0;
    const frame = Math.round((timestamp - firstTs) * this.config.frameRate);
    const timecode = TimecodeUtils.frameToSMPTE(frame, this.config.frameRate);

    return { timestamp, frame, timecode };
  }

  captureFrame(ctx: CanvasRenderingContext2D): void {
    if (this.currentSample) {
      const canvas = ctx.canvas;
      this.currentSample.draw(ctx, 0, 0, canvas.width, canvas.height);
    }
  }
}

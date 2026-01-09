/**
 * HTML5 video decoder - IVideoDecoder implementation using HTMLVideoElement.
 *
 * Provides frame-accurate seeking via currentTime manipulation.
 * Replaces the old VideoFrame.js library with cleaner, self-contained logic.
 */

import type { IVideoDecoder, FramePosition, VideoDecoderConfig } from './types';
import { TimecodeUtils } from './timecode';

/**
 * HTML5 video-based decoder.
 * Uses HTMLVideoElement.currentTime for seeking.
 */
export class Html5VideoDecoder implements IVideoDecoder {
  private video: HTMLVideoElement;
  private config: VideoDecoderConfig | null = null;

  constructor(video: HTMLVideoElement) {
    this.video = video;
  }

  get frameRate(): number {
    return this.config?.frameRate ?? 24;
  }

  async init(config: VideoDecoderConfig): Promise<boolean> {
    this.config = config;
    return true;
  }

  async stepFrame(direction: 1 | -1, count = 1): Promise<FramePosition> {
    if (!this.config) throw new Error('Html5VideoDecoder not initialized');

    // Pause video if playing (frame stepping requires paused state)
    if (!this.video.paused) {
      this.video.pause();
    }

    const currentFrame = this.getCurrentFrame();
    const targetFrame = Math.max(0, currentFrame + direction * count);
    // Seek to middle of target frame for tolerance against browser seek imprecision
    const targetTime = (targetFrame + 0.5) / this.config.frameRate;

    this.video.currentTime = targetTime;
    return this.getPosition();
  }

  async seekToTime(seconds: number): Promise<FramePosition> {
    this.video.currentTime = Math.max(0, seconds);
    return this.getPosition();
  }

  async seekToFrame(frame: number): Promise<FramePosition> {
    if (!this.config) throw new Error('Html5VideoDecoder not initialized');

    // Seek to middle of target frame for tolerance against browser seek imprecision
    const targetTime = (Math.max(0, frame) + 0.5) / this.config.frameRate;
    this.video.currentTime = targetTime;
    return this.getPosition();
  }

  getPosition(): FramePosition {
    if (!this.config) {
      return { timestamp: 0, frame: 0, timecode: '00:00:00:00' };
    }

    const timestamp = this.video.currentTime;
    const frame = this.getCurrentFrame();
    const timecode = TimecodeUtils.frameToSMPTE(frame, this.config.frameRate);

    return { timestamp, frame, timecode };
  }

  activate(): void {
    this.video.style.visibility = 'visible';
  }

  deactivate(): void {
    // No resources to free for HTML5 video
  }

  dispose(): void {
    this.config = null;
  }

  captureFrame(ctx: CanvasRenderingContext2D): void {
    ctx.drawImage(this.video, 0, 0);
  }

  /** Get current frame number (used internally) */
  private getCurrentFrame(): number {
    if (!this.config) return 0;
    return TimecodeUtils.timeToFrame(this.video.currentTime, this.config.frameRate);
  }
}

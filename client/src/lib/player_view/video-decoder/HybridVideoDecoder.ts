/**
 * Hybrid video decoder - orchestrates HTML5 and Mediabunny decoders.
 *
 * Auto-switches to Mediabunny for frame-accurate stepping (if available),
 * back to HTML5 for (color-correct, performant) playback with audio.
 */

import type { IVideoDecoder, FramePosition, VideoDecoderConfig } from './types';
import { Html5VideoDecoder } from './Html5VideoDecoder';
import { MediabunnyDecoder } from './MediabunnyDecoder';

export interface HybridVideoDecoderConfig extends VideoDecoderConfig {
  videoElement: HTMLVideoElement;
  videoSource: string | Blob;
  container: HTMLElement;
  onclick?: (event: MouseEvent) => void;
  enableMediabunny?: boolean;
}

export class HybridVideoDecoder implements IVideoDecoder {
  private html5: Html5VideoDecoder;
  private mediabunny: MediabunnyDecoder | null = null;
  private active: IVideoDecoder;

  private videoElement: HTMLVideoElement;
  private videoSource: string | Blob;
  private container: HTMLElement;
  private onclick?: (event: MouseEvent) => void;
  private config: HybridVideoDecoderConfig | null = null;

  private mediabunnyAvailable: boolean;
  private mediabunnyInitPromise: Promise<boolean> | null = null;

  constructor(config: HybridVideoDecoderConfig) {
    this.videoElement = config.videoElement;
    this.videoSource = config.videoSource;
    this.container = config.container;
    this.onclick = config.onclick;

    this.html5 = new Html5VideoDecoder(config.videoElement);
    this.active = this.html5;

    // Check both WebCodecs support AND config setting (defaults to true if not specified)
    const enableMediabunny = config.enableMediabunny !== false;
    this.mediabunnyAvailable = typeof VideoDecoder !== 'undefined' && enableMediabunny;
  }

  async init(config: VideoDecoderConfig): Promise<boolean> {
    this.config = { ...this.config!, ...config };
    await this.html5.init(config);
    this.html5.activate();
    return true;
  }

  get frameRate(): number {
    return this.active.frameRate;
  }

  // --- Mode switching ---

  private async ensureMediabunny(): Promise<boolean> {
    if (!this.mediabunnyAvailable || !this.config) return false;

    // If init is already in progress, wait for it
    if (this.mediabunnyInitPromise) {
      return this.mediabunnyInitPromise;
    }

    if (!this.mediabunny) {
      this.mediabunny = new MediabunnyDecoder(
        this.videoSource,
        this.videoElement,
        this.container,
        this.onclick
      );

      // Track the init promise to prevent race conditions
      this.mediabunnyInitPromise = this.mediabunny.init(this.config).then(success => {
        if (!success) {
          this.mediabunnyAvailable = false;
          this.mediabunny = null;
        }
        this.mediabunnyInitPromise = null;
        return success;
      });

      return this.mediabunnyInitPromise;
    }
    return true;
  }

  private async switchToMediabunny(): Promise<boolean> {
    if (this.active === this.mediabunny) return true;

    if (!await this.ensureMediabunny()) return false;

    // Sync position
    const currentTime = this.videoElement.currentTime;
    await this.mediabunny!.seekToTime(currentTime);

    this.html5.deactivate();
    this.mediabunny!.activate();
    this.active = this.mediabunny!;
    return true;
  }

  private switchToHtml5(): void {
    if (this.active === this.html5) return;

    if (this.mediabunny) {
      const pos = this.mediabunny.getPosition();
      this.videoElement.currentTime = pos.timestamp;
      this.mediabunny.deactivate();
    }

    this.html5.activate();
    this.active = this.html5;
  }

  // --- IVideoDecoder implementation ---

  activate(): void {
    this.active.activate();
  }

  deactivate(): void {
    this.active.deactivate();
  }

  dispose(): void {
    this.html5.dispose();
    this.mediabunny?.dispose();
  }

  async stepFrame(direction: 1 | -1, count = 1): Promise<FramePosition> {
    // Auto-switch to Mediabunny for frame stepping
    if (this.mediabunnyAvailable && this.active === this.html5) {
      await this.switchToMediabunny();
    }

    try {
      return await this.active.stepFrame(direction, count);
    } catch (err) {
      // Fallback on error
      if (this.active === this.mediabunny) {
        console.warn('Mediabunny step failed, falling back:', err);
        this.switchToHtml5();
        return await this.html5.stepFrame(direction, count);
      }
      throw err;
    }
  }

  async seekToTime(seconds: number): Promise<FramePosition> {
    const pos = await this.active.seekToTime(seconds);
    // Sync video element for UI bindings (e.g., progress bar, bind:currentTime)
    // This ensures timeline UI stays in sync regardless of which decoder is active
    this.videoElement.currentTime = pos.timestamp;
    return pos;
  }

  async seekToFrame(frame: number): Promise<FramePosition> {
    const pos = await this.active.seekToFrame(frame);
    // Sync video element for UI bindings
    this.videoElement.currentTime = pos.timestamp;
    return pos;
  }

  getPosition(): FramePosition {
    return this.active.getPosition();
  }

  captureFrame(ctx: CanvasRenderingContext2D): void {
    this.active.captureFrame(ctx);
  }

  // --- Public API for VideoPlayer ---

  /** Call before video.play() to switch back to HTML5 */
  prepareForPlayback(): void {
    this.switchToHtml5();
  }
}

/**
 * Video decoder abstraction layer for
 * - playback with accurate color and audio (HTML5 video element)
 * - frame-accurate seeking (Mediabunny/WebCodecs)
 *
 * Each implementation manages its own display (video element or canvas).
 * HybridVideoDecoder orchestrates switching between them.
 */

export interface FramePosition {
  /** Timestamp in seconds (matches video.currentTime) */
  timestamp: number;
  /** Frame number (0-indexed) */
  frame: number;
  /** SMPTE timecode string (HH:MM:SS:FF) */
  timecode: string;
}

export interface VideoDecoderConfig {
  /** Video frame rate (e.g., 24, 25, 30, 60) */
  frameRate: number;
  /** Video duration in seconds */
  duration: number;
  /** First timestamp - may not be 0 for trimmed content */
  firstTimestamp?: number;
}

/**
 * Interface for frame-accurate seeking implementations.
 * Each implementation manages its own display (video element or canvas).
 *
 * For timecode conversions, use TimecodeUtils directly with decoder.frameRate.
 */
export interface IVideoDecoder {
  readonly frameRate: number;

  init(config: VideoDecoderConfig): Promise<boolean>;

  /** Take control of display (show canvas/video as appropriate) */
  activate(): void;

  /** Release display control, free resources */
  deactivate(): void;

  /** Full cleanup on unmount */
  dispose(): void;

  stepFrame(direction: 1 | -1, count?: number): Promise<FramePosition>;
  seekToTime(seconds: number): Promise<FramePosition>;
  seekToFrame(frame: number): Promise<FramePosition>;
  getPosition(): FramePosition;

  /** Draw current frame to provided context (for screenshot/drawing capture) */
  captureFrame(ctx: CanvasRenderingContext2D): void;
}

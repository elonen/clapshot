/**
 * SMPTE timecode utilities.
 * All functions use consistent Math.floor for frame calculations.
 */
export const TimecodeUtils = {
  /**
   * Convert frame number to SMPTE timecode string (HH:MM:SS:FF).
   */
  frameToSMPTE(frame: number, frameRate: number): string {
    const totalFrames = Math.floor(frame);
    const framesPerMinute = frameRate * 60;
    const framesPerHour = framesPerMinute * 60;

    const hours = Math.floor(totalFrames / framesPerHour);
    const minutes = Math.floor((totalFrames % framesPerHour) / framesPerMinute);
    const seconds = Math.floor((totalFrames % framesPerMinute) / frameRate);
    const frames = Math.floor(totalFrames % frameRate);

    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${pad(hours)}:${pad(minutes)}:${pad(seconds)}:${pad(frames)}`;
  },

  /**
   * Convert timestamp (seconds) to SMPTE timecode string.
   */
  timeToSMPTE(seconds: number, frameRate: number): string {
    const frame = Math.floor(seconds * frameRate);
    return TimecodeUtils.frameToSMPTE(frame, frameRate);
  },

  /**
   * Parse SMPTE timecode string to frame number.
   * Accepts HH:MM:SS:FF or HH:MM:SS format.
   */
  smpteToFrame(smpte: string, frameRate: number): number {
    const parts = smpte.split(':').map(Number);
    if (parts.length < 3 || parts.some(isNaN)) {
      throw new Error(`Invalid SMPTE timecode: ${smpte}`);
    }

    const [hours, minutes, seconds, frames = 0] = parts;
    return Math.floor(
      hours * 60 * 60 * frameRate +
      minutes * 60 * frameRate +
      seconds * frameRate +
      frames
    );
  },

  /**
   * Convert SMPTE timecode to timestamp in seconds.
   */
  smpteToTime(smpte: string, frameRate: number): number {
    const frame = TimecodeUtils.smpteToFrame(smpte, frameRate);
    return frame / frameRate;
  },

  /**
   * Convert SMPTE timecode to milliseconds.
   */
  smpteToMilliseconds(smpte: string, frameRate: number): number {
    return TimecodeUtils.smpteToTime(smpte, frameRate) * 1000;
  },

  /**
   * Convert timestamp to frame number.
   */
  timeToFrame(seconds: number, frameRate: number): number {
    return Math.floor(seconds * frameRate);
  },

  /**
   * Convert frame number to timestamp in seconds.
   */
  frameToTime(frame: number, frameRate: number): number {
    return frame / frameRate;
  },
};

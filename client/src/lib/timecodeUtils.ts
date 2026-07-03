/**
 * Converts timecode "HH:MM:SS.mmm" or "HH:MM:SS:FF" to seconds.
 */
export function timecodeToSeconds(tc: string): number {
    if (!tc) return 0;

    // Parse HH:MM:SS.mmm format
    const match = tc.match(/^(\d{2}):(\d{2}):(\d{2})\.(\d{3})$/);
    if (match) {
        const [, hh, mm, ss, ms] = match.map(Number);
        return hh * 3600 + mm * 60 + ss + ms / 1000;
    }

    // Parse HH:MM:SS:FF format
    const edlMatch = tc.match(/^(\d{2}):(\d{2}):(\d{2}):(\d{2})$/);
    if (edlMatch) {
        const [, hh, mm, ss, ff] = edlMatch.map(Number);
        // Assume 25fps
        return hh * 3600 + mm * 60 + ss + ff / 25;
    }

    return 0;
}

/**
 * Like timecodeToSeconds but returns null for empty/undefined/unparseable timecodes.
 * Returns 0 only for genuine "00:00:00.000" timecodes.
 */
export function timecodeToSecondsOrNull(tc: string | undefined): number | null {
    if (!tc) return null;
    const s = timecodeToSeconds(tc);
    if (s === 0 && !tc.startsWith('00:00:00')) return null;
    return s;
}

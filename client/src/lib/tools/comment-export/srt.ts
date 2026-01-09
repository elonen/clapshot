import type { CommentExporter, ExportContext } from './types';

/**
 * Converts timecode "HH:MM:SS.mmm" to SRT format "HH:MM:SS,mmm"
 */
function timecodeToSRT(tc: string): string {
    if (!tc) return "00:00:00,000";

    // Parse HH:MM:SS.mmm format
    const match = tc.match(/^(\d{2}):(\d{2}):(\d{2})\.(\d{3})$/);
    if (match) {
        const [, hh, mm, ss, ms] = match;
        return `${hh}:${mm}:${ss},${ms}`;
    }

    // Parse HH:MM:SS:FF format (EDL style) - convert to approximate milliseconds
    const edlMatch = tc.match(/^(\d{2}):(\d{2}):(\d{2}):(\d{2})$/);
    if (edlMatch) {
        const [, hh, mm, ss, ff] = edlMatch;
        // Assume 25fps for frame-to-ms conversion
        const ms = Math.round((parseInt(ff) / 25) * 1000).toString().padStart(3, '0');
        return `${hh}:${mm}:${ss},${ms}`;
    }

    return "00:00:00,000";
}

/**
 * Adds seconds to an SRT timecode.
 */
function addSeconds(srtTc: string, seconds: number): string {
    const match = srtTc.match(/^(\d{2}):(\d{2}):(\d{2}),(\d{3})$/);
    if (!match) return srtTc;

    let hh = parseInt(match[1]);
    let mm = parseInt(match[2]);
    let ss = parseInt(match[3]) + seconds;
    const ms = match[4];

    while (ss >= 60) {
        ss -= 60;
        mm++;
    }
    while (mm >= 60) {
        mm -= 60;
        hh++;
    }

    return `${hh.toString().padStart(2, '0')}:${mm.toString().padStart(2, '0')}:${ss.toString().padStart(2, '0')},${ms}`;
}

export const srtExporter: CommentExporter = {
    id: 'srt',
    name: 'SRT Subtitles',
    extension: '.srt',
    options: [
        {
            id: 'durationSeconds',
            label: 'Duration per comment (seconds)',
            type: 'number',
            default: 3,
        },
    ],
    export(ctx: ExportContext, opts: Record<string, unknown>): string {
        const duration = (opts.durationSeconds as number) || 3;

        const lines: string[] = [];

        ctx.comments.forEach((c, i) => {
            const startTc = timecodeToSRT(c.timecode);
            const endTc = addSeconds(startTc, duration);

            let text = c.text;
            if (c.hasDrawing && !text.includes('[has drawing]')) {
                text += ' [has drawing]';
            }

            lines.push(`${i + 1}`);
            lines.push(`${startTc} --> ${endTc}`);
            lines.push(text);
            lines.push('');
        });

        return lines.join('\n');
    }
};

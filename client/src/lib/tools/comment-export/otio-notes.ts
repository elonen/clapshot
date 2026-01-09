import type { CommentExporter, ExportContext } from './types';

/**
 * Converts timecode "HH:MM:SS.mmm" to seconds.
 */
function timecodeToSeconds(tc: string): number {
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

export const otioNotesExporter: CommentExporter = {
    id: 'otio-notes',
    name: 'OpenTimelineIO Notes',
    extension: '.otrn',
    options: [],
    export(ctx: ExportContext): string {
        const notes = ctx.comments.map(c => {
            let text = c.text;
            if (c.hasDrawing && !text.includes('[has drawing]')) {
                text += ' [has drawing]';
            }

            return {
                comment: text,
                time: timecodeToSeconds(c.timecode),
            };
        });

        const output = {
            metadata: {},
            sequence: {
                notes,
            },
        };

        return JSON.stringify(output, null, 2);
    }
};

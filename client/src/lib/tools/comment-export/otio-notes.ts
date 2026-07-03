import type { CommentExporter, ExportContext } from './types';
import { timecodeToSeconds } from '@/lib/timecodeUtils';

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

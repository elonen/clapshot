import type { CommentExporter, ExportContext } from './types';

const RESOLVE_COLORS = [
    'Purple', 'Blue', 'Cyan', 'Green', 'Yellow', 'Red',
    'Pink', 'Rose', 'Lavender', 'Sky', 'Mint', 'Lemon', 'Sand', 'Cocoa', 'Cream'
];

/**
 * Converts timecode "HH:MM:SS.mmm" to EDL format "HH:MM:SS:FF"
 */
function timecodeToEDL(tc: string, fps: number): string {
    if (!tc) return "00:00:00:00";

    // Parse HH:MM:SS.mmm format
    const match = tc.match(/^(\d{2}):(\d{2}):(\d{2})\.(\d{3})$/);
    if (!match) {
        // Already in EDL format?
        if (tc.match(/^\d{2}:\d{2}:\d{2}:\d{2}$/)) return tc;
        return "00:00:00:00";
    }

    const [, hh, mm, ss, ms] = match;
    const frames = Math.round((parseInt(ms) / 1000) * fps);
    const ff = Math.min(frames, Math.floor(fps) - 1).toString().padStart(2, '0');

    return `${hh}:${mm}:${ss}:${ff}`;
}

/**
 * Adds frames to an EDL timecode, handling overflow.
 */
function addFrames(edlTc: string, framesToAdd: number, fps: number): string {
    const match = edlTc.match(/^(\d{2}):(\d{2}):(\d{2}):(\d{2})$/);
    if (!match) return edlTc;

    let hh = parseInt(match[1]);
    let mm = parseInt(match[2]);
    let ss = parseInt(match[3]);
    let ff = parseInt(match[4]) + framesToAdd;

    const maxFrames = Math.floor(fps);
    while (ff >= maxFrames) {
        ff -= maxFrames;
        ss++;
    }
    while (ss >= 60) {
        ss -= 60;
        mm++;
    }
    while (mm >= 60) {
        mm -= 60;
        hh++;
    }

    return `${hh.toString().padStart(2, '0')}:${mm.toString().padStart(2, '0')}:${ss.toString().padStart(2, '0')}:${ff.toString().padStart(2, '0')}`;
}

/**
 * Converts timecode to total frames for duration calculation.
 */
function timecodeToFrames(tc: string, fps: number): number {
    const match = tc.match(/^(\d{2}):(\d{2}):(\d{2}):(\d{2})$/);
    if (!match) return 0;
    const [, hh, mm, ss, ff] = match.map(Number);
    return hh * 3600 * fps + mm * 60 * fps + ss * fps + ff;
}

function formatDate(date: Date | undefined): string {
    if (!date) return "Unknown date";
    const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    const month = months[date.getMonth()];
    const day = date.getDate();
    let hours = date.getHours();
    const minutes = date.getMinutes().toString().padStart(2, '0');
    const ampm = hours >= 12 ? 'pm' : 'am';
    hours = hours % 12 || 12;
    return `${month} ${day} ${hours}:${minutes}${ampm}`;
}

export const resolveEdlExporter: CommentExporter = {
    id: 'resolve-edl',
    name: 'DaVinci Resolve EDL',
    extension: '.edl',
    options: [
        {
            id: 'markerColor',
            label: 'Marker color',
            type: 'select',
            default: 'Purple',
            choices: RESOLVE_COLORS.map(c => ({ value: c, label: c })),
        },
    ],
    export(ctx: ExportContext, opts: Record<string, unknown>): string {
        const fps = ctx.fps || 24;
        const color = (opts.markerColor as string) || 'Purple';

        let edl = `\uFEFFTITLE: ${ctx.title}\n`;
        edl += `FCM: NON DROP FRAME\n\n`;

        ctx.comments.forEach((c, i) => {
            const eventNum = String(i + 1).padStart(3, '0');
            const recordIn = timecodeToEDL(c.timecode, fps);
            // Calculate duration: default to ~1 second if no end time
            const durationFrames = Math.round(fps);
            const recordOut = addFrames(recordIn, durationFrames, fps);

            const username = c.username || 'Unknown';
            const dateStr = formatDate(c.created);
            const duration = timecodeToFrames(recordOut, fps) - timecodeToFrames(recordIn, fps);

            // Event line
            edl += `${eventNum}  001  C  V  ${recordIn}  ${recordOut}  ${recordIn}  ${recordOut}\n`;
            // Attribution line
            edl += `@${username}, ${dateStr}\n`;
            // Comment with metadata
            let text = c.text;
            if (c.hasDrawing && !text.includes('[has drawing]')) {
                text += ' [has drawing]';
            }
            edl += `${text} |C:ResolveColor${color} |M:${username} |D:${duration}\n\n`;
        });

        return edl;
    }
};

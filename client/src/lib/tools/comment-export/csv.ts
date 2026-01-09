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

/**
 * Formats timecode as HH:MM:SS for display.
 */
function formatTimecode(tc: string): string {
    if (!tc) return "00:00:00";

    const match = tc.match(/^(\d{2}):(\d{2}):(\d{2})/);
    if (match) {
        return `${match[1]}:${match[2]}:${match[3]}`;
    }

    return "00:00:00";
}

/**
 * Escapes a CSV field value.
 */
function escapeCsvField(value: string, delimiter: string): string {
    // If contains delimiter, quotes, or newlines, wrap in quotes and escape internal quotes
    if (value.includes(delimiter) || value.includes('"') || value.includes('\n') || value.includes('\r')) {
        return '"' + value.replace(/"/g, '""') + '"';
    }
    return value;
}

const DELIMITERS: Record<string, string> = {
    comma: ',',
    semicolon: ';',
    tab: '\t',
};

export const csvExporter: CommentExporter = {
    id: 'csv',
    name: 'CSV',
    extension: '.csv',
    options: [
        {
            id: 'includeHeaders',
            label: 'Include header row',
            type: 'boolean',
            default: true,
        },
        {
            id: 'delimiter',
            label: 'Delimiter',
            type: 'select',
            default: 'comma',
            choices: [
                { value: 'comma', label: 'Comma (,)' },
                { value: 'semicolon', label: 'Semicolon (;)' },
                { value: 'tab', label: 'Tab' },
            ],
        },
    ],
    export(ctx: ExportContext, opts: Record<string, unknown>): string {
        const includeHeaders = opts.includeHeaders !== false;
        const delimiterKey = (opts.delimiter as string) || 'comma';
        const delimiter = DELIMITERS[delimiterKey] || ',';

        const lines: string[] = [];

        if (includeHeaders) {
            const headers = ['Commenter', 'Comment', 'Timecode', 'Time (seconds)', 'Created At', 'Has Drawing'];
            lines.push(headers.map(h => escapeCsvField(h, delimiter)).join(delimiter));
        }

        for (const c of ctx.comments) {
            let text = c.text;
            if (c.hasDrawing && !text.includes('[has drawing]')) {
                // Don't append here since we have a separate column
            }

            const row = [
                c.username || '',
                text,
                formatTimecode(c.timecode),
                timecodeToSeconds(c.timecode).toFixed(2),
                c.created ? c.created.toISOString() : '',
                c.hasDrawing ? 'Yes' : 'No',
            ];

            lines.push(row.map(v => escapeCsvField(v, delimiter)).join(delimiter));
        }

        // Add BOM for Excel compatibility with UTF-8
        return '\uFEFF' + lines.join('\n');
    }
};

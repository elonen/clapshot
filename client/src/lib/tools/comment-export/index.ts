import type { CommentExporter, ExportedComment } from './types';
import type { IndentedComment } from '@/types';

import { resolveEdlExporter } from './resolve-edl';
import { premiereXmlExporter } from './premiere-xml';
import { srtExporter } from './srt';
import { csvExporter } from './csv';
import { otioNotesExporter } from './otio-notes';

export * from './types';

// Registry of all available exporters
export const exporters: CommentExporter[] = [
    resolveEdlExporter,
    premiereXmlExporter,
    srtExporter,
    csvExporter,
    otioNotesExporter,
];

export function getExporterById(id: string): CommentExporter | undefined {
    return exporters.find(e => e.id === id);
}

/**
 * Groups comments by merging replies into their parent comments.
 * - Replies are appended to parent text with " // " separator
 * - Reply drawings are marked with [has drawing] in the merged text
 * - Untimed comments get empty timecode (handled by exporters as time 0)
 * - Results are sorted by timecode
 */
export function groupComments(comments: IndentedComment[]): ExportedComment[] {
    const parentMap = new Map<string, ExportedComment>();
    const replyMap = new Map<string, string[]>(); // parentId -> reply texts

    // First pass: collect all comments and identify parents vs replies
    for (const ic of comments) {
        const c = ic.comment;
        if (!c.parentId) {
            // Top-level comment
            parentMap.set(c.id, {
                timecode: c.timecode || "",
                text: c.comment || "",
                hasDrawing: !!c.drawing,
                username: c.usernameIfnull || c.userId || undefined,
                created: c.created ? new Date(c.created) : undefined,
            });
        } else {
            // Reply - collect for later merging
            if (!replyMap.has(c.parentId)) {
                replyMap.set(c.parentId, []);
            }
            let replyText = c.comment || "";
            if (c.drawing) replyText += " [has drawing]";
            replyMap.get(c.parentId)!.push(replyText);
        }
    }

    // Second pass: merge replies into parents
    const result: ExportedComment[] = [];
    for (const [id, grouped] of parentMap) {
        const replies = replyMap.get(id) || [];
        let fullText = grouped.text;
        if (replies.length > 0) {
            fullText += " // " + replies.join(" // ");
        }
        result.push({
            ...grouped,
            text: fullText,
        });
    }

    // Sort by timecode (empty/untimed first, then by timecode string)
    result.sort((a, b) => {
        const tcA = a.timecode || "";
        const tcB = b.timecode || "";
        if (!tcA && tcB) return -1;
        if (tcA && !tcB) return 1;
        return tcA.localeCompare(tcB);
    });

    return result;
}

/**
 * Triggers browser download of a file with the given content.
 */
export function downloadFile(content: string, filename: string, mimeType: string = 'text/plain') {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
}

import { IndentedComment } from '@/types';
import { timecodeToSecondsOrNull } from '@/lib/timecodeUtils';

export type CommentSortMode = 'timecode' | 'date';

/**
 * Sort and nest a flat list of comments into an indented, ordered tree.
 *
 * @param sortMode - 'date': all roots by created ASC (original behavior).
 *   'timecode': non-timed roots first (by created ASC), then timed roots (by timecode ASC, tiebreak created ASC).
 *   Child comments are always sorted by created ASC regardless of mode.
 */
export function indentCommentTree(items: IndentedComment[], sortMode: CommentSortMode = 'date'): IndentedComment[] {
    let rootComments = items.filter(item => item.comment.parentId == null);

    if (sortMode === 'timecode') {
        rootComments.sort((a, b) => {
            const tcA = timecodeToSecondsOrNull(a.comment.timecode);
            const tcB = timecodeToSecondsOrNull(b.comment.timecode);

            // Non-timed (null) before timed
            if (tcA === null && tcB === null) {
                return (a.comment.created?.getTime() ?? 0) - (b.comment.created?.getTime() ?? 0);
            }
            if (tcA === null) return -1;
            if (tcB === null) return 1;

            // Both timed — sort by timecode, tiebreak by created
            if (tcA !== tcB) return tcA - tcB;
            return (a.comment.created?.getTime() ?? 0) - (b.comment.created?.getTime() ?? 0);
        });
    } else {
        rootComments.sort((a, b) => (a.comment.created?.getTime() ?? 0) - (b.comment.created?.getTime() ?? 0));
    }

    // Recursive DFS function to traverse and build the ordered list
    function dfs(c: IndentedComment, depth: number, result: IndentedComment[]): void {
        if (result.find((it) => it.comment.id === c.comment.id)) return;  // already added, cut infinite loop
        result.push({ ...c, indent: depth });
        let children = items.filter(item => (item.comment.parentId === c.comment.id));
        children.sort((a, b) => (a.comment.created?.getTime() ?? 0) - (b.comment.created?.getTime() ?? 0));
        for (let child of children)
            dfs(child, depth + 1, result);
    }

    let res: IndentedComment[] = [];
    rootComments.forEach((c) => dfs(c, 0, res));

    // Add any orphaned comments to the end (we may receive them out of order)
    items.forEach((c) => {
        if (!res.find((it) => it.comment.id === c.comment.id))
            res.push(c);
    });
    return res;
}

/**
 * Count timed root comments (for determining whether to show the sort toggle).
 */
export function countTimedRootComments(items: IndentedComment[]): number {
    return items.filter(c => c.indent === 0 && timecodeToSecondsOrNull(c.comment.timecode) !== null).length;
}

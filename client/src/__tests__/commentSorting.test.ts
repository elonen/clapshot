import { describe, it, expect } from 'vitest';
import { timecodeToSeconds, timecodeToSecondsOrNull } from '@/lib/timecodeUtils';
import { indentCommentTree, countTimedRootComments, type CommentSortMode } from '@/lib/commentTree';
import { IndentedComment } from '@/types';

function makeComment(overrides: {
    id?: string;
    parentId?: string;
    timecode?: string;
    created?: Date;
}): IndentedComment {
    return {
        indent: 0,
        comment: {
            id: overrides.id ?? 'c1',
            mediaFileId: 'media-1',
            usernameIfnull: 'User',
            comment: 'text',
            timecode: overrides.timecode,
            parentId: overrides.parentId,
            created: overrides.created ?? new Date('2024-01-01'),
        },
    };
}

describe('timecodeToSeconds', () => {
    it('parses HH:MM:SS.mmm format', () => {
        expect(timecodeToSeconds('00:01:30.500')).toBeCloseTo(90.5);
        expect(timecodeToSeconds('01:00:00.000')).toBe(3600);
        expect(timecodeToSeconds('00:00:00.000')).toBe(0);
    });

    it('parses HH:MM:SS:FF format (assumes 25fps)', () => {
        expect(timecodeToSeconds('00:01:30:00')).toBe(90);
        expect(timecodeToSeconds('00:00:01:12')).toBeCloseTo(1 + 12 / 25);
    });

    it('returns 0 for empty/falsy input', () => {
        expect(timecodeToSeconds('')).toBe(0);
    });

    it('returns 0 for unparseable input', () => {
        expect(timecodeToSeconds('invalid')).toBe(0);
        expect(timecodeToSeconds('30.5s')).toBe(0);
    });
});

describe('timecodeToSecondsOrNull', () => {
    it('returns null for undefined/empty', () => {
        expect(timecodeToSecondsOrNull(undefined)).toBeNull();
        expect(timecodeToSecondsOrNull('')).toBeNull();
    });

    it('returns 0 for genuine 00:00:00.000', () => {
        expect(timecodeToSecondsOrNull('00:00:00.000')).toBe(0);
    });

    it('returns null for unparseable strings', () => {
        expect(timecodeToSecondsOrNull('invalid')).toBeNull();
    });

    it('returns seconds for valid timecodes', () => {
        expect(timecodeToSecondsOrNull('00:01:30.500')).toBeCloseTo(90.5);
    });
});

describe('indentCommentTree', () => {
    describe('date mode (original behavior)', () => {
        it('sorts all roots by created date ASC', () => {
            const items = [
                makeComment({ id: 'c2', created: new Date('2024-01-03') }),
                makeComment({ id: 'c1', created: new Date('2024-01-01') }),
                makeComment({ id: 'c3', created: new Date('2024-01-02') }),
            ];
            const result = indentCommentTree(items, 'date');
            expect(result.map(r => r.comment.id)).toEqual(['c1', 'c3', 'c2']);
        });

        it('sorts children by created date ASC under parent', () => {
            const items = [
                makeComment({ id: 'parent', created: new Date('2024-01-01') }),
                makeComment({ id: 'child2', parentId: 'parent', created: new Date('2024-01-03') }),
                makeComment({ id: 'child1', parentId: 'parent', created: new Date('2024-01-02') }),
            ];
            const result = indentCommentTree(items, 'date');
            expect(result.map(r => r.comment.id)).toEqual(['parent', 'child1', 'child2']);
            expect(result[1].indent).toBe(1);
            expect(result[2].indent).toBe(1);
        });
    });

    describe('timecode mode', () => {
        it('places non-timed roots before timed roots', () => {
            const items = [
                makeComment({ id: 'timed', timecode: '00:01:00.000', created: new Date('2024-01-01') }),
                makeComment({ id: 'untimed', timecode: undefined, created: new Date('2024-01-02') }),
            ];
            const result = indentCommentTree(items, 'timecode');
            expect(result[0].comment.id).toBe('untimed');
            expect(result[1].comment.id).toBe('timed');
        });

        it('sorts non-timed roots by created ASC among themselves', () => {
            const items = [
                makeComment({ id: 'u2', created: new Date('2024-01-03') }),
                makeComment({ id: 'u1', created: new Date('2024-01-01') }),
                makeComment({ id: 'timed', timecode: '00:00:30.000', created: new Date('2024-01-02') }),
            ];
            const result = indentCommentTree(items, 'timecode');
            expect(result[0].comment.id).toBe('u1');
            expect(result[1].comment.id).toBe('u2');
            expect(result[2].comment.id).toBe('timed');
        });

        it('sorts timed roots by timecode, not by created date', () => {
            const items = [
                makeComment({ id: 'late-tc', timecode: '00:05:00.000', created: new Date('2024-01-01') }),
                makeComment({ id: 'early-tc', timecode: '00:01:00.000', created: new Date('2024-01-03') }),
                makeComment({ id: 'mid-tc', timecode: '00:03:00.000', created: new Date('2024-01-02') }),
            ];
            const result = indentCommentTree(items, 'timecode');
            expect(result.map(r => r.comment.id)).toEqual(['early-tc', 'mid-tc', 'late-tc']);
        });

        it('tiebreaks identical timecodes by created date', () => {
            const items = [
                makeComment({ id: 'c2', timecode: '00:01:00.000', created: new Date('2024-01-03') }),
                makeComment({ id: 'c1', timecode: '00:01:00.000', created: new Date('2024-01-01') }),
            ];
            const result = indentCommentTree(items, 'timecode');
            expect(result[0].comment.id).toBe('c1');
            expect(result[1].comment.id).toBe('c2');
        });

        it('sorts children by created ASC regardless of timecode mode', () => {
            const items = [
                makeComment({ id: 'parent', timecode: '00:02:00.000', created: new Date('2024-01-01') }),
                makeComment({ id: 'child2', parentId: 'parent', timecode: '00:01:00.000', created: new Date('2024-01-03') }),
                makeComment({ id: 'child1', parentId: 'parent', timecode: '00:05:00.000', created: new Date('2024-01-02') }),
            ];
            const result = indentCommentTree(items, 'timecode');
            expect(result.map(r => r.comment.id)).toEqual(['parent', 'child1', 'child2']);
        });
    });

    it('appends orphaned comments at the end', () => {
        const items = [
            makeComment({ id: 'root', created: new Date('2024-01-01') }),
            makeComment({ id: 'orphan', parentId: 'missing', created: new Date('2024-01-02') }),
        ];
        const result = indentCommentTree(items, 'date');
        expect(result[0].comment.id).toBe('root');
        expect(result[1].comment.id).toBe('orphan');
    });
});

describe('countTimedRootComments', () => {
    it('returns 0 when no timed root comments', () => {
        const items = [
            { ...makeComment({ id: 'c1' }), indent: 0 },
            { ...makeComment({ id: 'c2' }), indent: 0 },
        ];
        expect(countTimedRootComments(items)).toBe(0);
    });

    it('counts only root comments with valid timecodes', () => {
        const items = [
            { ...makeComment({ id: 'c1', timecode: '00:01:00.000' }), indent: 0 },
            { ...makeComment({ id: 'c2', timecode: '00:02:00.000' }), indent: 0 },
            { ...makeComment({ id: 'c3' }), indent: 0 },
            { ...makeComment({ id: 'child', timecode: '00:03:00.000' }), indent: 1 },
        ];
        expect(countTimedRootComments(items)).toBe(2);
    });

    it('returns 1 when only one timed root (toggle should be hidden)', () => {
        const items = [
            { ...makeComment({ id: 'c1', timecode: '00:01:00.000' }), indent: 0 },
            { ...makeComment({ id: 'c2' }), indent: 0 },
        ];
        expect(countTimedRootComments(items)).toBe(1);
    });
});

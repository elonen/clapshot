import { describe, it, expect } from 'vitest';
import { indentCommentTree, countTimedRootComments, type CommentSortMode } from '@/lib/commentTree';
import { IndentedComment } from '@/types';

function makeComment(overrides: {
  id?: string;
  parentId?: string;
  timecode?: string;
  created?: Date;
  comment?: string;
}): IndentedComment {
  return {
    indent: 0,
    comment: {
      id: overrides.id ?? 'c1',
      mediaFileId: 'media-1',
      usernameIfnull: 'User',
      comment: overrides.comment ?? 'text',
      timecode: overrides.timecode,
      parentId: overrides.parentId,
      created: overrides.created ?? new Date('2024-01-01'),
    },
  };
}

describe('indentCommentTree', () => {
  it('defaults to date mode when no sort mode is given', () => {
    const items = [
      makeComment({ id: 'c2', created: new Date('2024-01-03') }),
      makeComment({ id: 'c1', created: new Date('2024-01-01') }),
    ];
    const result = indentCommentTree(items);
    expect(result.map(r => r.comment.id)).toEqual(['c1', 'c2']);
  });

  it('preserves existing indent values in the output', () => {
    const items = [
      { ...makeComment({ id: 'root' }), indent: 0 },
      { ...makeComment({ id: 'child', parentId: 'root' }), indent: 1 },
    ];
    const result = indentCommentTree(items, 'date');
    expect(result[0].indent).toBe(0);
    expect(result[1].indent).toBe(1);
  });

  it('handles deeply nested replies', () => {
    const items = [
      makeComment({ id: 'r', created: new Date('2024-01-01') }),
      makeComment({ id: 'c1', parentId: 'r', created: new Date('2024-01-02') }),
      makeComment({ id: 'c2', parentId: 'c1', created: new Date('2024-01-03') }),
      makeComment({ id: 'c3', parentId: 'c2', created: new Date('2024-01-04') }),
    ];
    const result = indentCommentTree(items, 'date');
    expect(result.map(r => ({ id: r.comment.id, indent: r.indent }))).toEqual([
      { id: 'r', indent: 0 },
      { id: 'c1', indent: 1 },
      { id: 'c2', indent: 2 },
      { id: 'c3', indent: 3 },
    ]);
  });

  it('sorts root comments by timecode ASC in timecode mode', () => {
    const items = [
      makeComment({ id: 'c3', timecode: '00:03:00.000' }),
      makeComment({ id: 'c1', timecode: '00:01:00.000' }),
      makeComment({ id: 'c2', timecode: '00:02:00.000' }),
    ];
    const result = indentCommentTree(items, 'timecode');
    expect(result.map(r => r.comment.id)).toEqual(['c1', 'c2', 'c3']);
  });

  it('places non-timed roots before timed roots and sorts by created date', () => {
    const items = [
      makeComment({ id: 't1', timecode: '00:01:00.000', created: new Date('2024-01-01') }),
      makeComment({ id: 'u2', created: new Date('2024-01-03') }),
      makeComment({ id: 'u1', created: new Date('2024-01-02') }),
    ];
    const result = indentCommentTree(items, 'timecode');
    expect(result.map(r => r.comment.id)).toEqual(['u1', 'u2', 't1']);
  });

  it('tiebreaks timed roots by created date when timecodes match', () => {
    const items = [
      makeComment({ id: 'c2', timecode: '00:01:00.000', created: new Date('2024-01-05') }),
      makeComment({ id: 'c1', timecode: '00:01:00.000', created: new Date('2024-01-02') }),
      makeComment({ id: 'c3', timecode: '00:01:00.000', created: new Date('2024-01-03') }),
    ];
    const result = indentCommentTree(items, 'timecode');
    expect(result.map(r => r.comment.id)).toEqual(['c1', 'c3', 'c2']);
  });

  it('sorts children by created date regardless of timecode mode', () => {
    const items = [
      makeComment({ id: 'p', timecode: '00:05:00.000', created: new Date('2024-01-01') }),
      makeComment({ id: 'c2', parentId: 'p', timecode: '00:01:00.000', created: new Date('2024-01-04') }),
      makeComment({ id: 'c1', parentId: 'p', timecode: '00:09:00.000', created: new Date('2024-01-02') }),
    ];
    const result = indentCommentTree(items, 'timecode');
    expect(result.map(r => r.comment.id)).toEqual(['p', 'c1', 'c2']);
  });

  it('appends orphaned comments to the end without modifying indent', () => {
    const items = [
      makeComment({ id: 'root', created: new Date('2024-01-01') }),
      makeComment({ id: 'orphan', parentId: 'missing', created: new Date('2024-01-02') }),
    ];
    const result = indentCommentTree(items, 'date');
    expect(result.map(r => r.comment.id)).toEqual(['root', 'orphan']);
    expect(result[1].indent).toBe(0);
  });

  it('handles a circular parent reference without infinite looping', () => {
    const items = [
      makeComment({ id: 'a', parentId: 'b' }),
      makeComment({ id: 'b', parentId: 'a' }),
    ];
    const result = indentCommentTree(items, 'date');
    // Both are roots (parentId != null) and neither is found as root, so they are appended as orphans
    expect(result).toHaveLength(2);
    expect(result.map(r => r.comment.id)).toContain('a');
    expect(result.map(r => r.comment.id)).toContain('b');
  });

  it('handles an empty comment list', () => {
    expect(indentCommentTree([], 'date')).toEqual([]);
    expect(indentCommentTree([], 'timecode')).toEqual([]);
  });

  it('handles a single root comment', () => {
    const items = [makeComment({ id: 'only' })];
    const result = indentCommentTree(items, 'date');
    expect(result).toHaveLength(1);
    expect(result[0].comment.id).toBe('only');
    expect(result[0].indent).toBe(0);
  });
});

describe('countTimedRootComments', () => {
  it('returns 0 for an empty list', () => {
    expect(countTimedRootComments([])).toBe(0);
  });

  it('counts only root-level timed comments', () => {
    const items = [
      { ...makeComment({ id: 'root-timed', timecode: '00:01:00.000' }), indent: 0 },
      { ...makeComment({ id: 'root-untimed' }), indent: 0 },
      { ...makeComment({ id: 'child-timed', timecode: '00:02:00.000' }), indent: 1 },
    ];
    expect(countTimedRootComments(items)).toBe(1);
  });

  it('does not count invalid timecodes as timed', () => {
    const items = [
      { ...makeComment({ id: 'bad', timecode: 'not-a-timecode' }), indent: 0 },
      { ...makeComment({ id: 'zero', timecode: '00:00:00.000' }), indent: 0 },
    ];
    expect(countTimedRootComments(items)).toBe(1);
  });
});

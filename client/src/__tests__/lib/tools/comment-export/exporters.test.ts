import { describe, it, expect } from 'vitest';
import { resolveEdlExporter } from '@/lib/tools/comment-export/resolve-edl';
import { premiereXmlExporter } from '@/lib/tools/comment-export/premiere-xml';
import { srtExporter } from '@/lib/tools/comment-export/srt';
import { csvExporter } from '@/lib/tools/comment-export/csv';
import { otioNotesExporter } from '@/lib/tools/comment-export/otio-notes';
import { groupComments } from '@/lib/tools/comment-export/index';
import type { ExportContext } from '@/lib/tools/comment-export/types';
import type { IndentedComment } from '@/types';

const createMockContext = (overrides: Partial<ExportContext> = {}): ExportContext => ({
    comments: [
        {
            timecode: '00:01:30.500',
            text: 'First test comment',
            hasDrawing: false,
            username: 'TestUser',
            created: new Date('2024-01-15T10:30:00Z'),
        },
        {
            timecode: '00:02:45.000',
            text: 'Second comment with drawing',
            hasDrawing: true,
            username: 'AnotherUser',
            created: new Date('2024-01-15T11:00:00Z'),
        },
        {
            timecode: '',
            text: 'Untimed comment',
            hasDrawing: false,
            username: 'TestUser',
        },
    ],
    title: 'Test Video Title',
    fps: 25,
    duration: 300,
    ...overrides,
});

describe('Comment Exporters', () => {
    describe('resolve-edl', () => {
        it('should have correct metadata', () => {
            expect(resolveEdlExporter.id).toBe('resolve-edl');
            expect(resolveEdlExporter.extension).toBe('.edl');
            expect(resolveEdlExporter.options.length).toBeGreaterThan(0);
        });

        it('should export valid EDL with header', () => {
            const ctx = createMockContext();
            const output = resolveEdlExporter.export(ctx, { markerColor: 'Purple' });

            expect(output).toContain('TITLE: Test Video Title');
            expect(output).toContain('FCM: NON DROP FRAME');
        });

        it('should include numbered events', () => {
            const ctx = createMockContext();
            const output = resolveEdlExporter.export(ctx, { markerColor: 'Blue' });

            expect(output).toContain('001');
            expect(output).toContain('002');
            expect(output).toContain('003');
        });

        it('should include marker color metadata', () => {
            const ctx = createMockContext();
            const output = resolveEdlExporter.export(ctx, { markerColor: 'Green' });

            expect(output).toContain('|C:ResolveColorGreen');
        });

        it('should include username in output', () => {
            const ctx = createMockContext();
            const output = resolveEdlExporter.export(ctx, {});

            expect(output).toContain('TestUser');
            expect(output).toContain('AnotherUser');
        });

        it('should handle empty comments array', () => {
            const ctx = createMockContext({ comments: [] });
            const output = resolveEdlExporter.export(ctx, {});

            expect(output).toContain('TITLE:');
            expect(output).not.toContain('001');
        });
    });

    describe('premiere-xml', () => {
        it('should have correct metadata', () => {
            expect(premiereXmlExporter.id).toBe('premiere-xml');
            expect(premiereXmlExporter.extension).toBe('.xml');
        });

        it('should export valid XML declaration', () => {
            const ctx = createMockContext();
            const output = premiereXmlExporter.export(ctx, {});

            expect(output).toContain('<?xml version="1.0" encoding="UTF-8"?>');
            expect(output).toContain('<!DOCTYPE xmeml>');
            expect(output).toContain('<xmeml version="4">');
        });

        it('should include markers', () => {
            const ctx = createMockContext();
            const output = premiereXmlExporter.export(ctx, {});

            expect(output).toContain('<marker>');
            expect(output).toContain('<comment>');
            expect(output).toContain('</marker>');
        });

        it('should include comment text in markers', () => {
            const ctx = createMockContext();
            const output = premiereXmlExporter.export(ctx, {});

            expect(output).toContain('First test comment');
            expect(output).toContain('Second comment with drawing');
        });

        it('should escape XML special characters', () => {
            const ctx = createMockContext({
                comments: [{
                    timecode: '00:00:01.000',
                    text: 'Comment with <special> & "chars"',
                    hasDrawing: false,
                }],
            });
            const output = premiereXmlExporter.export(ctx, {});

            expect(output).toContain('&lt;special&gt;');
            expect(output).toContain('&amp;');
            expect(output).toContain('&quot;');
        });

        it('should be well-formed XML (matching tags)', () => {
            const ctx = createMockContext();
            const output = premiereXmlExporter.export(ctx, {});

            expect(output).toContain('</xmeml>');
            expect(output).toContain('</sequence>');
        });
    });

    describe('srt', () => {
        it('should have correct metadata', () => {
            expect(srtExporter.id).toBe('srt');
            expect(srtExporter.extension).toBe('.srt');
        });

        it('should export numbered entries', () => {
            const ctx = createMockContext();
            const output = srtExporter.export(ctx, { durationSeconds: 3 });

            expect(output).toContain('1\n');
            expect(output).toContain('2\n');
            expect(output).toContain('3\n');
        });

        it('should include timecode arrows', () => {
            const ctx = createMockContext();
            const output = srtExporter.export(ctx, {});

            expect(output).toContain('-->');
        });

        it('should use SRT timecode format (comma for milliseconds)', () => {
            const ctx = createMockContext();
            const output = srtExporter.export(ctx, {});

            // SRT uses comma for milliseconds: 00:01:30,500
            expect(output).toMatch(/\d{2}:\d{2}:\d{2},\d{3}/);
        });

        it('should include comment text', () => {
            const ctx = createMockContext();
            const output = srtExporter.export(ctx, {});

            expect(output).toContain('First test comment');
            expect(output).toContain('Untimed comment');
        });

        it('should respect duration option', () => {
            const ctx = createMockContext({
                comments: [{
                    timecode: '00:00:00.000',
                    text: 'Test',
                    hasDrawing: false,
                }],
            });
            const output = srtExporter.export(ctx, { durationSeconds: 5 });

            // Start at 00:00:00,000, should end at 00:00:05,000
            expect(output).toContain('00:00:00,000 --> 00:00:05,000');
        });
    });

    describe('csv', () => {
        it('should have correct metadata', () => {
            expect(csvExporter.id).toBe('csv');
            expect(csvExporter.extension).toBe('.csv');
        });

        it('should include header row by default', () => {
            const ctx = createMockContext();
            const output = csvExporter.export(ctx, {});

            expect(output).toContain('Commenter');
            expect(output).toContain('Comment');
            expect(output).toContain('Timecode');
        });

        it('should exclude header row when option disabled', () => {
            const ctx = createMockContext();
            const output = csvExporter.export(ctx, { includeHeaders: false });

            // First line should be data, not header
            const lines = output.split('\n').filter(l => l.trim());
            expect(lines[0]).not.toContain('Commenter');
            expect(lines[0]).toContain('TestUser');
        });

        it('should use comma delimiter by default', () => {
            const ctx = createMockContext();
            const output = csvExporter.export(ctx, {});

            const lines = output.split('\n');
            expect(lines[0]).toContain(',');
        });

        it('should support semicolon delimiter', () => {
            const ctx = createMockContext();
            const output = csvExporter.export(ctx, { delimiter: 'semicolon' });

            expect(output).toContain(';');
        });

        it('should support tab delimiter', () => {
            const ctx = createMockContext();
            const output = csvExporter.export(ctx, { delimiter: 'tab' });

            expect(output).toContain('\t');
        });

        it('should escape fields with special characters', () => {
            const ctx = createMockContext({
                comments: [{
                    timecode: '00:00:01.000',
                    text: 'Comment with, comma and "quotes"',
                    hasDrawing: false,
                    username: 'User',
                }],
            });
            const output = csvExporter.export(ctx, {});

            // Fields with commas or quotes should be quoted
            expect(output).toContain('"Comment with, comma and ""quotes"""');
        });

        it('should include Has Drawing column', () => {
            const ctx = createMockContext();
            const output = csvExporter.export(ctx, {});

            expect(output).toContain('Has Drawing');
            expect(output).toContain('Yes');
            expect(output).toContain('No');
        });
    });

    describe('otio-notes', () => {
        it('should have correct metadata', () => {
            expect(otioNotesExporter.id).toBe('otio-notes');
            expect(otioNotesExporter.extension).toBe('.otrn');
            expect(otioNotesExporter.options.length).toBe(0);
        });

        it('should export valid JSON', () => {
            const ctx = createMockContext();
            const output = otioNotesExporter.export(ctx, {});

            expect(() => JSON.parse(output)).not.toThrow();
        });

        it('should have correct structure', () => {
            const ctx = createMockContext();
            const output = otioNotesExporter.export(ctx, {});
            const parsed = JSON.parse(output);

            expect(parsed).toHaveProperty('metadata');
            expect(parsed).toHaveProperty('sequence');
            expect(parsed.sequence).toHaveProperty('notes');
            expect(Array.isArray(parsed.sequence.notes)).toBe(true);
        });

        it('should include all comments as notes', () => {
            const ctx = createMockContext();
            const output = otioNotesExporter.export(ctx, {});
            const parsed = JSON.parse(output);

            expect(parsed.sequence.notes.length).toBe(3);
        });

        it('should convert timecode to seconds', () => {
            const ctx = createMockContext({
                comments: [{
                    timecode: '00:01:30.500',
                    text: 'Test',
                    hasDrawing: false,
                }],
            });
            const output = otioNotesExporter.export(ctx, {});
            const parsed = JSON.parse(output);

            // 1 minute 30.5 seconds = 90.5 seconds
            expect(parsed.sequence.notes[0].time).toBeCloseTo(90.5, 1);
        });

        it('should include comment text', () => {
            const ctx = createMockContext();
            const output = otioNotesExporter.export(ctx, {});
            const parsed = JSON.parse(output);

            expect(parsed.sequence.notes[0].comment).toContain('First test comment');
        });

        it('should mark drawings in comment text', () => {
            const ctx = createMockContext();
            const output = otioNotesExporter.export(ctx, {});
            const parsed = JSON.parse(output);

            const drawingNote = parsed.sequence.notes.find((n: { comment: string }) =>
                n.comment.includes('Second comment')
            );
            expect(drawingNote.comment).toContain('[has drawing]');
        });
    });

    describe('common behavior', () => {
        const allExporters = [
            resolveEdlExporter,
            premiereXmlExporter,
            srtExporter,
            csvExporter,
            otioNotesExporter,
        ];

        it.each(allExporters)('$name should produce non-empty output', (exporter) => {
            const ctx = createMockContext();
            const output = exporter.export(ctx, {});

            expect(output).toBeTruthy();
            expect(output.length).toBeGreaterThan(0);
        });

        it.each(allExporters)('$name should handle empty comments', (exporter) => {
            const ctx = createMockContext({ comments: [] });

            expect(() => exporter.export(ctx, {})).not.toThrow();
        });

        it.each(allExporters)('$name should handle missing optional fields', (exporter) => {
            const ctx: ExportContext = {
                comments: [{
                    timecode: '',
                    text: 'Minimal comment',
                    hasDrawing: false,
                }],
                title: 'Test',
                fps: 24,
            };

            expect(() => exporter.export(ctx, {})).not.toThrow();
        });
    });
});

describe('groupComments', () => {
    const createIndentedComment = (overrides: {
        id?: string;
        parentId?: string;
        comment?: string;
        timecode?: string;
        drawing?: string;
        usernameIfnull?: string;
    } = {}): IndentedComment => ({
        indent: overrides.parentId ? 1 : 0,
        comment: {
            id: overrides.id || 'comment-1',
            mediaFileId: 'media-1',
            usernameIfnull: overrides.usernameIfnull || 'TestUser',
            comment: overrides.comment || 'Test comment',
            timecode: overrides.timecode,
            parentId: overrides.parentId,
            drawing: overrides.drawing,
        },
    });

    describe('hasDrawing flag', () => {
        it('should set hasDrawing=false when comment has no drawing', () => {
            const comments = [createIndentedComment({ drawing: undefined })];
            const result = groupComments(comments);

            expect(result[0].hasDrawing).toBe(false);
            expect(result[0].text).not.toContain('[has drawing]');
        });

        it('should set hasDrawing=false when drawing is empty string', () => {
            const comments = [createIndentedComment({ drawing: '' })];
            const result = groupComments(comments);

            expect(result[0].hasDrawing).toBe(false);
            expect(result[0].text).not.toContain('[has drawing]');
        });

        it('should set hasDrawing=true when comment has drawing data', () => {
            const comments = [createIndentedComment({ drawing: 'data:image/png;base64,abc123' })];
            const result = groupComments(comments);

            expect(result[0].hasDrawing).toBe(true);
        });

        it('should NOT add [has drawing] to text for parent with drawing (exporter does that)', () => {
            const comments = [createIndentedComment({
                comment: 'Parent comment',
                drawing: 'data:image/png;base64,abc123',
            })];
            const result = groupComments(comments);

            // groupComments sets hasDrawing flag, but doesn't modify text
            // The exporter is responsible for adding [has drawing] to output
            expect(result[0].text).toBe('Parent comment');
            expect(result[0].hasDrawing).toBe(true);
        });

        it('should add [has drawing] to reply text when reply has drawing', () => {
            const comments = [
                createIndentedComment({ id: 'parent-1', comment: 'Parent comment' }),
                createIndentedComment({
                    id: 'reply-1',
                    parentId: 'parent-1',
                    comment: 'Reply with art',
                    drawing: 'data:image/png;base64,abc123',
                }),
            ];
            const result = groupComments(comments);

            expect(result[0].text).toContain('Reply with art [has drawing]');
        });

        it('should NOT add [has drawing] to reply text when reply has no drawing', () => {
            const comments = [
                createIndentedComment({ id: 'parent-1', comment: 'Parent comment' }),
                createIndentedComment({
                    id: 'reply-1',
                    parentId: 'parent-1',
                    comment: 'Reply without art',
                    drawing: undefined,
                }),
            ];
            const result = groupComments(comments);

            expect(result[0].text).toContain('Reply without art');
            expect(result[0].text).not.toContain('[has drawing]');
        });

        it('should handle mixed: parent with drawing, reply without', () => {
            const comments = [
                createIndentedComment({
                    id: 'parent-1',
                    comment: 'Parent with art',
                    drawing: 'data:image/png;base64,parent',
                }),
                createIndentedComment({
                    id: 'reply-1',
                    parentId: 'parent-1',
                    comment: 'Plain reply',
                    drawing: undefined,
                }),
            ];
            const result = groupComments(comments);

            expect(result[0].hasDrawing).toBe(true);
            expect(result[0].text).toBe('Parent with art // Plain reply');
            // No [has drawing] in text because reply has no drawing
            // Parent's hasDrawing flag is true, exporter will add marker
        });

        it('should handle mixed: parent without drawing, reply with', () => {
            const comments = [
                createIndentedComment({
                    id: 'parent-1',
                    comment: 'Plain parent',
                    drawing: undefined,
                }),
                createIndentedComment({
                    id: 'reply-1',
                    parentId: 'parent-1',
                    comment: 'Reply with art',
                    drawing: 'data:image/png;base64,reply',
                }),
            ];
            const result = groupComments(comments);

            expect(result[0].hasDrawing).toBe(false);
            expect(result[0].text).toBe('Plain parent // Reply with art [has drawing]');
        });
    });

    describe('reply merging', () => {
        it('should merge multiple replies with separator', () => {
            const comments = [
                createIndentedComment({ id: 'parent-1', comment: 'Parent' }),
                createIndentedComment({ id: 'reply-1', parentId: 'parent-1', comment: 'Reply 1' }),
                createIndentedComment({ id: 'reply-2', parentId: 'parent-1', comment: 'Reply 2' }),
            ];
            const result = groupComments(comments);

            expect(result.length).toBe(1);
            expect(result[0].text).toBe('Parent // Reply 1 // Reply 2');
        });

        it('should not include orphaned replies (parent not in list)', () => {
            const comments = [
                createIndentedComment({ id: 'reply-1', parentId: 'missing-parent', comment: 'Orphan' }),
            ];
            const result = groupComments(comments);

            // Orphaned reply is not included since parent doesn't exist
            expect(result.length).toBe(0);
        });
    });

    describe('sorting', () => {
        it('should sort by timecode', () => {
            const comments = [
                createIndentedComment({ id: 'c2', comment: 'Second', timecode: '00:02:00.000' }),
                createIndentedComment({ id: 'c1', comment: 'First', timecode: '00:01:00.000' }),
                createIndentedComment({ id: 'c3', comment: 'Third', timecode: '00:03:00.000' }),
            ];
            const result = groupComments(comments);

            expect(result[0].text).toBe('First');
            expect(result[1].text).toBe('Second');
            expect(result[2].text).toBe('Third');
        });

        it('should put untimed comments first', () => {
            const comments = [
                createIndentedComment({ id: 'c1', comment: 'Timed', timecode: '00:01:00.000' }),
                createIndentedComment({ id: 'c2', comment: 'Untimed', timecode: undefined }),
            ];
            const result = groupComments(comments);

            expect(result[0].text).toBe('Untimed');
            expect(result[1].text).toBe('Timed');
        });
    });
});

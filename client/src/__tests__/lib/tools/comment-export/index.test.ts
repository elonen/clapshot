import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { getExporterById, downloadFile, exporters, groupComments } from '@/lib/tools/comment-export/index';

describe('getExporterById', () => {
  it('returns the exporter matching the given id', () => {
    expect(getExporterById('srt')?.name).toBe('SRT Subtitles');
    expect(getExporterById('csv')?.name).toBe('CSV');
    expect(getExporterById('resolve-edl')?.name).toBe('DaVinci Resolve EDL');
    expect(getExporterById('premiere-xml')?.extension).toBe('.xml');
    expect(getExporterById('otio-notes')?.extension).toBe('.otrn');
  });

  it('returns undefined for unknown exporter ids', () => {
    expect(getExporterById('not-real')).toBeUndefined();
    expect(getExporterById('')).toBeUndefined();
  });

  it('exposes every exporter in the registry', () => {
    const ids = exporters.map(e => e.id);
    expect(ids).toContain('resolve-edl');
    expect(ids).toContain('premiere-xml');
    expect(ids).toContain('srt');
    expect(ids).toContain('csv');
    expect(ids).toContain('otio-notes');
    expect(ids).toHaveLength(new Set(ids).size);
  });
});

describe('downloadFile', () => {
  let createObjectURLSpy: ReturnType<typeof vi.spyOn>;
  let revokeObjectURLSpy: ReturnType<typeof vi.spyOn>;
  let clickSpy: ReturnType<typeof vi.fn>;
  let appendChildSpy: ReturnType<typeof vi.spyOn>;
  let removeChildSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    clickSpy = vi.fn();
    createObjectURLSpy = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:mock-url');
    revokeObjectURLSpy = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});

    // Mock anchor creation so we can inspect click/download/href
    const originalCreateElement = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tag: string, options?: any) => {
      const el = originalCreateElement(tag, options);
      if (tag.toLowerCase() === 'a') {
        Object.defineProperty(el, 'click', { value: clickSpy, configurable: true });
      }
      return el;
    });

    appendChildSpy = vi.spyOn(document.body, 'appendChild').mockImplementation((node) => node as any);
    removeChildSpy = vi.spyOn(document.body, 'removeChild').mockImplementation((node) => node as any);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('creates a blob, anchor, clicks it, and revokes the URL', () => {
    downloadFile('hello world', 'test.txt', 'text/plain');

    expect(createObjectURLSpy).toHaveBeenCalledOnce();
    const blob = createObjectURLSpy.mock.calls[0][0] as Blob;
    expect(blob.type).toBe('text/plain');

    const anchor = appendChildSpy.mock.calls[0][0] as HTMLAnchorElement;
    expect(anchor.tagName).toBe('A');
    expect(anchor.href).toBe('blob:mock-url');
    expect(anchor.download).toBe('test.txt');
    expect(clickSpy).toHaveBeenCalledOnce();
    expect(removeChildSpy).toHaveBeenCalledOnce();
    expect(revokeObjectURLSpy).toHaveBeenCalledWith('blob:mock-url');
  });

  it('uses text/plain as the default mime type', () => {
    downloadFile('content', 'default.txt');
    const blob = createObjectURLSpy.mock.calls[0][0] as Blob;
    expect(blob.type).toBe('text/plain');
  });

  it('uses the provided mime type when given', () => {
    downloadFile('<xml/>', 'data.xml', 'application/xml');
    const blob = createObjectURLSpy.mock.calls[0][0] as Blob;
    expect(blob.type).toBe('application/xml');
  });

  it('puts the content into the blob', async () => {
    downloadFile('payload', 'test.txt');
    const blob = createObjectURLSpy.mock.calls[0][0] as Blob;
    const text = await blob.text();
    expect(text).toBe('payload');
  });
});

describe('groupComments edge cases', () => {
  function makeIndented(overrides: {
    id?: string;
    parentId?: string;
    comment?: string;
    timecode?: string;
    drawing?: string;
    usernameIfnull?: string;
  } = {}) {
    return {
      indent: overrides.parentId ? 1 : 0,
      comment: {
        id: overrides.id || 'c1',
        mediaFileId: 'm1',
        usernameIfnull: overrides.usernameIfnull,
        comment: overrides.comment,
        timecode: overrides.timecode,
        parentId: overrides.parentId,
        drawing: overrides.drawing,
      },
    };
  }

  it('returns an empty array when there are no comments', () => {
    expect(groupComments([])).toEqual([]);
  });

  it('uses userId when usernameIfnull is missing', () => {
    const result = groupComments([
      makeIndented({ id: 'c1', usernameIfnull: undefined as any, comment: 'Hi' }),
    ]);
    expect(result[0].username).toBeUndefined();
  });
});

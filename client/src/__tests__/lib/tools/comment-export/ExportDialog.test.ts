import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, waitFor, cleanup, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import ExportDialog from '@/lib/tools/comment-export/ExportDialog.svelte';
import { allComments, curVideo } from '@/stores';
import { IndentedComment } from '@/types';

function makeComment(overrides: {
  id?: string;
  parentId?: string;
  comment?: string;
  timecode?: string;
  drawing?: string;
  usernameIfnull?: string;
}): IndentedComment {
  return {
    indent: overrides.parentId ? 1 : 0,
    comment: {
      id: overrides.id ?? 'c1',
      mediaFileId: 'media-1',
      usernameIfnull: overrides.usernameIfnull ?? 'User',
      comment: overrides.comment ?? 'Test comment',
      timecode: overrides.timecode,
      parentId: overrides.parentId,
      drawing: overrides.drawing,
    },
  };
}

describe('ExportDialog.svelte', () => {
  let createObjectURLSpy: ReturnType<typeof vi.spyOn>;
  let revokeObjectURLSpy: ReturnType<typeof vi.spyOn>;
  let clickSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    allComments.set([]);
    curVideo.set({ id: 'video-1', title: 'Test Video', duration: { fps: '30' } } as any);

    clickSpy = vi.fn();
    createObjectURLSpy = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:mock-url');
    revokeObjectURLSpy = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});

    const originalCreateElement = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tag: string, options?: any) => {
      const el = originalCreateElement(tag, options);
      if (tag.toLowerCase() === 'a') {
        Object.defineProperty(el, 'click', { value: clickSpy, configurable: true });
      }
      return el;
    });

    const originalAppendChild = document.body.appendChild.bind(document.body);
    vi.spyOn(document.body, 'appendChild').mockImplementation((node) => originalAppendChild(node));
    const originalRemoveChild = document.body.removeChild.bind(document.body);
    vi.spyOn(document.body, 'removeChild').mockImplementation((node) => originalRemoveChild(node));
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders the export dialog when open', () => {
    render(ExportDialog, { props: { isOpen: true } });
    expect(screen.getByText('Export Comments')).toBeInTheDocument();
    expect(screen.getByLabelText('Export format')).toBeInTheDocument();
    expect(screen.getByLabelText('Frame rate')).toBeInTheDocument();
  });

  it('shows a message when there are no comments to export', () => {
    render(ExportDialog, { props: { isOpen: true } });
    expect(screen.getByText('No comments to export.')).toBeInTheDocument();
    expect(screen.queryByText('Export')).not.toBeInTheDocument();
  });

  it('shows the export button when comments are present', () => {
    allComments.set([makeComment({ id: 'c1', comment: 'Hello' })]);
    render(ExportDialog, { props: { isOpen: true } });
    expect(screen.getByText('Export')).toBeInTheDocument();
    expect(screen.getByText('1 comment will be exported.')).toBeInTheDocument();
  });

  it('renders format-specific options for the default exporter', () => {
    allComments.set([makeComment({ id: 'c1' })]);
    render(ExportDialog, { props: { isOpen: true } });
    // Default exporter is resolve-edl, which has a marker color select
    expect(screen.getByLabelText('Marker color')).toBeInTheDocument();
  });

  it('updates options when a different format is selected', async () => {
    allComments.set([makeComment({ id: 'c1' })]);
    render(ExportDialog, { props: { isOpen: true } });

    const formatSelect = screen.getByLabelText('Export format') as HTMLSelectElement;
    // Switch to CSV
    fireEvent.change(formatSelect, { target: { value: 'csv' } });
    await tick();

    await waitFor(() => {
      expect(screen.getByLabelText('Include header row')).toBeInTheDocument();
      expect(screen.getByLabelText('Delimiter')).toBeInTheDocument();
      expect(screen.queryByLabelText('Marker color')).not.toBeInTheDocument();
    });

    // Switch to SRT
    fireEvent.change(formatSelect, { target: { value: 'srt' } });
    await tick();
    await waitFor(() => {
      expect(screen.getByLabelText('Duration per comment (seconds)')).toBeInTheDocument();
      expect(screen.queryByLabelText('Include header row')).not.toBeInTheDocument();
    });
  });

  it('triggers a download when Export is clicked', async () => {
    allComments.set([makeComment({ id: 'c1', comment: 'Hello world', timecode: '00:01:00.000' })]);
    render(ExportDialog, { props: { isOpen: true } });

    fireEvent.click(screen.getByText('Export'));

    await waitFor(() => {
      expect(clickSpy).toHaveBeenCalledOnce();
      expect(createObjectURLSpy).toHaveBeenCalledOnce();
      expect(revokeObjectURLSpy).toHaveBeenCalledWith('blob:mock-url');
    });

    const anchor = (document.createElement as any).mock.results.find(
      (r: any) => r.value?.tagName === 'A'
    )?.value as HTMLAnchorElement;
    expect(anchor.download).toMatch(/^video-1\./);
  });

  it('uses the video id in the filename when no title is available', async () => {
    curVideo.set({ id: 'video-2', duration: { fps: '24' } } as any);
    allComments.set([makeComment({ id: 'c1' })]);
    render(ExportDialog, { props: { isOpen: true } });

    fireEvent.click(screen.getByText('Export'));

    await waitFor(() => {
      const anchor = (document.createElement as any).mock.results.find(
        (r: any) => r.value?.tagName === 'A'
      )?.value as HTMLAnchorElement;
      expect(anchor.download).toMatch(/^video-2\./);
    });
  });

  it('passes the configured frame rate to the exporter', async () => {
    allComments.set([makeComment({ id: 'c1', timecode: '00:01:00:000' })]);
    render(ExportDialog, { props: { isOpen: true } });

    const fpsInput = screen.getByLabelText('Frame rate') as HTMLInputElement;
    expect(fpsInput.value).toBe('30');

    fireEvent.input(fpsInput, { target: { value: '60' } });
    fireEvent.click(screen.getByText('Export'));

    await waitFor(() => expect(clickSpy).toHaveBeenCalledOnce());
  });

  it('closes the dialog when Cancel is clicked', async () => {
    const { component } = render(ExportDialog, { props: { isOpen: true } });
    fireEvent.click(screen.getByText('Cancel'));
    await waitFor(() => expect(component.isOpen).toBe(false));
  });
});

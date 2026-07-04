import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, waitFor, cleanup, fireEvent } from '@testing-library/svelte';
import EDLImport from '@/lib/tools/EDLImport.svelte';
import { curVideo } from '@/stores';

const sampleEDL = `TITLE: TEST EDIT
FCM: NON-DROP FRAME

001  AX       C     01:00:00:00 01:00:05:00 00:00:00:00 00:00:05:00
* FROM CLIP NAME:  Shot_01.mov

002  AX       C     01:00:10:00 01:00:15:00 00:00:05:00 00:00:10:00
* FROM CLIP NAME:  Shot_02.mov
`;

const sampleEDLNoClipNames = `TITLE: TEST EDIT
FCM: NON-DROP FRAME

001  AX       C     01:00:00:00 01:00:05:00 00:00:00:00 00:00:05:00
002  AX       C     01:00:10:00 01:00:15:00 00:00:05:00 00:00:10:00
`;

const invalidEDL = `This is not an EDL file.
It has no timecodes at all.
`;

describe('EDLImport.svelte', () => {
  function findFileInput() {
    return document.querySelector('input[type="file"]') as HTMLInputElement;
  }

  function uploadFile(input: HTMLInputElement, file: File) {
    fireEvent.change(input, { target: { files: [file] } });
  }

  beforeEach(() => {
    curVideo.set({ id: 'video-1', duration: { fps: '30' } } as any);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('renders the import dialog when open', () => {
    render(EDLImport, { props: { isOpen: true } });
    expect(screen.getByText('Import EDL as Comments')).toBeInTheDocument();
    expect(screen.getByLabelText('Upload EDL')).toBeInTheDocument();
    expect(screen.getByLabelText('Frame rate')).toBeInTheDocument();
  });

  it('reads the frame rate from the current video', () => {
    render(EDLImport, { props: { isOpen: true } });
    const fpsInput = screen.getByLabelText('Frame rate') as HTMLInputElement;
    expect(fpsInput.value).toBe('30');
  });

  it('falls back to 24fps when the video has no usable fps', async () => {
    curVideo.set({ id: 'video-1', duration: { fps: 'not-a-number' } } as any);
    render(EDLImport, { props: { isOpen: true } });
    const fpsInput = screen.getByLabelText('Frame rate') as HTMLInputElement;
    await waitFor(() => expect(fpsInput.value).toBe('24'));
  });

  it('falls back to 24fps when there is no current video', async () => {
    curVideo.set(null);
    render(EDLImport, { props: { isOpen: true } });
    const fpsInput = screen.getByLabelText('Frame rate') as HTMLInputElement;
    await waitFor(() => expect(fpsInput.value).toBe('24'));
  });

  it('parses a standard EDL file and lists the parsed spans', async () => {
    render(EDLImport, { props: { isOpen: true } });
    const input = findFileInput();
    const file = new File([sampleEDL], 'test.edl', { type: 'text/plain' });

    uploadFile(input, file);

    await waitFor(() => {
      expect(screen.getByText('Parsed spans')).toBeInTheDocument();
    });
    expect(screen.getByText('00:00:00:00: Shot_01.mov')).toBeInTheDocument();
    expect(screen.getByText('00:00:05:00: Shot_02.mov')).toBeInTheDocument();
  });

  it('falls back to the event number when FROM CLIP NAME is absent', async () => {
    render(EDLImport, { props: { isOpen: true } });
    const input = findFileInput();
    const file = new File([sampleEDLNoClipNames], 'test.edl', { type: 'text/plain' });

    uploadFile(input, file);

    await waitFor(() => {
      expect(screen.getByText('00:00:00:00: 001')).toBeInTheDocument();
      expect(screen.getByText('00:00:05:00: 002')).toBeInTheDocument();
    });
  });

  it('shows an error message for empty/invalid EDL input', async () => {
    render(EDLImport, { props: { isOpen: true } });
    const input = findFileInput();
    const file = new File([invalidEDL], 'bad.edl', { type: 'text/plain' });

    uploadFile(input, file);

    await waitFor(() => {
      expect(screen.getByText('No time spans found when parsing EDL.')).toBeInTheDocument();
    });
  });

  it('dispatches comments when Add as comments is clicked', async () => {
    const onaddcomments = vi.fn();
    render(EDLImport, {
      props: { isOpen: true, onaddcomments }
    });
    const input = findFileInput();
    const file = new File([sampleEDL], 'test.edl', { type: 'text/plain' });

    uploadFile(input, file);

    await waitFor(() => {
      expect(screen.getByText('Add as comments')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('Add as comments'));

    expect(onaddcomments).toHaveBeenCalledOnce();
    const comments = onaddcomments.mock.calls[0][0];
    expect(comments).toHaveLength(2);
    expect(comments[0]).toMatchObject({
      mediaFileId: 'video-1',
      timecode: '00:00:00:00',
      comment: expect.stringContaining('Shot_01.mov'),
    });
    expect(comments[1]).toMatchObject({
      mediaFileId: 'video-1',
      timecode: '00:00:05:00',
      comment: expect.stringContaining('Shot_02.mov'),
    });
  });

  it('does not dispatch comments when no spans were parsed', async () => {
    const onaddcomments = vi.fn();
    render(EDLImport, {
      props: { isOpen: true, onaddcomments }
    });
    const input = findFileInput();
    const file = new File([invalidEDL], 'bad.edl', { type: 'text/plain' });

    uploadFile(input, file);

    await waitFor(() => {
      expect(screen.getByText('No time spans found when parsing EDL.')).toBeInTheDocument();
    });

    expect(screen.queryByText('Add as comments')).not.toBeInTheDocument();
    expect(onaddcomments).not.toHaveBeenCalled();
  });

  it('closes the dialog when Cancel is clicked', async () => {
    let isOpen = true;
    const { component } = render(EDLImport, {
      props: { isOpen, onaddcomments: vi.fn() }
    });
    const cancel = screen.getByText('Cancel');
    fireEvent.click(cancel);
    // The internal bindable prop should be updated to false
    expect(component.isOpen).toBe(false);
  });
});

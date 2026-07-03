/**
 * Player header-HTML glue: App.svelte's clapshot.openMediaFile(id, opts) header handling + version
 * switching. App isn't rendered in this suite (its websocket wiring is too heavy), so — as
 * App.protocol.test.ts does — we mirror that host function verbatim and assert the store
 * transitions + command shapes it produces.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import { playerHeaderHtml } from '@/stores';

// Verbatim copy of App.svelte's window.clapshot.openMediaFile.
function makeOpenMediaFile(wsEmit: (c: any) => void, videoPlayer: any) {
  return (mediaFileId: string, opts?: { headerHtml?: string; keepHTML?: boolean; preserveTime?: boolean }) => {
    if (!opts?.keepHTML) { playerHeaderHtml.set(opts?.headerHtml ?? null); }
    const time = (opts?.preserveTime && videoPlayer) ? videoPlayer.getCurTime() : 0;
    const wasPlaying = (opts?.preserveTime && videoPlayer) ? !videoPlayer.isPaused() : false;
    wsEmit({ openMediaFile: { mediaFileId } });
    if (opts?.preserveTime && videoPlayer) { videoPlayer.queueSeekOnLoad(time, wasPlaying); }
  };
}

const HEADER = '<select>…</select>';

beforeEach(() => playerHeaderHtml.set(null));

describe('openMediaFile header-HTML semantics', () => {
  it('sets / keeps / clears playerHeaderHtml; showPage clears it', () => {
    const emitted: any[] = [];
    const open = makeOpenMediaFile((c) => emitted.push(c), null);

    open('a', { headerHtml: HEADER });                 // set
    expect(get(playerHeaderHtml)).toBe(HEADER);
    expect(emitted.at(-1)).toEqual({ openMediaFile: { mediaFileId: 'a' } });

    open('b');                                         // plain open clears it
    expect(get(playerHeaderHtml)).toBeNull();

    playerHeaderHtml.set(HEADER);
    open('c', { keepHTML: true });                     // keep (switch within a header)
    expect(get(playerHeaderHtml)).toBe(HEADER);

    playerHeaderHtml.set(null);                        // showPage handler clears it (folder view)
    expect(get(playerHeaderHtml)).toBeNull();
  });
});

describe('version switch via header <select>', () => {
  it('keeps the header, preserves time/play state, and the organizer persists the new active version', () => {
    const emitted: any[] = [];
    const wsEmit = (c: any) => emitted.push(c);
    const queueSeekOnLoad = vi.fn();
    const videoPlayer = { getCurTime: () => 12.5, isPaused: () => false, queueSeekOnLoad };
    const open = makeOpenMediaFile(wsEmit, videoPlayer);

    playerHeaderHtml.set(HEADER);

    // what the organizer's <select> onchange does:
    open('b', { keepHTML: true, preserveTime: true });
    wsEmit({ organizerCmd: { cmd: 'set_active_version', args: JSON.stringify({ folder_id: 7, media_file_id: 'b' }) } });

    expect(get(playerHeaderHtml)).toBe(HEADER);                  // header preserved across the switch
    expect(queueSeekOnLoad).toHaveBeenCalledWith(12.5, true);   // captured timecode + play state
    expect(emitted[0]).toEqual({ openMediaFile: { mediaFileId: 'b' } });
    expect(emitted[1].organizerCmd.cmd).toBe('set_active_version');
    expect(JSON.parse(emitted[1].organizerCmd.args)).toEqual({ folder_id: 7, media_file_id: 'b' });
  });
});

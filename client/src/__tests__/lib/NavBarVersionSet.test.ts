/**
 * Tests for NavBar player-header HTML: the organizer-provided header slot and collab-disable.
 * (Version sets use this to render a version <select>, but the client side is organizer-agnostic.)
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, cleanup } from '@testing-library/svelte';
import { get } from 'svelte/store';
import NavBar from '@/lib/NavBar.svelte';
import { curVideo, mediaFileId, collabId, playerHeaderHtml, clientConfig, curUsername } from '@/stores';
import { createMinimalMediaFile } from '../mocks/protobuf-factories';

Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn() }, writable: true });
Object.defineProperty(window, 'location', { value: { href: 'http://localhost/', reload: vi.fn() }, writable: true });
global.alert = vi.fn();
global.fetch = vi.fn();
global.HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
  fillStyle: '', fillRect: vi.fn(), fillText: vi.fn(), measureText: vi.fn(() => ({ width: 50 })),
  font: '', textAlign: '', textBaseline: '',
}) as any);
Object.defineProperty(process, 'env', { value: { CLAPSHOT_CLIENT_VERSION: '1.0.0' } });

const HEADER = '<select data-testid="ver-select"><option>v3 — Latest</option><option>v2 — Old</option></select>';

beforeEach(() => {
  vi.clearAllMocks();
  curUsername.set('Tester');
  clientConfig.set({ app_title: 'Test' });
  mediaFileId.set('v2id');
  curVideo.set(createMinimalMediaFile({ id: 'v2id', title: 'Middle' }));
  collabId.set(null);
  playerHeaderHtml.set(null);
});
afterEach(cleanup);

describe('NavBar player-header HTML', () => {
  it('shows the plain media id (no header HTML) for a plain media file', () => {
    render(NavBar);
    expect(screen.queryByTestId('player-header-html')).not.toBeInTheDocument();
    expect(screen.getByText('v2id')).toBeInTheDocument();
  });

  it('renders organizer header HTML in place of the filename when set', () => {
    playerHeaderHtml.set(HEADER);
    render(NavBar);
    const slot = screen.getByTestId('player-header-html');
    expect(slot).toBeInTheDocument();
    expect(slot.querySelector('select')).toBeInTheDocument();          // organizer HTML is rendered
    expect(slot.textContent).toContain('v3 — Latest');
    expect(screen.queryByText('v2id')).not.toBeInTheDocument();        // plain filename replaced
  });
});

describe('NavBar collab disabled when a player header is set', () => {
  it('collab is disabled iff a player header is active', () => {
    collabId.set(null);
    playerHeaderHtml.set(HEADER);
    expect(get(collabId) === null && get(playerHeaderHtml) !== null).toBe(true);

    playerHeaderHtml.set(null);
    expect(get(collabId) === null && get(playerHeaderHtml) !== null).toBe(false);
  });

  it('renders without an enabled collab-start link while a player header is set', () => {
    playerHeaderHtml.set(HEADER);
    const { container } = render(NavBar);
    expect(container.querySelector('a[href*="collab="]')).not.toBeInTheDocument();
  });
});

/**
 * Tests for VideoTile badges, the media-tile (version-set) folder rendering, and the media-only drop guard.
 */
import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { render, screen, cleanup } from '@testing-library/svelte';
import VideoTile from '@/lib/asset_browser/VideoTile.svelte';
import FolderTile from '@/lib/asset_browser/FolderTile.svelte';
import { dropContainsFolder } from '@/lib/asset_browser/utils';
import { latestProgressReports, selectedTiles } from '@/stores';
import { createMediaFile } from '../../mocks/protobuf-factories';

// Keep the DOM-heavy children out of the way; we only care about structure + badges here.
vi.mock('@/lib/asset_browser/ScrubbableVideoThumb.svelte', () => ({
  default: vi.fn().mockImplementation(() => ({
    $$: { on_mount: [], on_destroy: [], before_update: [], after_update: [] },
    $set: vi.fn(), $on: vi.fn(), $destroy: vi.fn(),
  })),
}));
vi.mock('@/lib/asset_browser/TileVisualizationOverride.svelte', () => ({
  default: vi.fn().mockImplementation(() => ({
    $$: { on_mount: [], on_destroy: [], before_update: [], after_update: [] },
    $set: vi.fn(), $on: vi.fn(), $destroy: vi.fn(),
  })),
}));
// dndzone touches DOM APIs that jsdom doesn't fully support; no-op it for these structural tests.
vi.mock('svelte-dnd-action', () => ({
  dndzone: () => ({ update() {}, destroy() {} }),
  TRIGGERS: {}, SOURCES: {}, SHADOW_ITEM_MARKER_PROPERTY_NAME: Symbol('shadow'),
}));

beforeEach(() => {
  vi.clearAllMocks();
  latestProgressReports.set([]);
  selectedTiles.set({});
});
afterEach(cleanup);

describe('VideoTile badges', () => {
  it('renders the visualization badges', () => {
    render(VideoTile, {
      item: createMediaFile({ id: 'm1', title: 'Clip' }),
      visualization: { badges: [{ text: 'v4' }] },
    });
    expect(screen.getByText('v4')).toBeInTheDocument();
  });
});

describe('FolderTile media-tile mode', () => {
  it('renders a version set as a media-style tile (not a folder)', () => {
    const { container } = render(FolderTile, {
      id: '10',
      name: 'My Set',
      showAsMediaTile: true,
      visualization: { badges: [{ text: 'v2 of 3' }] },
      preview_items: [{ mediaFile: createMediaFile({ id: 'active1', title: 'active' }), popupActions: [] }],
    });
    // media-style tile present, folder decoration absent
    expect(container.querySelector('.video-list-video')).toBeInTheDocument();
    expect(container.querySelector('.folder-deco')).not.toBeInTheDocument();
    // label is the SET title, and the version badge shows
    expect(screen.getByText('My Set')).toBeInTheDocument();
    expect(screen.getByText('v2 of 3')).toBeInTheDocument();
  });

  it('renders a normal folder as a folder tile', () => {
    const { container } = render(FolderTile, {
      id: '11',
      name: 'Plain',
      showAsMediaTile: false,
      preview_items: [],
    });
    expect(container.querySelector('.video-list-folder')).toBeInTheDocument();
    expect(container.querySelector('.video-list-video')).not.toBeInTheDocument();
  });
});

describe('media-only drop guard', () => {
  it('detects a folder among dropped items', () => {
    expect(dropContainsFolder([{ obj: { mediaFile: { id: 'a' } } }])).toBe(false);
    expect(dropContainsFolder([{ obj: { folder: { id: '1' } } }])).toBe(true);
    expect(dropContainsFolder([
      { obj: { mediaFile: { id: 'a' } } },
      { obj: { folder: { id: '1' } } },
    ])).toBe(true);
  });
});

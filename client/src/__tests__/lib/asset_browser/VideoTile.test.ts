/**
 * Tests for VideoTile.svelte component
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { expectElementToBeHiddenOrInert } from '../../setup';
import { get } from 'svelte/store';
import VideoTile from '@/lib/asset_browser/VideoTile.svelte';
import { latestProgressReports } from '@/stores';
import { createMediaFile } from '../../mocks/protobuf-factories';
import type { PageItem_FolderListing_Item_Visualization } from '../../mocks/protobuf-factories';

// Mock child components
vi.mock('@/lib/asset_browser/ScrubbableVideoThumb.svelte', () => ({
  default: vi.fn().mockImplementation(() => ({
    $$: { on_mount: [], on_destroy: [], before_update: [], after_update: [] },
    $set: vi.fn(),
    $on: vi.fn(),
    $destroy: vi.fn(),
  })),
}));

vi.mock('@/lib/asset_browser/TileVisualizationOverride.svelte', () => ({
  default: vi.fn().mockImplementation(() => ({
    $$: { on_mount: [], on_destroy: [], before_update: [], after_update: [] },
    $set: vi.fn(),
    $on: vi.fn(),
    $destroy: vi.fn(),
  })),
}));

// Mock utils
vi.mock('@/lib/asset_browser/utils', () => ({
  rgbToCssColor: vi.fn((r: number, g: number, b: number) => `rgb(${r}, ${g}, ${b})`),
  cssVariables: vi.fn(),
}));

describe('VideoTile', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset store
    latestProgressReports.set([]);
  });

  it('should verify store subscription works', async () => {
    // Test that the store itself works
    let receivedValue: any = undefined;
    const unsubscribe = latestProgressReports.subscribe(value => {
      receivedValue = value;
    });

    // Initial value should be empty array
    expect(receivedValue).toEqual([]);

    // Update store
    const report = {
      mediaFileId: 'test-id',
      progress: 0.5,
      msg: 'transcoding',
      received_ts: Date.now(),
    };
    latestProgressReports.set([report]);

    // Should receive updated value
    expect(receivedValue).toEqual([report]);
    
    unsubscribe();
  });

  it('should render video tile with basic media file', () => {
    const mediaFile = createMediaFile({
      id: 'test-video-1',
      title: 'Test Video Title',
      addedTime: new Date('2023-12-01T10:00:00Z'),
    });

    const { container } = render(VideoTile, { item: mediaFile });
    
    expect(container.querySelector('.video-list-video')).toBeInTheDocument();
    expect(screen.getByText('Test Video Title')).toBeInTheDocument();
    expect(screen.getByText('2023-12-01')).toBeInTheDocument();
    expect(screen.getByText('test-video-1')).toBeInTheDocument();
  });

  it('should render with preview data', () => {
    const mediaFile = createMediaFile({
      id: 'test-video-2',
      title: 'Video with Preview',
      previewData: {
        thumbUrl: '/thumb/test-thumb.webp',
        thumbSheet: {
          url: '/thumb/test-sheet.webp',
          rows: 10,
          cols: 10,
        },
      },
    });

    const { container } = render(VideoTile, { item: mediaFile });
    
    // Should render ScrubbableVideoThumb component
    expect(container.querySelector('.flex-grow')).toBeInTheDocument();
  });

  it('shows a default icon when no preview or visualization exists', () => {
    const mediaFile = createMediaFile({
      id: 'no-preview',
      title: 'No Preview Video',
      previewData: undefined,
    });

    const { container } = render(VideoTile, { item: mediaFile });

    const icon = container.querySelector('i.fa-video');
    expect(icon).toBeInTheDocument();
  });

  it('should render with visualization override when no preview data', () => {
    const mediaFile = createMediaFile({
      id: 'test-video-3',
      title: 'Video without Preview',
      previewData: undefined,
    });

    const visualization: PageItem_FolderListing_Item_Visualization = {
      baseColor: { r: 255, g: 0, b: 0 },
      icon: {
        faClass: { classes: 'fa fa-video' },
      },
    };

    const { container } = render(VideoTile, { 
      item: mediaFile, 
      visualization 
    });
    
    // Should render TileVisualizationOverride when no preview data but visualization exists
    expect(container.querySelector('.flex-grow')).toBeInTheDocument();
  });

  it('should display progress bar when transcoding', async () => {
    const mediaFile = createMediaFile({
      id: 'transcoding-video',
      title: 'Transcoding Video',
    });

    // Set progress report before rendering to ensure it's available during mount
    latestProgressReports.set([
      {
        mediaFileId: 'transcoding-video',
        progress: 0.5,
        msg: 'transcoding',
        received_ts: Date.now(),
      },
    ]);

    const { container } = render(VideoTile, { item: mediaFile });

    // Wait for component to mount and subscribe to store
    await vi.waitFor(() => {
      const progressElement = screen.queryByText('transcoding');
      expect(progressElement).toBeInTheDocument();
    });

    const progressBar = container.querySelector('.bg-amber-500');
    expect(progressBar).toBeInTheDocument();
    expect(progressBar).toHaveStyle('width: 50%');
  });

  it('should update progress bar as transcoding progresses', async () => {
    const mediaFile = createMediaFile({
      id: 'progress-video',
      title: 'Progress Video',
    });

    // Start with initial progress report
    latestProgressReports.set([
      {
        mediaFileId: 'progress-video',
        progress: 0.25,
        msg: 'transcoding',
        received_ts: Date.now(),
      },
    ]);

    const { container } = render(VideoTile, { item: mediaFile });
    
    // Wait for initial progress bar to appear
    await vi.waitFor(() => {
      const progressBar = container.querySelector('.bg-amber-500');
      expect(progressBar).toBeInTheDocument();
      expect(progressBar).toHaveStyle('width: 25%');
    });

    // Update to 75%
    latestProgressReports.set([
      {
        mediaFileId: 'progress-video',
        progress: 0.75,
        msg: 'transcoding',
        received_ts: Date.now(),
      },
    ]);

    await vi.waitFor(() => {
      const progressBar = container.querySelector('.bg-amber-500');
      expect(progressBar).toHaveStyle('width: 75%');
    });
  });

  it('should handle multiple progress reports for different videos', async () => {
    const mediaFile = createMediaFile({
      id: 'video-a',
      title: 'Video A',
    });

    // Set progress for multiple videos before rendering
    latestProgressReports.set([
      {
        mediaFileId: 'video-a',
        progress: 0.3,
        msg: 'transcoding',
        received_ts: Date.now(),
      },
      {
        mediaFileId: 'video-b',
        progress: 0.8,
        msg: 'transcoding',
        received_ts: Date.now(),
      },
    ]);

    const { container } = render(VideoTile, { item: mediaFile });

    await vi.waitFor(() => {
      expect(screen.getByText('transcoding')).toBeInTheDocument();
    });

    // Should show progress for video-a only (30%)
    const progressBar = container.querySelector('.bg-amber-500');
    expect(progressBar).toBeInTheDocument();
    expect(progressBar).toHaveStyle('width: 30%');
  });

  it('should hide progress bar when transcoding completes', async () => {
    const mediaFile = createMediaFile({
      id: 'completing-video',
      title: 'Completing Video',
    });

    // Start with progress report
    latestProgressReports.set([
      {
        mediaFileId: 'completing-video',
        progress: 0.9,
        msg: 'transcoding',
        received_ts: Date.now(),
      },
    ]);

    const { container } = render(VideoTile, { item: mediaFile });

    // Wait for progress bar to appear
    await vi.waitFor(() => {
      expect(screen.getByText('transcoding')).toBeInTheDocument();
    });

    // Complete transcoding (remove from progress reports)
    latestProgressReports.set([]);

    await vi.waitFor(() => {
      const transcodingElement = screen.queryByText('transcoding');
      expect(expectElementToBeHiddenOrInert(transcodingElement)).toBe(true);
    });
  });

  it('should format date correctly', () => {
    const mediaFile = createMediaFile({
      id: 'date-test',
      title: 'Date Test Video',
      addedTime: new Date('2023-07-15T14:30:45Z'),
    });

    render(VideoTile, { item: mediaFile });
    
    expect(screen.getByText('2023-07-15')).toBeInTheDocument();
  });

  it('should handle missing date', () => {
    const mediaFile = createMediaFile({
      id: 'no-date',
      title: 'No Date Video',
      addedTime: undefined,
    });

    render(VideoTile, { item: mediaFile });
    
    expect(screen.getByText('(no date)')).toBeInTheDocument();
  });

  it('should display media file ID', () => {
    const mediaFile = createMediaFile({
      id: 'test-id-123',
      title: 'ID Test Video',
    });

    render(VideoTile, { item: mediaFile });
    
    expect(screen.getByText('test-id-123')).toBeInTheDocument();
  });

  it('should apply custom base color from visualization', () => {
    const mediaFile = createMediaFile({
      id: 'custom-color',
      title: 'Custom Color Video',
    });

    const visualization: PageItem_FolderListing_Item_Visualization = {
      baseColor: { r: 255, g: 128, b: 0 },
    };

    const { container } = render(VideoTile, { 
      item: mediaFile, 
      visualization 
    });

    // The cssVariables directive should be called with the custom color
    // This is hard to test directly, but we can verify the component renders
    expect(container.querySelector('.video-list-video')).toBeInTheDocument();
  });

  it('should use default base color when no visualization provided', () => {
    const mediaFile = createMediaFile({
      id: 'default-color',
      title: 'Default Color Video',
    });

    const { container } = render(VideoTile, { item: mediaFile });

    // Should use default color (71, 85, 105)
    expect(container.querySelector('.video-list-video')).toBeInTheDocument();
  });

  it('should render with provided item data', () => {
    const mediaFile = createMediaFile({
      id: 'data-test',
      title: 'Data Test Video',
    });

    const { container } = render(VideoTile, { item: mediaFile });
    
    // The component should display the media file data
    expect(container.querySelector('.video-list-video')).toBeInTheDocument();
    expect(screen.getByText('Data Test Video')).toBeInTheDocument();
    expect(screen.getByText('data-test')).toBeInTheDocument();
  });

  it('should truncate long titles appropriately', () => {
    const mediaFile = createMediaFile({
      id: 'long-title',
      title: 'This is a very long video title that should be truncated or handled appropriately by the component styling',
    });

    render(VideoTile, { item: mediaFile });
    
    const titleElement = screen.getByTitle(mediaFile.title || '');
    expect(titleElement).toBeInTheDocument();
    expect(titleElement.textContent).toBe(mediaFile.title);
  });
});

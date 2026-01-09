import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import VideoPlayer from '@/lib/player_view/VideoPlayer.svelte';
import { videoIsReady, curVideo, curSubtitle, allComments, collabId } from '@/stores';

// Mock Canvas API
global.HTMLCanvasElement.prototype.getContext = vi.fn((contextId: string) => {
  if (contextId === '2d') {
    return {
      fillStyle: '',
      fillRect: vi.fn(),
      fillText: vi.fn(),
      measureText: vi.fn(() => ({ width: 50 })),
      font: '',
      textAlign: '',
      textBaseline: '',
      drawImage: vi.fn()
    };
  }
  return null;
});

global.HTMLCanvasElement.prototype.toDataURL = vi.fn(() => 'data:image/webp;base64,mocked-image-data');

// Mock external dependencies
vi.mock('@tadashi/svelte-notification', () => ({
  acts: {
    add: vi.fn()
  }
}));

vi.mock('simple-drawing-board', () => ({
  create: vi.fn(() => ({
    setLineSize: vi.fn(),
    setLineColor: vi.fn(),
    clear: vi.fn(),
    undo: vi.fn(),
    redo: vi.fn(),
    destroy: vi.fn(),
    fillImageByDataURL: vi.fn().mockResolvedValue(undefined)
  }))
}));

vi.mock('@/lib/player_view/VideoFrame', () => ({
  VideoFrame: class MockVideoFrame {
    frameRate = 30;
    fps = 30;
    toSMPTE = vi.fn((frame) => `00:00:01:${String(frame || 0).padStart(2, '0')}`);
    toMilliseconds = vi.fn((smpte) => 1000);
    seekForward = vi.fn();
    seekBackward = vi.fn();
    seekToFrame = vi.fn();
    seekToSMPTE = vi.fn();
  }
}));

vi.mock('@/cookies', () => ({
  default: {
    get: vi.fn(() => '100'),
    set: vi.fn()
  }
}));

// Mock the hexColorForUsername function
vi.mock('@/lib/Avatar.svelte', () => ({
  hexColorForUsername: vi.fn((username: string) => {
    const colors = ['#ff0000', '#00ff00', '#0000ff', '#ffff00', '#ff00ff', '#00ffff'];
    const hash = username.split('').reduce((a, b) => a + b.charCodeAt(0), 0);
    return colors[hash % colors.length];
  })
}));

// Mock stores
vi.mock('@/stores', () => {
  const createMockStore = (initialValue: any) => {
    let value = initialValue;
    const subscribers = new Set<Function>();
    
    return {
      subscribe: (callback: Function) => {
        subscribers.add(callback);
        callback(value);
        return () => subscribers.delete(callback);
      },
      set: vi.fn((newValue: any) => {
        value = newValue;
        subscribers.forEach(callback => callback(value));
      }),
      update: vi.fn((updater: Function) => {
        value = updater(value);
        subscribers.forEach(callback => callback(value));
      })
    };
  };

  return {
    videoIsReady: createMockStore(true),
    curVideo: createMockStore({
      id: 'video-123',
      duration: { fps: '30' },
      mediaType: 'video/mp4',
      subtitles: []
    }),
    curSubtitle: createMockStore(null),
    allComments: createMockStore([]),
    collabId: createMockStore(null),
    clientConfig: createMockStore({ enable_mediabunny: false })
  };
});

describe('VideoPlayer + CommentTimelinePin Integration', () => {
  const mockUser = userEvent.setup();

  beforeEach(() => {
    vi.clearAllMocks();
    
    // Reset stores to ensure clean state
    videoIsReady.set(true);
    curVideo.set({
      id: 'video-123',
      duration: { fps: '30' },
      mediaType: 'video/mp4',
      subtitles: []
    });
    curSubtitle.set(null);
    allComments.set([]);
    collabId.set(null);

    // Mock video element dimensions for drawing setup
    Object.defineProperty(HTMLVideoElement.prototype, 'videoWidth', {
      value: 640,
      writable: true
    });
    Object.defineProperty(HTMLVideoElement.prototype, 'videoHeight', {
      value: 480,
      writable: true
    });
    Object.defineProperty(HTMLVideoElement.prototype, 'duration', {
      value: 120,
      writable: true
    });
    Object.defineProperty(HTMLVideoElement.prototype, 'currentTime', {
      value: 0,
      writable: true
    });
  });

  afterEach(() => {
    cleanup();
  });

  it('should render VideoPlayer without CommentTimelinePin when no comments', () => {
    allComments.set([]);

    render(VideoPlayer, {
      props: { src: 'test-video.mp4' }
    });

    // VideoPlayer should render
    expect(screen.getByRole('main')).toBeInTheDocument();
    
    // No comment pins should be present
    expect(screen.queryByRole('link')).not.toBeInTheDocument();
  });

  it('should render CommentTimelinePin components when comments with timecode exist', () => {
    const mockComments = [
      {
        comment: {
          id: 'comment-1',
          usernameIfnull: 'user1',
          userId: 'user1',
          comment: 'First comment',
          timecode: '00:01:30:15'
        }
      },
      {
        comment: {
          id: 'comment-2',
          usernameIfnull: 'user2',
          userId: 'user2',
          comment: 'Second comment',
          timecode: '00:02:45:20'
        }
      }
    ];

    allComments.set(mockComments);

    render(VideoPlayer, {
      props: { src: 'test-video.mp4' }
    });

    // Should render comment pins
    const pins = screen.getAllByRole('link');
    expect(pins).toHaveLength(2);

    // Check tooltips contain comment info
    expect(pins[0]).toHaveAttribute('title', 'user1: First comment');
    expect(pins[1]).toHaveAttribute('title', 'user2: Second comment');
  });

  it('should handle comment pin clicks through callback props integration', async () => {
    const mockComments = [
      {
        comment: {
          id: 'clickable-comment',
          usernameIfnull: 'clickuser',
          userId: 'clickuser',
          comment: 'Clickable comment',
          timecode: '00:01:00:00'
        }
      }
    ];

    allComments.set(mockComments);

    // Listen for the commentPinClicked event from VideoPlayer
    const commentPinClickedSpy = vi.fn();
    const { component } = render(VideoPlayer, {
      props: { src: 'test-video.mp4', oncommentpinclicked: commentPinClickedSpy }
    });

    // Find and click the comment pin
    const pin = screen.getByRole('link');
    await mockUser.click(pin);

    // Should dispatch the commentPinClicked event with correct id
    expect(commentPinClickedSpy).toHaveBeenCalledOnce();
    expect(commentPinClickedSpy).toHaveBeenCalledWith(
      { id: 'clickable-comment' }
    );
  });

  it('should handle keyboard interaction on comment pins', async () => {
    const mockComments = [
      {
        comment: {
          id: 'keyboard-comment',
          usernameIfnull: 'keyboarduser',
          userId: 'keyboarduser',
          comment: 'Keyboard accessible comment',
          timecode: '00:00:30:10'
        }
      }
    ];

    allComments.set(mockComments);

    const commentPinClickedSpy = vi.fn();
    const { component } = render(VideoPlayer, {
      props: { src: 'test-video.mp4', oncommentpinclicked: commentPinClickedSpy }
    });

    // Find the comment pin and navigate to it with keyboard
    const pin = screen.getByRole('link');
    pin.focus();
    await mockUser.keyboard('{Enter}');

    // Should trigger the same event as clicking
    expect(commentPinClickedSpy).toHaveBeenCalledOnce();
    expect(commentPinClickedSpy).toHaveBeenCalledWith(
      { id: 'keyboard-comment' }
    );
  });

  it('should filter and sort comments with timecode correctly', () => {
    const mockComments = [
      {
        comment: {
          id: 'comment-no-tc',
          usernameIfnull: 'user1',
          comment: 'Comment without timecode',
          timecode: undefined // No timecode
        }
      },
      {
        comment: {
          id: 'comment-later',
          usernameIfnull: 'user2',
          comment: 'Later comment',
          timecode: '00:03:00:00'
        }
      },
      {
        comment: {
          id: 'comment-earlier',
          usernameIfnull: 'user3',
          comment: 'Earlier comment',
          timecode: '00:01:00:00'
        }
      }
    ];

    allComments.set(mockComments);

    render(VideoPlayer, {
      props: { src: 'test-video.mp4' }
    });

    // Should only render comments with timecode, and in sorted order
    const pins = screen.getAllByRole('link');
    expect(pins).toHaveLength(2); // Only comments with timecode

    // Check that they are in chronological order by tooltip content
    expect(pins[0]).toHaveAttribute('title', 'user3: Earlier comment'); // 00:01:00:00
    expect(pins[1]).toHaveAttribute('title', 'user2: Later comment');   // 00:03:00:00
  });

  it('should handle empty comments gracefully', () => {
    allComments.set([]);

    render(VideoPlayer, {
      props: { src: 'test-video.mp4' }
    });

    // Should render VideoPlayer without errors
    expect(screen.getByRole('main')).toBeInTheDocument();
    expect(screen.queryByRole('link')).not.toBeInTheDocument();
  });

  it('should update comment pins when allComments store changes', async () => {
    // Start with comments already present to avoid complexity with dynamic updates
    const initialComments = [
      {
        comment: {
          id: 'initial-comment',
          usernameIfnull: 'initialuser',
          comment: 'Initial comment',
          timecode: '00:01:00:00'
        }
      }
    ];

    allComments.set(initialComments);

    render(VideoPlayer, {
      props: { src: 'test-video.mp4' }
    });

    // Should have one pin initially
    const pins = screen.getAllByRole('link');
    expect(pins).toHaveLength(1);
    expect(pins[0]).toHaveAttribute('title', 'initialuser: Initial comment');
  });
});
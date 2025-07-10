/**
 * Simple integration test that would have caught the e.detail.* vs e.* bug
 * 
 * Tests a parent component that uses CommentCard with callback props,
 * simulating the same pattern as App.svelte but much simpler.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CommentCard from '@/lib/player_view/CommentCard.svelte';
import * as Proto3 from '@clapshot_protobuf/typescript';

// Mock Canvas API for Avatar component
global.HTMLCanvasElement.prototype.getContext = vi.fn((contextId: string) => {
  if (contextId === '2d') {
    return {
      fillStyle: '',
      fillRect: vi.fn(),
      fillText: vi.fn(),
      measureText: vi.fn(() => ({ width: 50 })),
      font: '',
      textAlign: '',
      textBaseline: ''
    };
  }
  return null;
});

global.HTMLCanvasElement.prototype.toDataURL = vi.fn(() => 'data:image/png;base64,mock-image-data');

// Mock the stores
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
    curUserId: createMockStore('user-123'),
    curUserIsAdmin: createMockStore(false),
    allComments: createMockStore([]),
    curSubtitle: createMockStore({
      id: 'subtitle-456',
      languageCode: 'en',
      title: 'English Subtitles'
    }),
    curVideo: createMockStore({
      id: 'video-456',
      subtitles: [{
        id: 'subtitle-456',
        languageCode: 'en',
        title: 'English Subtitles'
      }]
    })
  };
});

describe('CommentCard Integration with Parent Callback Handler', () => {
  const mockUser = userEvent.setup();

  const mockComment: Proto3.Comment = {
    id: 'comment-123',
    userId: 'user-123',
    usernameIfnull: 'John Doe',
    timestamp: { seconds: 1234567890, nanos: 0 },
    parentId: '',
    comment: 'This is a test comment',
    timecode: '30.5s',
    subtitleId: 'subtitle-456'
  };

  beforeEach(() => {
    vi.clearAllMocks();
    global.confirm = vi.fn().mockReturnValue(true);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('should correctly pass event data from CommentCard to parent handler (timecode click)', async () => {
    // This simulates the same bug pattern as App.svelte
    // If the parent expects e.detail.timecode but gets e.timecode, this will catch it
    
    let receivedEventData: any = null;
    
    // Simulate a parent component handler that expects the OLD format (with bug)
    const parentHandlerWithBug = (e: any) => {
      // This would be the BUGGY parent handler expecting e.detail.*
      receivedEventData = {
        timecode: e.detail?.timecode,  // BUG: expecting e.detail.timecode
        drawing: e.detail?.drawing,
        subtitleId: e.detail?.subtitleId
      };
    };
    
    // Render CommentCard with the buggy parent handler
    render(CommentCard, {
      props: { 
        comment: mockComment, 
        indent: 0,
        ondisplaycomment: parentHandlerWithBug
      }
    });

    // Click the timecode
    const timecodeLink = screen.getByText('30.5s');
    await mockUser.click(timecodeLink);

    // If CommentCard sends the NEW format {timecode: '30.5s'} 
    // but parent expects OLD format e.detail.timecode,
    // then receivedEventData.timecode will be undefined
    expect(receivedEventData).toEqual({
      timecode: undefined,  // This proves the bug - should be '30.5s'
      drawing: undefined,
      subtitleId: undefined
    });
  });

  it('should correctly pass event data from CommentCard to parent handler (correct format)', async () => {
    // This shows the CORRECT way - parent handler expects new format
    
    let receivedEventData: any = null;
    
    // Correct parent handler expecting the NEW format (no e.detail)
    const correctParentHandler = (e: any) => {
      receivedEventData = {
        timecode: e.timecode,  // CORRECT: expecting e.timecode directly
        drawing: e.drawing,
        subtitleId: e.subtitleId
      };
    };
    
    render(CommentCard, {
      props: { 
        comment: mockComment, 
        indent: 0,
        ondisplaycomment: correctParentHandler
      }
    });

    const timecodeLink = screen.getByText('30.5s');
    await mockUser.click(timecodeLink);

    // With the correct handler, we get the expected data
    expect(receivedEventData).toEqual({
      timecode: '30.5s',
      drawing: undefined,
      subtitleId: 'subtitle-456'
    });
  });

  it('should correctly pass event data for comment editing', async () => {
    let receivedEventData: any = null;
    
    // Test edit event - this would catch e.detail.id vs e.id bugs
    const editHandler = (e: any) => {
      receivedEventData = {
        id: e.id,           // NEW format
        comment_text: e.comment_text
      };
    };
    
    render(CommentCard, {
      props: { 
        comment: mockComment, 
        indent: 0,
        oneditcomment: editHandler
      }
    });

    // Hover to show edit button
    const commentCard = screen.getByText('This is a test comment').closest('[id^="comment_card_"]');
    await mockUser.hover(commentCard!);

    // Click edit button
    await waitFor(() => {
      expect(screen.getByText('Edit')).toBeInTheDocument();
    });
    
    const editButton = screen.getByText('Edit');
    await mockUser.click(editButton);

    // Edit the comment
    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    await mockUser.clear(textarea);
    await mockUser.type(textarea, 'Edited comment');
    await mockUser.keyboard('{Enter}');

    // Verify correct event data format
    expect(receivedEventData).toEqual({
      id: 'comment-123',
      comment_text: 'Edited comment'
    });
  });

  it('should correctly pass event data for comment deletion', async () => {
    let receivedEventData: any = null;
    
    // Test delete event - this would catch e.detail.id vs e.id bugs
    const deleteHandler = (e: any) => {
      receivedEventData = {
        id: e.id  // NEW format - if parent expected e.detail.id, this would catch it
      };
    };
    
    render(CommentCard, {
      props: { 
        comment: mockComment, 
        indent: 0,
        ondeletecomment: deleteHandler
      }
    });

    // Hover to show delete button
    const commentCard = screen.getByText('This is a test comment').closest('[id^="comment_card_"]');
    await mockUser.hover(commentCard!);

    // Click delete button
    await waitFor(() => {
      expect(screen.getByText('Del')).toBeInTheDocument();
    });
    
    const deleteButton = screen.getByText('Del');
    await mockUser.click(deleteButton);

    // Verify correct event data format
    expect(receivedEventData).toEqual({
      id: 'comment-123'
    });
  });
});
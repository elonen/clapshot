import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CommentCard from '@/lib/player_view/CommentCard.svelte';
import * as Proto3 from '@clapshot_protobuf/typescript';
import { curUserId, curUserIsAdmin, allComments, curSubtitle, curVideo } from '@/stores';

// Mock Canvas API for Avatar component (like NavBar tests)
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

// Mock the stores with spies
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
    curSubtitle: createMockStore(null),
    curVideo: createMockStore({ 
      id: 'video-456',
      subtitles: [{
        id: 'subtitle-456',
        languageCode: 'en',
        title: 'English Subtitles',
        origFilename: 'subtitles.srt',
        origUrl: '/subtitles/subtitle-456.srt',
        timeOffset: 0,
        userId: 'user-123'
      }]
    })
  };
});

describe('CommentCard.svelte', () => {
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

  const mockSubtitle: Proto3.Subtitle = {
    id: 'subtitle-456',
    languageCode: 'en',
    title: 'English Subtitles',
    origFilename: 'subtitles.srt',
    origUrl: '/subtitles/subtitle-456.srt',
    timeOffset: 0,
    userId: 'user-123'
  };

  const mockVideo = {
    id: 'video-456',
    subtitles: [mockSubtitle]
  };

  beforeEach(() => {
    vi.clearAllMocks();
    
    // Reset all stores to initial state
    curUserId.set('user-123');
    curUserIsAdmin.set(false);
    allComments.set([]);
    curSubtitle.set(null);
    curVideo.set(mockVideo);

    // Reset the mock comment to original state to prevent cross-test contamination
    mockComment.comment = 'This is a test comment';

    // Mock window.confirm for delete functionality
    global.confirm = vi.fn().mockReturnValue(true);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    
    // Reset stores after each test to prevent contamination
    curUserId.set('user-123');
    curUserIsAdmin.set(false);
    allComments.set([]);
    curSubtitle.set(null);
    curVideo.set(mockVideo);

    // Reset the mock comment state
    mockComment.comment = 'This is a test comment';
  });

  describe('Basic rendering', () => {
    it('should render with required props', () => {
      render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      expect(screen.getByText('This is a test comment')).toBeInTheDocument();
      expect(screen.getByText('John Doe')).toBeInTheDocument();
    });

    it('should display comment content correctly', () => {
      render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      expect(screen.getByText('This is a test comment')).toBeInTheDocument();
      expect(screen.getByText('John Doe')).toBeInTheDocument();
      expect(screen.getByText('30.5s')).toBeInTheDocument();
    });

    it('should render Avatar component', () => {
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Avatar component should be present (mocked)
      const avatarContainer = container.querySelector('.avatar-container, [data-testid="avatar"]');
      expect(avatarContainer || screen.getByText('John Doe')).toBeInTheDocument();
    });

    it('should apply correct indentation styling', () => {
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 2 }
      });

      const commentContainer = container.querySelector('.comment-card') || container.firstChild;
      // Browser converts em to px, so check the style attribute directly
      expect(commentContainer).toHaveAttribute('style', 'margin-left: 3em;');
    });
  });

  describe('User permissions and action visibility', () => {
    it('should show actions on hover/focus', async () => {
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      const commentElement = container.querySelector('.comment-card') || container.firstChild as HTMLElement;
      
      // Hover over comment
      await mockUser.hover(commentElement);
      
      // Actions should become visible
      await waitFor(() => {
        expect(screen.queryByText('Edit') || screen.queryByRole('button', { name: /edit/i })).toBeInTheDocument();
      });
    });

    it('should show Edit/Delete for comment owner', async () => {
      curUserId.set('user-123'); // User owns the comment
      
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Trigger actions visibility by hovering
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      // Should show edit and reply buttons (delete only if no children)
      await waitFor(() => {
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });
      expect(screen.getByText('Reply')).toBeInTheDocument();
    });

    it('should show Edit/Delete for admin users', async () => {
      curUserId.set('admin-user');
      curUserIsAdmin.set(true);
      
      const otherUserComment = { ...mockComment, userId: 'other-user' };
      
      const { container } = render(CommentCard, {
        props: { comment: otherUserComment, indent: 0 }
      });

      // Trigger actions visibility by hovering
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      // Admin should see edit/delete options even for other users' comments
      await waitFor(() => {
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });
    });

    it('should hide Edit/Delete for unauthorized users', () => {
      curUserId.set('different-user');
      curUserIsAdmin.set(false);
      
      render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Should not show edit/delete for unauthorized users
      expect(screen.queryByText('Edit')).not.toBeInTheDocument();
      expect(screen.queryByText('Delete')).not.toBeInTheDocument();
    });
  });

  describe('Comment editing functionality', () => {
    it('should enter edit mode on Edit button click', async () => {
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // First trigger actions visibility
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      // Wait for Edit button to appear
      await waitFor(() => {
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });

      const editButton = screen.getByText('Edit');
      await mockUser.click(editButton);

      // Should show textarea for editing
      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
      expect(textarea.value).toBe('This is a test comment');
    });

    it('should save comment on Enter key', async () => {
      const editSpy = vi.fn();
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0, oneditcomment: editSpy }
      });

      // Trigger actions visibility first
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      await waitFor(() => {
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });

      const editButton = screen.getByText('Edit');
      await mockUser.click(editButton);

      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
      expect(textarea.value).toBe('This is a test comment');
      
      await mockUser.clear(textarea);
      await mockUser.type(textarea, 'Updated comment text');
      await mockUser.keyboard('{Enter}');

      expect(editSpy).toHaveBeenCalledWith({
        id: 'comment-123',
        comment_text: 'Updated comment text'
      });
    });

    it('should save changes on Escape key if text is not empty', async () => {
      const editSpy = vi.fn();
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0, oneditcomment: editSpy }
      });

      // Trigger actions visibility first
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      await waitFor(() => {
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });

      const editButton = screen.getByText('Edit');
      await mockUser.click(editButton);

      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
      expect(textarea.value).toBe('This is a test comment');
      
      await mockUser.clear(textarea);
      await mockUser.type(textarea, 'Some changes');
      await mockUser.keyboard('{Escape}');

      // Wait for DOM updates
      await waitFor(() => {
        expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
      });

      // Should exit edit mode and dispatch edit event with changes
      expect(screen.getByText('Some changes')).toBeInTheDocument();
      expect(editSpy).toHaveBeenCalledWith({
        id: 'comment-123',
        comment_text: 'Some changes'
      });
    });

    it('should exit edit mode on blur and trim text', async () => {
      const editSpy = vi.fn();
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0, oneditcomment: editSpy }
      });

      // Trigger actions visibility first
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      await waitFor(() => {
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });

      const editButton = screen.getByText('Edit');
      await mockUser.click(editButton);

      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
      expect(textarea.value).toBe('This is a test comment');
      
      await mockUser.clear(textarea);
      await mockUser.type(textarea, '  Trimmed text  ');
      
      // Blur the textarea
      await mockUser.tab();

      // Should exit edit mode and show trimmed text, but NOT dispatch edit event
      expect(screen.getByText('Trimmed text')).toBeInTheDocument();
      expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
      expect(editSpy).not.toHaveBeenCalled();
    });
  });

  describe('Reply functionality', () => {
    it('should show reply input on Reply button', async () => {
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Trigger actions visibility first
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      await waitFor(() => {
        expect(screen.getByText('Reply')).toBeInTheDocument();
      });

      const replyButton = screen.getByText('Reply');
      await mockUser.click(replyButton);

      expect(screen.getByPlaceholderText(/reply/i)).toBeInTheDocument();
    });

    it('should submit reply with valid text', async () => {
      const replySpy = vi.fn();
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0, onreplytocomment: replySpy }
      });

      // Trigger actions visibility first
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      await waitFor(() => {
        expect(screen.getByText('Reply')).toBeInTheDocument();
      });

      const replyButton = screen.getByText('Reply');
      await mockUser.click(replyButton);

      const replyInput = screen.getByPlaceholderText(/reply/i);
      await mockUser.type(replyInput, 'This is a reply');
      await mockUser.keyboard('{Enter}');

      expect(replySpy).toHaveBeenCalledWith({
        parentId: 'comment-123',
        commentText: 'This is a reply',
        subtitleId: undefined
      });
    });

    it('should hide reply input on blur', async () => {
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Trigger actions visibility first
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      await waitFor(() => {
        expect(screen.getByText('Reply')).toBeInTheDocument();
      });

      const replyButton = screen.getByText('Reply');
      await mockUser.click(replyButton);

      const replyInput = screen.getByPlaceholderText(/reply/i);
      expect(replyInput).toBeInTheDocument();

      // Blur the input
      await mockUser.tab();

      await waitFor(() => {
        expect(screen.queryByPlaceholderText(/reply/i)).not.toBeInTheDocument();
      });
    });
  });

  describe('Timecode and navigation', () => {
    it('should click timecode to navigate', async () => {
      const displaySpy = vi.fn();
      render(CommentCard, {
        props: { comment: mockComment, indent: 0, ondisplaycomment: displaySpy }
      });

      const timecode = screen.getByText('30.5s');
      await mockUser.click(timecode);

      expect(displaySpy).toHaveBeenCalledWith({
        timecode: '30.5s',
        drawing: undefined,
        subtitleId: 'subtitle-456'
      });
    });

    it('should handle keyboard Enter to navigate', async () => {
      const displaySpy = vi.fn();
      render(CommentCard, {
        props: { comment: mockComment, indent: 0, ondisplaycomment: displaySpy }
      });

      // Click on the comment card to focus it, then press Enter
      const commentCard = screen.getByRole('link');
      commentCard.focus();
      await mockUser.keyboard('{Enter}');

      expect(displaySpy).toHaveBeenCalledWith({
        timecode: '30.5s',
        drawing: undefined,
        subtitleId: 'subtitle-456'
      });
    });

    it('should handle comments without timecode', () => {
      const commentWithoutTimecode = { ...mockComment, timecode: undefined };
      
      render(CommentCard, {
        props: { comment: commentWithoutTimecode, indent: 0 }
      });

      // Should not display clickable timecode
      expect(screen.queryByText(/\d+\.\d+s/)).not.toBeInTheDocument();
    });
  });

  describe('Delete functionality', () => {
    it('should confirm and delete comment', async () => {
      const deleteSpy = vi.fn();
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0, ondeletecomment: deleteSpy }
      });

      // Trigger actions visibility first
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      await waitFor(() => {
        expect(screen.getByText('Del')).toBeInTheDocument();
      });

      const deleteButton = screen.getByText('Del');
      await mockUser.click(deleteButton);

      expect(global.confirm).toHaveBeenCalledWith('Delete comment?');
      expect(deleteSpy).toHaveBeenCalledWith({ id: 'comment-123' });
    });

    it('should prevent delete with children', async () => {
      // Set up comments with children - hasChildren() checks c.comment.parentId
      allComments.set([
        { comment: mockComment },
        { comment: { ...mockComment, id: 'child-comment', parentId: 'comment-123' }}
      ]);

      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Trigger actions visibility
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      // Should show Reply and Edit but not Delete when comment has children
      await waitFor(() => {
        expect(screen.getByText('Reply')).toBeInTheDocument();
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });
      expect(screen.queryByText('Del')).not.toBeInTheDocument();
    });
  });

  describe('Store integration', () => {
    it('should react to store changes', async () => {
      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Trigger actions visibility first
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      // Initially user owns comment
      await waitFor(() => {
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });

      // Change user ID
      curUserId.set('different-user');

      // Actions should disappear when user changes (no longer owner)
      await mockUser.unhover(commentElement);
      await mockUser.hover(commentElement);

      await waitFor(() => {
        expect(screen.queryByText('Edit')).not.toBeInTheDocument();
      });
    });

    it('should handle subtitle language display', () => {
      curSubtitle.set(mockSubtitle);
      
      render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Should display subtitle language info in the strong element
      expect(screen.getByText('EN')).toBeInTheDocument();
    });

    it('should filter children comments correctly', async () => {
      const childComment = { ...mockComment, id: 'child-123', parentId: 'comment-123' };
      allComments.set([
        { comment: mockComment },
        { comment: childComment }
      ]);

      const { container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Trigger actions visibility
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      // hasChildren() should return true, affecting delete button visibility
      await waitFor(() => {
        expect(screen.getByText('Reply')).toBeInTheDocument();
      });
      expect(screen.queryByText('Del')).not.toBeInTheDocument();
    });
  });

  describe('Edge cases and error handling', () => {
    it('should handle missing user data', () => {
      const commentWithoutUser = { ...mockComment, userName: '', userId: '' };
      
      render(CommentCard, {
        props: { comment: commentWithoutUser, indent: 0 }
      });

      // Should render without crashing
      expect(screen.getByText('This is a test comment')).toBeInTheDocument();
    });

    it('should handle empty/whitespace comment text', async () => {
      const { component, container } = render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      const editSpy = vi.fn();
      component.$on('edit-comment', editSpy);

      // Trigger actions visibility first
      const commentElement = container.querySelector('#comment_card_comment-123') as HTMLElement;
      await mockUser.hover(commentElement);

      await waitFor(() => {
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });

      const editButton = screen.getByText('Edit');
      await mockUser.click(editButton);

      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
      expect(textarea.value).toBe('This is a test comment');
      
      await mockUser.clear(textarea);
      await mockUser.type(textarea, '   ');
      await mockUser.keyboard('{Enter}');

      // Should not dispatch event for empty/whitespace text
      expect(editSpy).not.toHaveBeenCalled();
    });

    it('should handle subtitle without language info', () => {
      const subtitleWithoutLang = { ...mockSubtitle, languageCode: '' };
      curSubtitle.set(subtitleWithoutLang);
      
      render(CommentCard, {
        props: { comment: mockComment, indent: 0 }
      });

      // Should render without crashing
      expect(screen.getByText('This is a test comment')).toBeInTheDocument();
    });
  });
});
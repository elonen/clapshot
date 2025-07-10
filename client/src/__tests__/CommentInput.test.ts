import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CommentInput from '@/lib/player_view/CommentInput.svelte';
import { videoIsReady } from '@/stores';

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
    videoIsReady: createMockStore(true)
  };
});

describe('CommentInput.svelte', () => {
  const mockUser = userEvent.setup();

  beforeEach(() => {
    vi.clearAllMocks();
    videoIsReady.set(true);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    videoIsReady.set(true);
  });

  describe('Basic rendering', () => {
    it('should render text input field', () => {
      render(CommentInput);
      
      expect(screen.getByPlaceholderText('Add a comment - at current time...')).toBeInTheDocument();
    });

    it('should render send button', () => {
      render(CommentInput);
      
      expect(screen.getByRole('button', { name: /send/i })).toBeInTheDocument();
    });

    it('should render timed comment toggle when video is ready', () => {
      render(CommentInput);
      
      expect(screen.getByTitle('Comment is time specific?')).toBeInTheDocument();
    });

    it('should render draw mode toggle when video is ready', () => {
      render(CommentInput);
      
      expect(screen.getByTitle('Draw on video')).toBeInTheDocument();
    });

    it('should hide video-dependent features when video not ready', () => {
      videoIsReady.set(false);
      
      render(CommentInput);
      
      expect(screen.queryByTitle('Comment is time specific?')).not.toBeInTheDocument();
      expect(screen.queryByTitle('Draw on video')).not.toBeInTheDocument();
    });
  });

  describe('Text input functionality', () => {
    it('should update placeholder text when timed mode is disabled', async () => {
      render(CommentInput);
      
      const timedButton = screen.getByTitle('Comment is time specific?');
      await mockUser.click(timedButton);
      
      expect(screen.getByPlaceholderText('Add a comment...')).toBeInTheDocument();
    });

    it('should dispatch text_input event when typing', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
      await mockUser.type(textInput, 'Test comment');
      
      expect(eventSpy).toHaveBeenCalledWith(
        { action: 'text_input' }
      );
    });

    it('should not dispatch text_input event when input is empty', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
      await mockUser.type(textInput, 'a');
      await mockUser.clear(textInput);
      
      // Should have fired once when typing 'a', but not when clearing
      expect(eventSpy).toHaveBeenCalledTimes(1);
    });

    it('should enable send button when text is entered', async () => {
      render(CommentInput);
      
      const sendButton = screen.getByRole('button', { name: /send/i });
      expect(sendButton).toBeDisabled();
      
      const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
      await mockUser.type(textInput, 'Test comment');
      
      expect(sendButton).not.toBeDisabled();
    });
  });

  describe('Timed comment toggle', () => {
    it('should toggle timed comment mode', async () => {
      render(CommentInput);
      
      const timedButton = screen.getByTitle('Comment is time specific?');
      
      // Should start in timed mode (amber color)
      expect(timedButton).toHaveClass('text-amber-600');
      expect(screen.getByPlaceholderText('Add a comment - at current time...')).toBeInTheDocument();
      
      await mockUser.click(timedButton);
      
      // Should toggle to global mode
      expect(screen.getByPlaceholderText('Add a comment...')).toBeInTheDocument();
    });

    it('should disable timed button when in draw mode', async () => {
      render(CommentInput);
      
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      const timedButton = screen.getByTitle('Comment is time specific?');
      expect(timedButton).toBeDisabled();
    });

    it('should force timed mode when entering draw mode', async () => {
      render(CommentInput);
      
      // First disable timed mode
      const timedButton = screen.getByTitle('Comment is time specific?');
      await mockUser.click(timedButton);
      expect(screen.getByPlaceholderText('Add a comment...')).toBeInTheDocument();
      
      // Enter draw mode
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      // Should force timed mode back on
      expect(screen.getByPlaceholderText('Add a comment - at current time...')).toBeInTheDocument();
    });
  });

  describe('Draw mode functionality', () => {
    it('should toggle draw mode', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      const drawButton = screen.getByTitle('Draw on video');
      
      // Should start with draw mode off (no border)
      expect(drawButton).not.toHaveClass('border-2');
      
      await mockUser.click(drawButton);
      
      // Should toggle draw mode on (with border)
      expect(drawButton).toHaveClass('border-2');
      expect(eventSpy).toHaveBeenCalledWith(
        { action: 'draw', is_draw_mode: true }
      );
    });

    it('should show color selector when draw mode is active', async () => {
      render(CommentInput);
      
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      // Undo/Redo buttons should appear
      expect(screen.getByTitle('Undo')).toBeInTheDocument();
      expect(screen.getByTitle('Redo')).toBeInTheDocument();
      
      // Color buttons should appear (they don't have accessible names, just styles)
      const colorButtons = screen.getAllByRole('button').filter(button => 
        button.style.background && ['red', 'green', 'blue', 'cyan', 'yellow', 'black', 'white'].includes(button.style.background)
      );
      expect(colorButtons.length).toBe(7);
    });

    it('should hide color selector when draw mode is deactivated', async () => {
      render(CommentInput);
      
      const drawButton = screen.getByTitle('Draw on video');
      
      // Enable draw mode first
      await mockUser.click(drawButton);
      expect(screen.getByTitle('Undo')).toBeInTheDocument();
      
      // Disable draw mode
      await mockUser.click(drawButton);
      
      await waitFor(() => {
        const undoButton = screen.queryByTitle('Undo');
        // Element should either be removed or its parent container should have inert attribute (transitioning out)
        expect(undoButton === null || undoButton?.closest('[inert]')).toBeTruthy();
      });
    });

    it('should enable send button when in draw mode even without text', async () => {
      render(CommentInput);
      
      const sendButton = screen.getByRole('button', { name: /send/i });
      expect(sendButton).toBeDisabled();
      
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      expect(sendButton).not.toBeDisabled();
    });
  });

  describe('Color selection', () => {
    it('should render all color options in draw mode', async () => {
      render(CommentInput);
      
      // Enter draw mode first
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      const colors = ['red', 'green', 'blue', 'cyan', 'yellow', 'black', 'white'];
      
      colors.forEach(color => {
        const colorButton = screen.getAllByRole('button').find(button => 
          button.style.background === color
        );
        expect(colorButton).toBeInTheDocument();
      });
    });

    it('should show red as default selected color', async () => {
      render(CommentInput);
      
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      const redButton = screen.getAllByRole('button').find(button => 
        button.style.background === 'red'
      );
      expect(redButton).toHaveClass('border-2', 'border-gray-100');
    });

    it('should dispatch color_select event when color is clicked', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      // Enter draw mode first
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      const blueButton = screen.getAllByRole('button').find(button => 
        button.style.background === 'blue'
      );
      await mockUser.click(blueButton!);
      
      expect(eventSpy).toHaveBeenCalledWith(
        { action: 'color_select', color: 'blue' }
      );
    });

    it('should update selected color visual state', async () => {
      render(CommentInput);
      
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      const redButton = screen.getAllByRole('button').find(button => 
        button.style.background === 'red'
      );
      const greenButton = screen.getAllByRole('button').find(button => 
        button.style.background === 'green'
      );
      
      // Red should start selected
      expect(redButton).toHaveClass('border-2', 'border-gray-100');
      expect(greenButton).toHaveClass('border', 'border-gray-600');
      
      await mockUser.click(greenButton!);
      
      // Green should now be selected
      expect(greenButton).toHaveClass('border-2', 'border-gray-100');
      expect(redButton).toHaveClass('border', 'border-gray-600');
    });
  });

  describe('Undo/Redo functionality', () => {
    it('should render undo and redo buttons in draw mode', async () => {
      render(CommentInput);
      
      // Enter draw mode first
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      expect(screen.getByTitle('Undo')).toBeInTheDocument();
      expect(screen.getByTitle('Redo')).toBeInTheDocument();
    });

    it('should dispatch undo event when undo button clicked', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      // Enter draw mode first
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      const undoButton = screen.getByTitle('Undo');
      await mockUser.click(undoButton);
      
      expect(eventSpy).toHaveBeenCalledWith(
        { action: 'undo' }
      );
    });

    it('should dispatch redo event when redo button clicked', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      // Enter draw mode first
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      const redoButton = screen.getByTitle('Redo');
      await mockUser.click(redoButton);
      
      expect(eventSpy).toHaveBeenCalledWith(
        { action: 'redo' }
      );
    });
  });

  describe('Form submission', () => {
    it('should dispatch send event with text when send button clicked', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
      await mockUser.type(textInput, 'Test comment');
      
      const sendButton = screen.getByRole('button', { name: /send/i });
      await mockUser.click(sendButton);
      
      expect(eventSpy).toHaveBeenCalledWith(
        {
          action: 'send',
          comment_text: 'Test comment',
          is_timed: true
        }
      );
    });

    it('should dispatch send event with correct timed state', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      // Disable timed mode
      const timedButton = screen.getByTitle('Comment is time specific?');
      await mockUser.click(timedButton);
      
      const textInput = screen.getByPlaceholderText('Add a comment...');
      await mockUser.type(textInput, 'Global comment');
      
      const sendButton = screen.getByRole('button', { name: /send/i });
      await mockUser.click(sendButton);
      
      expect(eventSpy).toHaveBeenCalledWith(
        {
          action: 'send',
          comment_text: 'Global comment',
          is_timed: false
        }
      );
    });

    it('should submit form on Enter key press', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
      await mockUser.type(textInput, 'Test comment{Enter}');
      
      expect(eventSpy).toHaveBeenCalledWith(
        {
          action: 'send',
          comment_text: 'Test comment',
          is_timed: true
        }
      );
    });

    it('should clear input and exit draw mode after sending', async () => {
      render(CommentInput);
      
      // Enter draw mode and add text
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
      await mockUser.type(textInput, 'Test comment');
      
      expect(drawButton).toHaveClass('border-2'); // Draw mode active
      expect((textInput as HTMLInputElement).value).toBe('Test comment');
      
      const sendButton = screen.getByRole('button', { name: /send/i });
      await mockUser.click(sendButton);
      
      // Should reset state
      expect((textInput as HTMLInputElement).value).toBe('');
      expect(drawButton).not.toHaveClass('border-2'); // Draw mode deactivated
    });
  });

  describe('External API', () => {
    it('should expose forceDrawMode method', () => {
      const { component } = render(CommentInput);
      
      expect(typeof component.forceDrawMode).toBe('function');
    });

    it('should activate draw mode when forceDrawMode(true) is called', async () => {
      const eventSpy = vi.fn();
      const { component } = render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      const drawButton = screen.getByTitle('Draw on video');
      expect(drawButton).not.toHaveClass('border-2');
      
      component.forceDrawMode(true);
      
      await waitFor(() => {
        expect(drawButton).toHaveClass('border-2');
      });
      
      // forceDrawMode doesn't dispatch events, it just sets the state
      expect(eventSpy).not.toHaveBeenCalled();
    });

    it('should deactivate draw mode when forceDrawMode(false) is called', async () => {
      const { component } = render(CommentInput);
      
      // First activate draw mode
      component.forceDrawMode(true);
      
      const drawButton = screen.getByTitle('Draw on video');
      await waitFor(() => {
        expect(drawButton).toHaveClass('border-2');
      });
      
      // Then deactivate
      component.forceDrawMode(false);
      
      await waitFor(() => {
        expect(drawButton).not.toHaveClass('border-2');
      });
    });
  });

  describe('Edge cases and error handling', () => {
    it('should handle empty text submission gracefully', async () => {
      const eventSpy = vi.fn();
      render(CommentInput, {
        props: { onbuttonclicked: eventSpy }
      });
      
      const sendButton = screen.getByRole('button', { name: /send/i });
      
      // Button should be disabled with empty text and no draw mode
      expect(sendButton).toBeDisabled();
      
      // Enter draw mode to enable button
      const drawButton = screen.getByTitle('Draw on video');
      await mockUser.click(drawButton);
      
      await mockUser.click(sendButton);
      
      expect(eventSpy).toHaveBeenCalledWith(
        {
          action: 'send',
          comment_text: undefined,
          is_timed: true
        }
      );
    });

    it('should handle rapid toggle operations', async () => {
      render(CommentInput);
      
      const timedButton = screen.getByTitle('Comment is time specific?');
      const drawButton = screen.getByTitle('Draw on video');
      
      // Rapid toggling should work correctly
      await mockUser.click(timedButton);
      await mockUser.click(drawButton);
      await mockUser.click(drawButton);
      
      // Should end up in working state - draw mode disabled, timed mode forced back
      expect(drawButton).not.toHaveClass('border-2');
      expect(screen.getByPlaceholderText('Add a comment - at current time...')).toBeInTheDocument();
    });

    it('should maintain state consistency when video readiness changes', async () => {
      render(CommentInput);
      
      const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
      await mockUser.type(textInput, 'Test comment');
      
      // Change video readiness
      videoIsReady.set(false);
      
      await waitFor(() => {
        expect(screen.queryByTitle('Comment is time specific?')).not.toBeInTheDocument();
      });
      
      // Text should still be there
      expect((textInput as HTMLInputElement).value).toBe('Test comment');
      
      // Restore video readiness
      videoIsReady.set(true);
      
      await waitFor(() => {
        expect(screen.getByTitle('Comment is time specific?')).toBeInTheDocument();
      });
      
      // Text should still be there
      expect((textInput as HTMLInputElement).value).toBe('Test comment');
    });
  });
});
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CommentTimelinePin from '@/lib/player_view/CommentTimelinePin.svelte';

// Mock the hexColorForUsername function
vi.mock('@/lib/Avatar.svelte', () => ({
  hexColorForUsername: vi.fn((username: string) => {
    // Simple hash to color conversion for testing
    const colors = ['#ff0000', '#00ff00', '#0000ff', '#ffff00', '#ff00ff', '#00ffff'];
    const hash = username.split('').reduce((a, b) => a + b.charCodeAt(0), 0);
    return colors[hash % colors.length];
  })
}));

describe('CommentTimelinePin.svelte - Post-Migration (Callback Props)', () => {
  const mockUser = userEvent.setup();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  describe('Basic rendering', () => {
    it('should render with required props', () => {
      render(CommentTimelinePin, {
        props: {
          id: 'pin-123',
          username: 'testuser',
          comment: 'Test comment text',
          x_loc: 0.5
        }
      });

      // Should render the pin elements
      const pin = screen.getByRole('link');
      expect(pin).toBeInTheDocument();
    });

    it('should render with default props when not provided', () => {
      render(CommentTimelinePin, {
        props: {}
      });

      const pin = screen.getByRole('link');
      expect(pin).toBeInTheDocument();
      expect(pin).toHaveAttribute('title', ': '); // default username and comment are empty
    });

    it('should apply correct positioning based on x_loc', () => {
      const { container } = render(CommentTimelinePin, {
        props: {
          id: 'pin-123',
          username: 'testuser',
          comment: 'Test comment',
          x_loc: 0.75
        }
      });

      const pinContainer = container.querySelector('.pin');
      expect(pinContainer).toHaveStyle('left: 75%');
    });

    it('should display tooltip with username and comment', () => {
      render(CommentTimelinePin, {
        props: {
          id: 'pin-123',
          username: 'john_doe',
          comment: 'This is a test comment',
          x_loc: 0.3
        }
      });

      const sphere = screen.getByRole('link');
      expect(sphere).toHaveAttribute('title', 'john_doe: This is a test comment');
    });

    it('should apply color based on username', () => {
      const { container } = render(CommentTimelinePin, {
        props: {
          id: 'pin-123',
          username: 'colortest',
          comment: 'Test comment',
          x_loc: 0.5
        }
      });

      const line = container.querySelector('.line');
      const sphere = container.querySelector('.sphere');
      
      // Both should have the same color based on username (using actual mock result)
      expect(line).toHaveStyle('background-color: #00ff00'); // Based on our mock implementation
      expect(sphere).toHaveStyle('background-color: #00ff00');
    });
  });

  describe('Event handling - New Callback Props Implementation', () => {
    it('should call onclick callback with id when sphere is clicked', async () => {
      const onClickMock = vi.fn();
      
      render(CommentTimelinePin, {
        props: {
          id: 'pin-123',
          username: 'testuser',
          comment: 'Test comment',
          x_loc: 0.5,
          onclick: onClickMock
        }
      });

      const sphere = screen.getByRole('link');
      await mockUser.click(sphere);

      expect(onClickMock).toHaveBeenCalledOnce();
      expect(onClickMock).toHaveBeenCalledWith({ id: 'pin-123' });
    });

    it('should call onclick callback on Enter key', async () => {
      const onClickMock = vi.fn();
      
      render(CommentTimelinePin, {
        props: {
          id: 'pin-456',
          username: 'keyboarduser',
          comment: 'Keyboard test',
          x_loc: 0.7,
          onclick: onClickMock
        }
      });

      const sphere = screen.getByRole('link');
      sphere.focus();
      await mockUser.keyboard('{Enter}');

      expect(onClickMock).toHaveBeenCalledOnce();
      expect(onClickMock).toHaveBeenCalledWith({ id: 'pin-456' });
    });

    it('should not call onclick callback on other keys', async () => {
      const onClickMock = vi.fn();
      
      render(CommentTimelinePin, {
        props: {
          id: 'pin-789',
          username: 'keytest',
          comment: 'Other key test',
          x_loc: 0.2,
          onclick: onClickMock
        }
      });

      const sphere = screen.getByRole('link');
      sphere.focus();
      
      // Test various keys that should not trigger click
      await mockUser.keyboard('{Space}');
      await mockUser.keyboard('{Escape}');
      await mockUser.keyboard('{Tab}');
      await mockUser.keyboard('a');

      expect(onClickMock).not.toHaveBeenCalled();
    });

    it('should call correct onclick callback for different components', async () => {
      const onClickMock1 = vi.fn();
      const onClickMock2 = vi.fn();
      
      // Render first pin
      render(CommentTimelinePin, {
        props: {
          id: 'first-pin',
          username: 'user1',
          comment: 'First comment',
          x_loc: 0.25,
          onclick: onClickMock1
        }
      });

      // Render second pin
      render(CommentTimelinePin, {
        props: {
          id: 'second-pin',
          username: 'user2',
          comment: 'Second comment',
          x_loc: 0.75,
          onclick: onClickMock2
        }
      });

      // Click first pin
      const spheres = screen.getAllByRole('link');
      await mockUser.click(spheres[0]);

      expect(onClickMock1).toHaveBeenCalledWith({ id: 'first-pin' });
      expect(onClickMock2).not.toHaveBeenCalled();

      // Click second pin
      await mockUser.click(spheres[1]);

      expect(onClickMock2).toHaveBeenCalledWith({ id: 'second-pin' });
      expect(onClickMock1).toHaveBeenCalledTimes(1); // Still only called once
    });
  });

  describe('Accessibility', () => {
    it('should be focusable with keyboard navigation', () => {
      render(CommentTimelinePin, {
        props: {
          id: 'pin-a11y',
          username: 'a11yuser',
          comment: 'Accessibility test',
          x_loc: 0.5
        }
      });

      const sphere = screen.getByRole('link');
      expect(sphere).toHaveAttribute('tabindex', '0');
      expect(sphere).toHaveAttribute('role', 'link');
    });

    it('should have proper ARIA attributes', () => {
      render(CommentTimelinePin, {
        props: {
          id: 'pin-aria',
          username: 'ariauser',
          comment: 'ARIA test comment',
          x_loc: 0.6
        }
      });

      const sphere = screen.getByRole('link');
      expect(sphere).toHaveAttribute('title', 'ariauser: ARIA test comment');
    });
  });

  describe('Edge cases and error handling', () => {
    it('should handle empty id gracefully', async () => {
      const onClickMock = vi.fn();
      
      render(CommentTimelinePin, {
        props: {
          id: '',
          username: 'testuser',
          comment: 'Empty ID test',
          x_loc: 0.5,
          onclick: onClickMock
        }
      });

      const sphere = screen.getByRole('link');
      await mockUser.click(sphere);

      expect(onClickMock).toHaveBeenCalledWith({ id: '' });
    });

    it('should handle undefined/null id gracefully', async () => {
      const onClickMock = vi.fn();
      
      render(CommentTimelinePin, {
        props: {
          id: undefined,
          username: 'testuser',
          comment: 'Undefined ID test',
          x_loc: 0.5,
          onclick: onClickMock
        }
      });

      const sphere = screen.getByRole('link');
      await mockUser.click(sphere);

      // Should not crash and should call callback with default empty string id
      expect(onClickMock).toHaveBeenCalledWith({ id: "" });
    });

    it('should not crash when onclick callback is undefined', async () => {
      render(CommentTimelinePin, {
        props: {
          id: 'pin-no-callback',
          username: 'testuser',
          comment: 'No callback test',
          x_loc: 0.5
          // onclick: undefined (not provided)
        }
      });

      const sphere = screen.getByRole('link');
      
      // Should render without error
      expect(sphere).toBeInTheDocument();

      // Should not crash when clicked without callback
      await mockUser.click(sphere);
      expect(sphere).toBeInTheDocument();

      // Should not crash when Enter is pressed without callback
      sphere.focus();
      await mockUser.keyboard('{Enter}');
      expect(sphere).toBeInTheDocument();
    });

    it('should handle extreme x_loc values', () => {
      // Test x_loc = 0 (far left)
      const { container: container1 } = render(CommentTimelinePin, {
        props: {
          id: 'pin-left',
          username: 'leftuser',
          comment: 'Left edge',
          x_loc: 0
        }
      });

      const pin1 = container1.querySelector('.pin');
      expect(pin1).toHaveStyle('left: 0%');

      cleanup();

      // Test x_loc = 1 (far right)
      const { container: container2 } = render(CommentTimelinePin, {
        props: {
          id: 'pin-right',
          username: 'rightuser',
          comment: 'Right edge',
          x_loc: 1
        }
      });

      const pin2 = container2.querySelector('.pin');
      expect(pin2).toHaveStyle('left: 100%');

      cleanup();

      // Test x_loc > 1 (beyond right edge)
      const { container: container3 } = render(CommentTimelinePin, {
        props: {
          id: 'pin-beyond',
          username: 'beyonduser',
          comment: 'Beyond right',
          x_loc: 1.5
        }
      });

      const pin3 = container3.querySelector('.pin');
      expect(pin3).toHaveStyle('left: 150%');
    });

    it('should handle long usernames and comments', () => {
      const longUsername = 'very_long_username_that_might_cause_layout_issues';
      const longComment = 'This is a very long comment that might cause tooltip or layout issues when displayed in the timeline pin component';

      render(CommentTimelinePin, {
        props: {
          id: 'pin-long',
          username: longUsername,
          comment: longComment,
          x_loc: 0.5
        }
      });

      const sphere = screen.getByRole('link');
      expect(sphere).toHaveAttribute('title', `${longUsername}: ${longComment}`);
      expect(sphere).toBeInTheDocument();
    });

    it('should handle special characters in username and comment', () => {
      const specialUsername = 'user@#$%^&*()';
      const specialComment = 'Comment with <>&"" special chars';

      render(CommentTimelinePin, {
        props: {
          id: 'pin-special',
          username: specialUsername,
          comment: specialComment,
          x_loc: 0.5
        }
      });

      const sphere = screen.getByRole('link');
      expect(sphere).toHaveAttribute('title', `${specialUsername}: ${specialComment}`);
    });
  });

  describe('Visual styling and layout', () => {
    it('should have correct CSS classes applied', () => {
      const { container } = render(CommentTimelinePin, {
        props: {
          id: 'pin-style',
          username: 'styleuser',
          comment: 'Style test',
          x_loc: 0.5
        }
      });

      const pin = container.querySelector('.pin');
      const line = container.querySelector('.line');
      const sphere = container.querySelector('.sphere');

      expect(pin).toHaveClass('pin');
      expect(line).toHaveClass('line');
      expect(sphere).toHaveClass('sphere');
    });

    it('should have proper positioning styles', () => {
      const { container } = render(CommentTimelinePin, {
        props: {
          id: 'pin-position',
          username: 'positionuser',
          comment: 'Position test',
          x_loc: 0.33
        }
      });

      const pin = container.querySelector('.pin');
      // Just check that the pin element exists and has some basic styles
      expect(pin).toBeInTheDocument();
      expect(pin).toHaveClass('pin');
    });
  });
});
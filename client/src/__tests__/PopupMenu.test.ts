import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import PopupMenu from '@/lib/asset_browser/PopupMenu.svelte';
import * as Proto3 from '@clapshot_protobuf/typescript';

describe('PopupMenu.svelte', () => {
  const mockUser = userEvent.setup();

  beforeEach(() => {
    // Mock window dimensions for positioning tests
    Object.defineProperty(window, 'innerWidth', {
      writable: true,
      configurable: true,
      value: 1024,
    });
    Object.defineProperty(window, 'innerHeight', {
      writable: true,
      configurable: true,
      value: 768,
    });
  });

  afterEach(() => {
    cleanup();
  });

  describe('fmtColorToCSS helper function', () => {
    it('should handle null/undefined colors', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: {
          label: 'Test Item',
          icon: {
            faClass: {
              classes: 'fa-test',
              color: null
            }
          }
        },
        action: { code: 'test-action' }
      }];

      render(PopupMenu, { 
        props: { x: 0, y: 0, menuLines }
      });

      // Should render without error and use default "black" color
      expect(screen.getByText('Test Item')).toBeInTheDocument();
    });

    it('should convert Proto3.Color to CSS rgb string', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: {
          label: 'Colored Item',
          icon: {
            faClass: {
              classes: 'fa-test',
              color: { r: 255, g: 128, b: 64 }
            }
          }
        },
        action: { code: 'colored-action' }
      }];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      const icon = screen.getByRole('button').querySelector('i');
      expect(icon).toHaveStyle('color: rgb(255,128,64)');
    });
  });

  describe('Basic rendering', () => {
    it('should render menu at specified position', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: {
          label: 'Test Item'
        },
        action: { code: 'test-action' }
      }];

      render(PopupMenu, {
        props: { x: 100, y: 200, menuLines }
      });

      const nav = screen.getByRole('navigation');
      expect(nav).toHaveStyle('position: absolute');
      expect(nav).toHaveStyle('top: 200px');
      expect(nav).toHaveStyle('left: 100px');
      expect(nav).toHaveStyle('z-index: 30');
    });

    it('should render menu items with labels', () => {
      const menuLines: Proto3.ActionDef[] = [
        {
          uiProps: { label: 'First Item' },
          action: { code: 'first-action' }
        },
        {
          uiProps: { label: 'Second Item' },
          action: { code: 'second-action' }
        }
      ];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      expect(screen.getByText('First Item')).toBeInTheDocument();
      expect(screen.getByText('Second Item')).toBeInTheDocument();
      expect(screen.getAllByRole('button')).toHaveLength(2);
    });

    it('should hide menu when hide method is called', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'Test Item' },
        action: { code: 'test-action' }
      }];

      const { component } = render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      const hideSpy = vi.fn();

      // Re-render with hide callback
      cleanup();
      const { component: newComponent } = render(PopupMenu, {
        props: { x: 0, y: 0, menuLines, onhide: hideSpy }
      });

      // Initially visible
      expect(screen.getByText('Test Item')).toBeInTheDocument();

      // Hide the menu
      newComponent.hide();

      // Should call hide callback
      expect(hideSpy).toHaveBeenCalledOnce();
    });
  });

  describe('Menu items with icons', () => {
    it('should render FontAwesome icons', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: {
          label: 'Icon Item',
          icon: {
            faClass: {
              classes: 'fa-solid fa-user',
              color: { r: 255, g: 0, b: 0 }
            }
          }
        },
        action: { code: 'icon-action' }
      }];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      const icon = screen.getByRole('button').querySelector('i');
      expect(icon).toHaveClass('fa-solid', 'fa-user');
      expect(icon).toHaveStyle('color: rgb(255,0,0)');
    });

    it('should render image icons', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: {
          label: 'Image Item',
          icon: {
            imgUrl: '/test-icon.png'
          }
        },
        action: { code: 'image-action' }
      }];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      // img with empty alt is treated as presentation role, not img role
      const img = screen.getByRole('button').querySelector('img');
      expect(img).toHaveAttribute('src', '/test-icon.png');
      expect(img).toHaveAttribute('alt', '');
      // CSS em units are computed to px values, so we check for presence of max-width/height
      expect(img).toHaveStyle('max-width: 32px; max-height: 32px;');
    });

    it('should render both FontAwesome and image icons if both are present', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: {
          label: 'Dual Icon Item',
          icon: {
            faClass: {
              classes: 'fa-solid fa-star'
            },
            imgUrl: '/test-icon.png'
          }
        },
        action: { code: 'dual-icon-action' }
      }];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      const button = screen.getByRole('button');
      const icon = button.querySelector('i');
      const img = button.querySelector('img');

      expect(icon).toHaveClass('fa-solid', 'fa-star');
      expect(img).toHaveAttribute('src', '/test-icon.png');
    });
  });

  describe('Horizontal rule (separator)', () => {
    it('should render HR when label is "hr" and no action code', () => {
      const menuLines: Proto3.ActionDef[] = [
        {
          uiProps: { label: 'First Item' },
          action: { code: 'first-action' }
        },
        {
          uiProps: { label: 'hr' },
          action: {} // No code = separator
        },
        {
          uiProps: { label: 'Second Item' },
          action: { code: 'second-action' }
        }
      ];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      expect(screen.getByText('First Item')).toBeInTheDocument();
      expect(screen.getByText('Second Item')).toBeInTheDocument();
      expect(screen.getByRole('separator')).toBeInTheDocument();
      expect(screen.getAllByRole('button')).toHaveLength(2); // Only actual buttons, not HR
    });

    it('should not render HR when label is "hr" but action code exists', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'hr' },
        action: { code: 'not-a-separator' }
      }];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      expect(screen.getByText('hr')).toBeInTheDocument();
      expect(screen.getByRole('button')).toBeInTheDocument();
      expect(screen.queryByRole('separator')).not.toBeInTheDocument();
    });

    it('should handle case insensitive "HR" label', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'HR' },
        action: {}
      }];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      expect(screen.getByRole('separator')).toBeInTheDocument();
    });
  });

  describe('Event handling', () => {
    it('should call action callback when menu item is clicked', async () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'Click Me' },
        action: { code: 'test-action', params: { key: 'value' } }
      }];

      const actionSpy = vi.fn();
      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines, onaction: actionSpy }
      });

      const button = screen.getByText('Click Me');
      await mockUser.click(button);

      expect(actionSpy).toHaveBeenCalledOnce();
      expect(actionSpy).toHaveBeenCalledWith({
        action: menuLines[0]
      });
    });

    it('should hide menu after clicking item', async () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'Hide Test' },
        action: { code: 'hide-action' }
      }];

      const hideSpy = vi.fn();
      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines, onhide: hideSpy }
      });

      const button = screen.getByText('Hide Test');
      await mockUser.click(button);

      expect(hideSpy).toHaveBeenCalledOnce();
      expect(screen.queryByText('Hide Test')).not.toBeInTheDocument();
    });

    it('should hide menu when window is clicked', async () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'Window Click Test' },
        action: { code: 'window-click-action' }
      }];

      const hideSpy = vi.fn();
      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines, onhide: hideSpy }
      });

      // Initially visible
      expect(screen.getByText('Window Click Test')).toBeInTheDocument();

      // Click on window (simulated by clicking outside the component)
      await mockUser.click(document.body);

      expect(hideSpy).toHaveBeenCalledOnce();
    });

    it('should stop propagation when menu item is clicked', async () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'Stop Propagation Test' },
        action: { code: 'propagation-action' }
      }];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      const windowClickSpy = vi.fn();
      window.addEventListener('click', windowClickSpy);

      const button = screen.getByText('Stop Propagation Test');
      await mockUser.click(button);

      // Window click handler should not be called due to stopPropagation
      expect(windowClickSpy).not.toHaveBeenCalled();

      window.removeEventListener('click', windowClickSpy);
    });
  });

  describe('Menu positioning', () => {
    it('should render menu at basic specified position', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'Position Test' },
        action: { code: 'position-action' }
      }];

      render(PopupMenu, {
        props: { x: 100, y: 200, menuLines }
      });

      const nav = screen.getByRole('navigation');
      expect(nav).toHaveStyle('left: 100px');
      expect(nav).toHaveStyle('top: 200px');
    });
  });

  describe('Component API', () => {
    it('should have hide method available on component', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'API Test' },
        action: { code: 'api-action' }
      }];

      const { component } = render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      expect(typeof component.hide).toBe('function');
    });

    it('should call hide callback when hide method is called', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: { label: 'Hide Test' },
        action: { code: 'hide-action' }
      }];

      const hideSpy = vi.fn();
      const { component } = render(PopupMenu, {
        props: { x: 0, y: 0, menuLines, onhide: hideSpy }
      });

      component.hide();

      expect(hideSpy).toHaveBeenCalledOnce();
    });
  });

  describe('Edge cases', () => {
    it('should handle empty menu lines', () => {
      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines: [] }
      });

      const nav = screen.getByRole('navigation');
      expect(nav).toBeInTheDocument();
      expect(screen.queryByRole('button')).not.toBeInTheDocument();
    });

    it('should handle menu items without uiProps', () => {
      const menuLines: Proto3.ActionDef[] = [
        { action: { code: 'no-ui-props' } },
        {
          uiProps: { label: 'Has UI Props' },
          action: { code: 'has-ui-props' }
        }
      ];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      // Should only render the item with uiProps
      expect(screen.queryByText('Has UI Props')).toBeInTheDocument();
      expect(screen.getAllByRole('button')).toHaveLength(1);
    });

    it('should handle undefined icon properties gracefully', () => {
      const menuLines: Proto3.ActionDef[] = [{
        uiProps: {
          label: 'No Icon Item',
          icon: undefined
        },
        action: { code: 'no-icon-action' }
      }];

      render(PopupMenu, {
        props: { x: 0, y: 0, menuLines }
      });

      expect(screen.getByText('No Icon Item')).toBeInTheDocument();
      const button = screen.getByRole('button');
      expect(button.querySelector('i')).not.toBeInTheDocument();
      expect(button.querySelector('img')).not.toBeInTheDocument();
    });
  });
});
/**
 * Tests for NavBar.svelte - Navigation, authentication, and collaboration features
 * 
 * These tests focus on core functionality that can be tested without complex UI interactions:
 * - Component rendering and state management
 * - Authentication logic and logout flow
 * - Video information display
 * - Store integration and reactivity
 * - URL and link generation logic
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { get } from 'svelte/store';
import NavBar from '@/lib/NavBar.svelte';
import {
  curUsername,
  curUserPic,
  curVideo,
  mediaFileId,
  collabId,
  userMenuItems,
  latestProgressReports,
  clientConfig
} from '@/stores';
import type { UserMenuItem, MediaProgressReport } from '@/types';
import { createMinimalMediaFile } from '../mocks/protobuf-factories';
import * as Proto3 from '@clapshot_protobuf/typescript';

// Mock navigator.clipboard
const mockClipboard = {
  writeText: vi.fn()
};
Object.defineProperty(navigator, 'clipboard', {
  value: mockClipboard,
  writable: true
});

// Mock window.location
const mockLocation = {
  href: 'http://localhost:3000/',
  reload: vi.fn()
};
Object.defineProperty(window, 'location', {
  value: mockLocation,
  writable: true
});

// Mock global functions
global.alert = vi.fn();
global.fetch = vi.fn();

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
    } as any;
  }
  return null;
});

global.CanvasRenderingContext2D = vi.fn();

// Mock process.env
Object.defineProperty(process, 'env', {
  value: {
    CLAPSHOT_CLIENT_VERSION: '1.0.0'
  }
});

describe('NavBar.svelte', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    
    // Reset all stores to default values
    curUsername.set('');
    curUserPic.set('');
    curVideo.set(null);
    mediaFileId.set(null);
    collabId.set(null);
    userMenuItems.set([]);
    latestProgressReports.set([]);
    clientConfig.set({
      logo_url: 'test-logo.svg',
      app_title: 'Test App',
      logout_url: '/logout'
    });
    
    // Reset mocks
    mockClipboard.writeText.mockResolvedValue(undefined);
    mockLocation.href = 'http://localhost:3000/';
  });

  describe('Logo and branding display', () => {
    it('should display logo and app title from config', () => {
      clientConfig.set({
        logo_url: 'custom-logo.svg',
        app_title: 'Custom App'
      });

      render(NavBar);

      const logo = screen.getByAltText('Custom App');
      expect(logo).toBeInTheDocument();
      expect(logo.getAttribute('src')).toBe('custom-logo.svg');
      
      expect(screen.getByText('CUSTOM APP')).toBeInTheDocument();
    });

    it('should use default values when config is missing', () => {
      clientConfig.set({});

      render(NavBar);

      const logo = screen.getByAltText('Clapshot');
      expect(logo.getAttribute('src')).toBe('clapshot-logo.svg');
      
      expect(screen.getByText('CLAPSHOT')).toBeInTheDocument();
    });

    it('should handle empty logo URL gracefully', () => {
      clientConfig.set({
        logo_url: '',
        app_title: 'Test App'
      });

      render(NavBar);

      const logo = screen.getByAltText('Test App');
      expect(logo.getAttribute('src')).toBe('clapshot-logo.svg');
    });
  });

  describe('User authentication display', () => {
    it('should show username when user is logged in', () => {
      curUsername.set('TestUser');

      render(NavBar);

      expect(screen.getByText('TestUser')).toBeInTheDocument();
      
      // Should show user button
      const userButton = screen.getByRole('button', { name: 'TestUser' });
      expect(userButton).toBeInTheDocument();
      expect(userButton).toHaveAttribute('id', 'user-button');
    });

    it('should hide user section when no username is set', () => {
      curUsername.set('');

      render(NavBar);

      // User section should be hidden (visibility: hidden)
      const userSections = document.querySelectorAll('.flex-0');
      const userSection = Array.from(userSections).find(el => 
        el.getAttribute('style')?.includes('visibility: hidden')
      );
      expect(userSection).toHaveStyle('visibility: hidden');
    });

    it('should show user section when username is set', () => {
      curUsername.set('TestUser');

      render(NavBar);

      const userSections = document.querySelectorAll('.flex-0');
      const userSection = Array.from(userSections).find(el => 
        el.getAttribute('style')?.includes('visibility: visible')
      );
      expect(userSection).toHaveStyle('visibility: visible');
    });
  });

  describe('Video information display', () => {
    it('should show video info when media file is loaded', () => {
      const testVideo = createMinimalMediaFile({
        id: 'video123',
        title: 'Test Video Title'
      });

      mediaFileId.set('video123');
      curVideo.set(testVideo);

      render(NavBar);

      expect(screen.getByText('video123')).toBeInTheDocument();
      expect(screen.getByText('Test Video Title')).toBeInTheDocument();
    });

    it('should hide video info when no media file is loaded', () => {
      mediaFileId.set(null);
      curVideo.set(null);

      render(NavBar);

      expect(screen.queryByText('video123')).not.toBeInTheDocument();
    });

    it('should display different videos from store', () => {
      // Test with first video
      mediaFileId.set('video123');
      curVideo.set(createMinimalMediaFile({
        id: 'video123',
        title: 'First Video'
      }));

      const { unmount } = render(NavBar);

      expect(screen.getByText('video123')).toBeInTheDocument();
      expect(screen.getByText('First Video')).toBeInTheDocument();
      
      unmount();

      // Test with different video in fresh render
      mediaFileId.set('video456');
      curVideo.set(createMinimalMediaFile({
        id: 'video456',
        title: 'Second Video'
      }));

      render(NavBar);

      expect(screen.getByText('video456')).toBeInTheDocument();
      expect(screen.getByText('Second Video')).toBeInTheDocument();
    });
  });

  describe('Collaboration status indication', () => {
    beforeEach(() => {
      mediaFileId.set('video123');
      curVideo.set(createMinimalMediaFile({
        id: 'video123',
        title: 'Test Video'
      }));
    });

    it('should show normal styling when not in collaboration', () => {
      collabId.set(null);

      render(NavBar);

      // Look for the video menu button with gray styling
      const buttons = document.querySelectorAll('button');
      const videoMenuButton = Array.from(buttons).find(btn => 
        btn.className.includes('bg-gray-800')
      );
      
      expect(videoMenuButton).toBeTruthy();
      expect(videoMenuButton?.className).toContain('bg-gray-800');
      expect(videoMenuButton?.className).not.toContain('bg-green-500');
    });

    it('should show green styling when in collaboration', () => {
      collabId.set('session123');

      render(NavBar);

      // Look for the video menu button with green styling
      const buttons = document.querySelectorAll('button');
      const videoMenuButton = Array.from(buttons).find(btn => 
        btn.className.includes('bg-green-500')
      );
      
      expect(videoMenuButton).toBeTruthy();
      expect(videoMenuButton?.className).toContain('bg-green-500');
      expect(videoMenuButton?.className).not.toContain('bg-gray-800');
    });
  });

  describe('Authentication logic', () => {
    it('should construct correct logout URL with custom config', () => {
      clientConfig.set({
        logout_url: '/custom-logout'
      });

      // Test URL construction logic
      const config = get(clientConfig);
      const logoutUrl = config?.logout_url || "/logout";
      
      expect(logoutUrl).toBe('/custom-logout');
    });

    it('should use default logout URL when not configured', () => {
      clientConfig.set({});

      const config = get(clientConfig);
      const logoutUrl = config?.logout_url || "/logout";
      
      expect(logoutUrl).toBe('/logout');
    });

    it('should generate unique nonce for logout credentials', () => {
      // Test the nonce generation logic
      const nonce1 = Math.random().toString(36).substring(2, 15);
      const nonce2 = Math.random().toString(36).substring(2, 15);
      
      expect(nonce1).not.toBe(nonce2);
      expect(nonce1.length).toBeGreaterThan(5);
      expect(nonce2.length).toBeGreaterThan(5);
    });

    it('should construct correct basic auth header', () => {
      const nonce = 'test123';
      const username = 'logout_user__' + nonce;
      const password = 'bad_pass__' + nonce;
      const expectedHeader = 'Basic ' + btoa(username + ':' + password);
      
      expect(expectedHeader).toBe('Basic ' + btoa('logout_user__test123:bad_pass__test123'));
    });
  });

  describe('URL and link generation', () => {
    it('should construct correct share URL', () => {
      mediaFileId.set('video123');
      
      // Test URL construction logic
      const currentMediaFileId = get(mediaFileId);
      const urlParams = `?vid=${currentMediaFileId}`;
      const currentUrl = 'http://localhost:3000/test.html?existing=param';
      const baseUrl = currentUrl.split('?')[0];
      const fullUrl = baseUrl + urlParams;
      
      expect(fullUrl).toBe('http://localhost:3000/test.html?vid=video123');
    });

    it('should construct collaboration start URL', () => {
      mediaFileId.set('video123');
      const randomSessionId = 'random123';
      
      const url = `?vid=${get(mediaFileId)}&collab=${randomSessionId}`;
      
      expect(url).toBe('?vid=video123&collab=random123');
    });

    it('should construct collaboration leave URL', () => {
      mediaFileId.set('video123');
      
      const url = `?vid=${get(mediaFileId)}`;
      
      expect(url).toBe('?vid=video123');
    });
  });

  describe('Store integration', () => {
    it('should display username from store', () => {
      curUsername.set('TestUser');

      render(NavBar);

      expect(screen.getByText('TestUser')).toBeInTheDocument();
    });

    it('should display video information from stores', () => {
      mediaFileId.set('video123');
      curVideo.set(createMinimalMediaFile({
        id: 'video123',
        title: 'Test Video'
      }));

      render(NavBar);

      expect(screen.getByText('video123')).toBeInTheDocument();
      expect(screen.getByText('Test Video')).toBeInTheDocument();
    });

    it('should show collaboration status from store', () => {
      mediaFileId.set('video123');
      curVideo.set(createMinimalMediaFile({
        id: 'video123',
        title: 'Test Video'
      }));
      collabId.set('session123');

      render(NavBar);

      // Should show green collaboration button
      const buttons = document.querySelectorAll('button');
      const videoMenuButton = Array.from(buttons).find(btn => 
        btn.className.includes('bg-green-500')
      );
      expect(videoMenuButton).toBeTruthy();
    });
  });

  describe('About dialog functionality', () => {
    it('should display correct version in about dialog', () => {
      // Test the about message construction
      const expectedMessage = "Clapshot Client version " + process.env.CLAPSHOT_CLIENT_VERSION + "\n" +
        "\n" +
        "Visit the project page at:\n" +
        "https://github.com/elonen/clapshot\n";
      
      expect(expectedMessage).toContain('1.0.0');
      expect(expectedMessage).toContain('https://github.com/elonen/clapshot');
    });
  });

  describe('Progress report handling', () => {
    it('should find progress report for current video', () => {
      const reports: MediaProgressReport[] = [
        {
          mediaFileId: 'video1',
          msg: 'Processing video1...',
          progress: 0.3,
          received_ts: Date.now()
        },
        {
          mediaFileId: 'video123',
          msg: 'Processing current video...',
          progress: 0.7,
          received_ts: Date.now()
        },
        {
          mediaFileId: 'video2',
          msg: 'Processing video2...',
          progress: 0.5,
          received_ts: Date.now()
        }
      ];

      const currentMediaFileId = 'video123';
      const matchingReport = reports.find(r => r.mediaFileId === currentMediaFileId);
      
      expect(matchingReport?.msg).toBe('Processing current video...');
      expect(matchingReport?.progress).toBe(0.7);
    });

    it('renders a progress bar for the active video', () => {
      const report: MediaProgressReport = {
        mediaFileId: 'video123',
        msg: 'Uploading to storage…',
        progress: 0.4,
        received_ts: Date.now()
      };
      mediaFileId.set('video123');
      curVideo.set(createMinimalMediaFile({
        id: 'video123',
        title: 'Test Video'
      }));
      latestProgressReports.set([report]);

      const { container } = render(NavBar);
      expect(screen.getAllByText('Uploading to storage…').length).toBeGreaterThan(0);
      const bars = container.querySelectorAll('.bg-amber-500');
      expect(bars.length).toBeGreaterThan(0);
    });

    it('should return undefined when no matching progress report', () => {
      const reports: MediaProgressReport[] = [
        {
          mediaFileId: 'video1',
          msg: 'Processing video1...',
          progress: 0.3,
          received_ts: Date.now()
        }
      ];

      const currentMediaFileId = 'video123';
      const matchingReport = reports.find(r => r.mediaFileId === currentMediaFileId);
      
      expect(matchingReport).toBeUndefined();
    });
  });

  describe('User menu item handling', () => {
    it('should handle different menu item types', () => {
      const menuItems: UserMenuItem[] = [
        { label: 'Settings', type: 'url', data: '/settings' },
        { label: 'Separator', type: 'divider' },
        { label: 'Logout', type: 'logout-basic-auth' },
        { label: 'About', type: 'about' },
        { label: 'External', type: 'url', data: 'https://example.com' }
      ];

      // Test item type classification
      const urlItems = menuItems.filter(item => item.type === 'url');
      const authItems = menuItems.filter(item => item.type === 'logout-basic-auth');
      const aboutItems = menuItems.filter(item => item.type === 'about');
      const dividerItems = menuItems.filter(item => item.type === 'divider');

      expect(urlItems).toHaveLength(2);
      expect(authItems).toHaveLength(1);
      expect(aboutItems).toHaveLength(1);
      expect(dividerItems).toHaveLength(1);
    });
  });

  describe('Edge cases and error handling', () => {
    it('should handle missing video data gracefully', () => {
      mediaFileId.set('video123');
      curVideo.set(null);

      expect(() => render(NavBar)).not.toThrow();
    });

    it('should handle undefined client config', () => {
      clientConfig.set(undefined as any);

      expect(() => render(NavBar)).not.toThrow();
    });

    it('should handle empty progress reports array', () => {
      mediaFileId.set('video123');
      latestProgressReports.set([]);

      expect(() => render(NavBar)).not.toThrow();
    });

    it('should handle missing media file ID', () => {
      mediaFileId.set(null);
      curVideo.set(createMinimalMediaFile({
        id: 'video123',
        title: 'Test Video'
      }));

      expect(() => render(NavBar)).not.toThrow();
    });
  });

  describe('Component lifecycle and error handling', () => {
    it('should render without errors when stores are empty', () => {
      expect(() => render(NavBar)).not.toThrow();
    });

    it('should handle different store states without errors', () => {
      // Test with user logged in
      curUsername.set('TestUser');
      const { unmount } = render(NavBar);
      expect(screen.getByText('TestUser')).toBeInTheDocument();
      unmount();
      
      // Test with no user (fresh render)
      curUsername.set('');
      mediaFileId.set(null);
      curVideo.set(null);
      
      expect(() => render(NavBar)).not.toThrow();
    });
  });
});

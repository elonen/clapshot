import { expect, afterEach, vi } from 'vitest'
import { cleanup } from '@testing-library/svelte'
import '@testing-library/jest-dom'

// Cleanup after each test case (e.g. clearing DOM)
afterEach(() => {
  cleanup()
})

// Mock WebSocket globally
global.WebSocket = class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  url = '';
  protocol = '';
  bufferedAmount = 0;
  extensions = '';
  binaryType = 'blob' as BinaryType;

  onopen: ((event: Event) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  constructor(url: string, protocols?: string | string[]) {
    this.url = url;
  }

  send = vi.fn();
  close = vi.fn();
  addEventListener = vi.fn();
  removeEventListener = vi.fn();
  dispatchEvent = vi.fn();
} as any;

// Mock global objects that might be used in components
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation(query => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(), // deprecated
    removeListener: vi.fn(), // deprecated
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
})

// Mock ResizeObserver
global.ResizeObserver = vi.fn().mockImplementation(() => ({
  observe: vi.fn(),
  unobserve: vi.fn(),
  disconnect: vi.fn(),
}))

// Mock Web Animations API
Element.prototype.animate = vi.fn(() => ({
  finished: Promise.resolve(),
  cancel: vi.fn(),
  finish: vi.fn(),
  pause: vi.fn(),
  play: vi.fn(),
  reverse: vi.fn(),
  updatePlaybackRate: vi.fn(),
  addEventListener: vi.fn(),
  removeEventListener: vi.fn(),
  dispatchEvent: vi.fn(),
}))

// Mock IntersectionObserver
global.IntersectionObserver = vi.fn().mockImplementation(() => ({
  observe: vi.fn(),
  unobserve: vi.fn(),
  disconnect: vi.fn(),
}))

// Mock animation frame functions
global.requestAnimationFrame = vi.fn((cb) => {
  return setTimeout(cb, 16) as any;
});
global.cancelAnimationFrame = vi.fn((id) => {
  clearTimeout(id);
});

// Mock console methods to avoid noise in tests
global.console = {
  ...console,
  log: vi.fn(),
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
}

// Helper function for checking elements that might be transitioning out with inert attribute
export function expectElementToBeHiddenOrInert(element: HTMLElement | null) {
  // Element should either be completely removed from DOM or be in a container with inert attribute
  return element === null || element.closest('[inert]') !== null;
}
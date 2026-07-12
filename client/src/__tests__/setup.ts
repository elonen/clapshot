import { expect, afterEach, vi } from 'vitest'
import { cleanup } from '@testing-library/svelte'
import '@testing-library/jest-dom'

// Store active timeouts and intervals for cleanup
const activeTimeouts = new Set<number>();
const activeIntervals = new Set<number>();
const activeRafs = new Set<number>();

// Override setTimeout and setInterval to track active timers
const originalSetTimeout = globalThis.setTimeout;
const originalSetInterval = globalThis.setInterval;
const originalClearTimeout = globalThis.clearTimeout;
const originalClearInterval = globalThis.clearInterval;

// Declare animation frame mocks early for use in cleanup
let mockRequestAnimationFrame: ReturnType<typeof vi.fn>;
let mockCancelAnimationFrame: ReturnType<typeof vi.fn>;

globalThis.setTimeout = (...args) => {
  const id = originalSetTimeout(...args);
  activeTimeouts.add(id);
  return id;
};

globalThis.setInterval = (...args) => {
  const id = originalSetInterval(...args);
  activeIntervals.add(id);
  return id;
};

globalThis.clearTimeout = (id) => {
  activeTimeouts.delete(id);
  return originalClearTimeout(id);
};

globalThis.clearInterval = (id) => {
  activeIntervals.delete(id);
  return originalClearInterval(id);
};

// Cleanup after each test case (e.g. clearing DOM and all timers)
afterEach(() => {
  // Clear all active timeouts and intervals before DOM cleanup
  activeTimeouts.forEach(id => {
    try { originalClearTimeout(id); } catch (e) { /* ignore */ }
  });
  activeIntervals.forEach(id => {
    try { originalClearInterval(id); } catch (e) { /* ignore */ }
  });
  activeRafs.forEach(id => {
    try { mockCancelAnimationFrame(id); } catch (e) { /* ignore */ }
  });
  
  // Clear the tracking sets
  activeTimeouts.clear();
  activeIntervals.clear();
  activeRafs.clear();
  
  // Clear DOM
  cleanup();
})

// Mock localStorage for i18n and other browser-dependent tests
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: vi.fn((key: string) => store[key] || null),
    setItem: vi.fn((key: string, value: string) => { store[key] = value; }),
    removeItem: vi.fn((key: string) => { delete store[key]; }),
    clear: vi.fn(() => { store = {}; }),
  };
})();

Object.defineProperty(window, 'localStorage', {
  value: localStorageMock,
  writable: true,
  configurable: true,
});

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

// Mock animation frame functions with persistent global binding
mockRequestAnimationFrame = vi.fn((cb) => {
  const id = originalSetTimeout(cb, 16);
  activeRafs.add(id);
  return id;
});
mockCancelAnimationFrame = vi.fn((id) => {
  activeRafs.delete(id);
  originalClearTimeout(id);
});

global.requestAnimationFrame = mockRequestAnimationFrame;
global.cancelAnimationFrame = mockCancelAnimationFrame;

// Ensure these are also available on window object for broader compatibility
if (typeof window !== 'undefined') {
  window.requestAnimationFrame = mockRequestAnimationFrame;
  window.cancelAnimationFrame = mockCancelAnimationFrame;
}

// Make these functions available globally, even after potential garbage collection
Object.defineProperty(globalThis, 'requestAnimationFrame', {
  value: mockRequestAnimationFrame,
  writable: true,
  configurable: true
});

Object.defineProperty(globalThis, 'cancelAnimationFrame', {
  value: mockCancelAnimationFrame,
  writable: true,
  configurable: true
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
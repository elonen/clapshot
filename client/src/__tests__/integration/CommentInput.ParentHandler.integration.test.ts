/**
 * Simple integration test that demonstrates the exact e.detail vs e bug pattern
 * that occurred in App.svelte with CommentInput
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CommentInput from '@/lib/player_view/CommentInput.svelte';

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
      set: vi.fn(),
      update: vi.fn()
    };
  };

  return {
    videoIsReady: createMockStore(true)
  };
});

describe('CommentInput -> Parent Handler Integration (Bug Detection)', () => {
  const mockUser = userEvent.setup();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it('demonstrates the e.detail vs e bug pattern that occurred in App.svelte', async () => {
    /**
     * This test shows EXACTLY what happened in App.svelte:
     * 1. CommentInput sends: {action: 'text_input'}
     * 2. App.svelte expected: e.detail.action
     * 3. Result: undefined because e.detail doesn't exist
     */
    
    let receivedAction: any = 'not-called';
    
    // Simulate the BUGGY App.svelte handler
    const buggyAppHandler = (e: any) => {
      // This is what App.svelte was doing (the bug):
      receivedAction = e.detail?.action;  // BUG: tries to access e.detail.action
    };
    
    render(CommentInput, {
      props: { onbuttonclicked: buggyAppHandler }
    });

    // Type something to trigger text_input event
    const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
    await mockUser.type(textInput, 'T'); // Single character to trigger once

    // The bug results in undefined because e.detail doesn't exist
    expect(receivedAction).toBe(undefined); // Because e.detail doesn't exist
  });

  it('shows the correct pattern that fixes the bug', async () => {
    /**
     * This shows the CORRECT way after the Svelte 5 migration
     */
    
    let receivedAction = null;
    
    // Correct handler (what App.svelte should do):
    const correctAppHandler = (e: any) => {
      receivedAction = e.action;  // CORRECT: access e.action directly
    };
    
    render(CommentInput, {
      props: { onbuttonclicked: correctAppHandler }
    });

    const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
    await mockUser.type(textInput, 'Test');

    // With the correct handler, we get the expected data
    expect(receivedAction).toBe('text_input');
  });

  it('demonstrates the bug with send action', async () => {
    let lastReceivedData: any = null;
    
    // Buggy handler trying to access e.detail.*
    const buggyHandler = (e: any) => {
      lastReceivedData = {
        action: e.detail?.action,         // BUG
        comment_text: e.detail?.comment_text,  // BUG  
        is_timed: e.detail?.is_timed      // BUG
      };
    };
    
    render(CommentInput, {
      props: { onbuttonclicked: buggyHandler }
    });

    const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
    await mockUser.type(textInput, 'Test comment');
    
    const sendButton = screen.getByRole('button', { name: /send/i });
    await mockUser.click(sendButton);

    // Shows that the buggy pattern gives undefined values
    expect(lastReceivedData).toEqual({
      action: undefined,
      comment_text: undefined,
      is_timed: undefined
    });
  });

  it('shows correct send action handling', async () => {
    let lastReceivedData: any = null;
    
    // Correct handler
    const correctHandler = (e: any) => {
      if (e.action === 'send') {  // Only capture send events
        lastReceivedData = {
          action: e.action,           // CORRECT
          comment_text: e.comment_text,    // CORRECT
          is_timed: e.is_timed        // CORRECT
        };
      }
    };
    
    render(CommentInput, {
      props: { onbuttonclicked: correctHandler }
    });

    const textInput = screen.getByPlaceholderText('Add a comment - at current time...');
    await mockUser.type(textInput, 'Test comment');
    
    const sendButton = screen.getByRole('button', { name: /send/i });
    await mockUser.click(sendButton);

    // Correct pattern works
    expect(lastReceivedData).toEqual({
      action: 'send',
      comment_text: 'Test comment',
      is_timed: true
    });
  });
});
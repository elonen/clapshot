import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { expectElementToBeHiddenOrInert } from './setup';
import SubtitleCard from '@/lib/player_view/SubtitleCard.svelte';
import * as Proto3 from '@clapshot_protobuf/typescript';
import { get } from 'svelte/store';
import { curSubtitle, curUserId, curUserIsAdmin, curVideo, subtitleEditingId } from '@/stores';

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
    curSubtitle: createMockStore(null),
    curUserId: createMockStore('user-123'),
    curUserIsAdmin: createMockStore(false),
    curVideo: createMockStore({ userId: 'user-123', id: 'video-456' }),
    subtitleEditingId: createMockStore(null)
  };
});

describe('SubtitleCard.svelte', () => {
  const mockUser = userEvent.setup();

  const mockSubtitle: Proto3.Subtitle = {
    id: 'sub-123',
    title: 'English Subtitles',
    languageCode: 'en',
    origFilename: 'subtitles.srt',
    origUrl: '/subtitles/sub-123.srt',
    timeOffset: 0,
    userId: 'user-123'
  };

  beforeEach(() => {
    // Reset all mocks and stores
    vi.resetAllMocks();
    
    // Reset all stores to initial state
    curSubtitle.set(null);
    curUserId.set('user-123');
    curUserIsAdmin.set(false);
    curVideo.set({ userId: 'user-123', id: 'video-456' });
    subtitleEditingId.set(null);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    
    // Reset stores after each test to prevent contamination
    curSubtitle.set(null);
    curUserId.set('user-123');
    curUserIsAdmin.set(false);
    curVideo.set({ userId: 'user-123', id: 'video-456' });
    subtitleEditingId.set(null);
  });

  describe('Basic rendering', () => {
    it('should render subtitle title and language code', () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      // Check for language code and subtitle text 
      const elements = screen.getAllByText((content, element) => {
        return element?.textContent?.includes('EN – English Subtitles') || false;
      });
      expect(elements.length).toBeGreaterThan(0);
      expect(screen.getByRole('button', { name: /english subtitles/i })).toBeInTheDocument();
    });

    it('should display eye-slash icon when subtitle is not current', () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const button = screen.getByRole('button', { name: /english subtitles/i });
      const icon = button.querySelector('i');
      expect(icon).toHaveClass('fa-eye-slash');
      expect(button).toHaveClass('text-gray-400');
    });

    it('should display eye icon when subtitle is current', () => {
      curSubtitle.set(mockSubtitle);

      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const button = screen.getByRole('button', { name: /english subtitles/i });
      const icon = button.querySelector('i');
      expect(icon).toHaveClass('fa-eye');
      expect(button).toHaveClass('text-amber-600');
    });

    it('should show filename in button title attribute', () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const button = screen.getByRole('button', { name: /english subtitles/i });
      expect(button).toHaveAttribute('title', 'subtitles.srt');
    });
  });

  describe('Language code formatting', () => {
    it('should uppercase language codes', () => {
      const frenchSub = { ...mockSubtitle, languageCode: 'fr', title: 'French Subtitles' };
      
      render(SubtitleCard, {
        props: { sub: frenchSub, isDefault: false }
      });

      const elements = screen.getAllByText((content, element) => {
        return element?.textContent?.includes('FR – French Subtitles') || false;
      });
      expect(elements.length).toBeGreaterThan(0);
    });

    it('should handle 3-character language codes', () => {
      const chineseSub = { ...mockSubtitle, languageCode: 'chi', title: 'Chinese Subtitles' };
      
      render(SubtitleCard, {
        props: { sub: chineseSub, isDefault: false }
      });

      const elements = screen.getAllByText((content, element) => {
        return element?.textContent?.includes('CHI – Chinese Subtitles') || false;
      });
      expect(elements.length).toBeGreaterThan(0);
    });
  });

  describe('User permissions and edit button', () => {
    it('should show edit button when user owns the video', () => {
      curVideo.set({ userId: 'user-123', id: 'video-456' });
      curUserId.set('user-123');

      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      expect(editButton).toBeInTheDocument();
      expect(editButton).toHaveClass('fa-angle-right');
    });

    it('should show edit button when user is admin', () => {
      curVideo.set({ userId: 'other-user', id: 'video-456' });
      curUserId.set('user-123');
      curUserIsAdmin.set(true);

      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      expect(editButton).toBeInTheDocument();
    });

    it('should hide edit button when user does not own video and is not admin', () => {
      curVideo.set({ userId: 'other-user', id: 'video-456' });
      curUserId.set('user-123');
      curUserIsAdmin.set(false);

      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      expect(screen.queryByRole('button', { name: /edit subtitle/i })).not.toBeInTheDocument();
    });
  });

  describe('Edit form toggle', () => {
    it('should show edit form when edit button is clicked', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      expect(screen.getByLabelText('Title')).toBeInTheDocument();
      expect(screen.getByDisplayValue('English Subtitles')).toBeInTheDocument();
      expect(screen.getByDisplayValue('en')).toBeInTheDocument();
      expect(screen.getByDisplayValue('0')).toBeInTheDocument();
    });

    it('should change edit button icon when form is open', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      expect(editButton).toHaveClass('fa-angle-right');

      await mockUser.click(editButton);
      expect(editButton).toHaveClass('fa-angle-down');
    });

    it('should hide edit form when edit button is clicked again', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);
      expect(screen.getByLabelText('Title')).toBeInTheDocument();

      await mockUser.click(editButton);
      await waitFor(() => {
        const titleInput = screen.queryByLabelText('Title');
        // Element should either be removed or its form should have inert attribute (transitioning out)
        expect(titleInput === null || titleInput?.closest('[inert]')).toBeTruthy();
      });
    });

    it('should show edit form when subtitle is double-clicked', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const subtitleButton = screen.getByRole('button', { name: /english subtitles/i });
      await mockUser.dblClick(subtitleButton);

      expect(screen.getByLabelText('Title')).toBeInTheDocument();
    });
  });

  describe('Edit form fields', () => {
    beforeEach(async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);
    });

    it('should populate form fields with current subtitle data', () => {
      const titleInput = screen.getByDisplayValue('English Subtitles') as HTMLInputElement;
      const languageInput = screen.getByDisplayValue('en') as HTMLInputElement;
      const offsetInput = screen.getByDisplayValue('0') as HTMLInputElement;
      const defaultCheckbox = screen.getByRole('checkbox') as HTMLInputElement;

      expect(titleInput.value).toBe('English Subtitles');
      expect(languageInput.value).toBe('en');
      expect(offsetInput.value).toBe('0');
      expect(defaultCheckbox.checked).toBe(false);
    });

    it('should validate language code input length', () => {
      const languageInput = screen.getByDisplayValue('en') as HTMLInputElement;
      
      expect(languageInput).toHaveAttribute('minlength', '2');
      expect(languageInput).toHaveAttribute('maxlength', '3');
      expect(languageInput).toHaveClass('uppercase', 'font-mono');
    });

    it('should configure time offset as number input with step', () => {
      const offsetInput = screen.getByLabelText(/time offset/i) as HTMLInputElement;
      
      expect(offsetInput).toHaveAttribute('type', 'number');
      expect(offsetInput).toHaveAttribute('step', '0.01');
    });

    it('should show language code info link', () => {
      const infoLink = screen.getByRole('link', { name: 'ISO 639 language codes information' });
      expect(infoLink).toHaveAttribute('href', 'https://en.wikipedia.org/wiki/List_of_ISO_639_language_codes');
      expect(infoLink).toHaveAttribute('target', '_blank');
    });

    it('should render download link with correct URL', () => {
      const downloadLink = screen.getByRole('link', { name: /download/i });
      expect(downloadLink).toHaveAttribute('href', '/subtitles/sub-123.srt');
      expect(downloadLink).toHaveAttribute('download');
    });
  });

  describe('Form validation', () => {
    it('should handle title input changes', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      const titleInput = screen.getByLabelText('Title');
      await mockUser.clear(titleInput);
      await mockUser.type(titleInput, 'Updated Title');

      expect((titleInput as HTMLInputElement).value).toBe('Updated Title');
    });

    it('should handle language code input changes', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      const languageInput = screen.getByDisplayValue('en');
      await mockUser.clear(languageInput);
      await mockUser.type(languageInput, 'fr');

      expect((languageInput as HTMLInputElement).value).toBe('fr');
    });

    it('should handle time offset input changes', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      const offsetInput = screen.getByLabelText(/time offset/i);
      await mockUser.clear(offsetInput);
      await mockUser.type(offsetInput, '2.5');

      expect((offsetInput as HTMLInputElement).value).toBe('2.5');
    });

    it('should handle default checkbox changes', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      const defaultCheckbox = screen.getByLabelText('Default Subtitle') as HTMLInputElement;
      expect(defaultCheckbox.checked).toBe(false);

      await mockUser.click(defaultCheckbox);
      expect(defaultCheckbox.checked).toBe(true);
    });
  });

  describe('Event dispatching', () => {
    it('should dispatch change-subtitle event when subtitle button is clicked', async () => {
      const changeSpy = vi.fn();
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false, onchangesubtitle: changeSpy }
      });

      // Get the main subtitle button by its title attribute
      const subtitleButton = screen.getByTitle('subtitles.srt');
      await mockUser.click(subtitleButton);

      expect(changeSpy).toHaveBeenCalledOnce();
      expect(changeSpy).toHaveBeenCalledWith(
        { id: 'sub-123' }
      );
    });

    it('should dispatch update-subtitle event when save button is clicked', async () => {
      const updateSpy = vi.fn();
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: true, onupdatesubtitle: updateSpy }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      const saveButton = screen.getByRole('button', { name: /save/i });
      await mockUser.click(saveButton);

      expect(updateSpy).toHaveBeenCalledOnce();
      expect(updateSpy).toHaveBeenCalledWith(
        { sub: mockSubtitle, isDefault: true }
      );
    });

    it('should dispatch delete-subtitle event when delete button is clicked', async () => {
      const deleteSpy = vi.fn();
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false, ondeletesubtitle: deleteSpy }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      const deleteButton = screen.getByRole('button', { name: /del/i });
      await mockUser.click(deleteButton);

      expect(deleteSpy).toHaveBeenCalledOnce();
      expect(deleteSpy).toHaveBeenCalledWith(
        { id: 'sub-123' }
      );
    });
  });

  describe('Store integration', () => {
    it('should update subtitleEditingId store when edit button is clicked', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      // Check that the store was updated
      expect(subtitleEditingId.set).toHaveBeenCalledWith('sub-123');
    });

    it('should clear subtitleEditingId store when save is clicked', async () => {
      subtitleEditingId.set('sub-123');
      
      const { component } = render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      component.$on('update-subtitle', vi.fn());

      const saveButton = screen.getByRole('button', { name: /save/i });
      await mockUser.click(saveButton);

      expect(subtitleEditingId.set).toHaveBeenCalledWith(null);
    });

    it('should clear subtitleEditingId store when delete is clicked', async () => {
      subtitleEditingId.set('sub-123');
      
      const { component } = render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      component.$on('delete-subtitle', vi.fn());

      const deleteButton = screen.getByRole('button', { name: /del/i });
      await mockUser.click(deleteButton);

      expect(subtitleEditingId.set).toHaveBeenCalledWith(null);
    });

    it('should toggle subtitleEditingId store correctly', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      
      // First click - should set to subtitle id
      await mockUser.click(editButton);
      expect(subtitleEditingId.set).toHaveBeenCalledWith('sub-123');

      // Second click - should set to null
      subtitleEditingId.set('sub-123'); // Simulate store update
      await mockUser.click(editButton);
      expect(subtitleEditingId.set).toHaveBeenCalledWith(null);
    });
  });

  describe('Keyboard navigation', () => {
    it('should close edit form when Escape key is pressed', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      expect(screen.getByLabelText('Title')).toBeInTheDocument();

      const titleInput = screen.getByLabelText('Title');
      await mockUser.type(titleInput, '{Escape}');

      expect(subtitleEditingId.set).toHaveBeenCalledWith(null);
    });

    it('should allow tab navigation through form fields', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      const titleInput = screen.getByDisplayValue('English Subtitles');
      const languageInput = screen.getByDisplayValue('en');
      const offsetInput = screen.getByDisplayValue('0');

      await mockUser.tab();
      expect(titleInput).toHaveFocus();

      await mockUser.tab();
      // Tab goes to info link next, then language input
      await mockUser.tab();
      expect(languageInput).toHaveFocus();

      await mockUser.tab();
      expect(offsetInput).toHaveFocus();
    });
  });

  describe('Edge cases', () => {
    it('should handle empty subtitle title', () => {
      const emptyTitleSub = { ...mockSubtitle, title: '' };
      
      render(SubtitleCard, {
        props: { sub: emptyTitleSub, isDefault: false }
      });

      // Should show language code with dash but no title text after the dash
      const elements = screen.getAllByText((content, element) => {
        // Look for the pattern where we have language code and dash, but title is empty
        return element?.textContent?.match(/^[A-Z]{2,3} – ?$/) || false;
      });
      expect(elements.length).toBeGreaterThan(0);
    });

    it('should handle negative time offset', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: false }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      const offsetInput = screen.getByLabelText(/time offset/i);
      await mockUser.clear(offsetInput);
      await mockUser.type(offsetInput, '-1.5');

      expect((offsetInput as HTMLInputElement).value).toBe('-1.5');
    });

    it('should handle subtitle with different user ID', () => {
      const otherUserSub = { ...mockSubtitle, userId: 'other-user' };
      
      render(SubtitleCard, {
        props: { sub: otherUserSub, isDefault: false }
      });

      // Should render the subtitle regardless of who owns it
      expect(screen.getByTitle('subtitles.srt')).toBeInTheDocument();
      // Test passes if subtitle is rendered properly with different user ID
      expect(screen.getByRole('button', { name: /edit subtitle/i })).toBeInTheDocument();
    });

    it('should render correctly when isDefault prop is true', async () => {
      render(SubtitleCard, {
        props: { sub: mockSubtitle, isDefault: true }
      });

      const editButton = screen.getByRole('button', { name: /edit subtitle/i });
      await mockUser.click(editButton);

      const defaultCheckbox = screen.getByLabelText('Default Subtitle') as HTMLInputElement;
      expect(defaultCheckbox.checked).toBe(true);
    });
  });
});
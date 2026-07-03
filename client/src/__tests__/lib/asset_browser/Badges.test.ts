/**
 * Tests for Badges.svelte (the mini-badge pills, e.g. version "vN" labels).
 */
import { describe, it, expect, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/svelte';
import Badges from '@/lib/asset_browser/Badges.svelte';

afterEach(cleanup);

describe('Badges.svelte', () => {
  it('renders one pill per badge with its text', () => {
    render(Badges, { props: { badges: [{ text: 'v3' }, { text: 'v1 of 5' }] } });
    expect(screen.getByText('v3')).toBeInTheDocument();
    expect(screen.getByText('v1 of 5')).toBeInTheDocument();
  });

  it('defaults to orange when a badge has no color', () => {
    render(Badges, { props: { badges: [{ text: 'v2' }] } });
    expect(screen.getByText('v2')).toHaveStyle('background-color: rgb(234, 88, 12)');
  });

  it('uses the badge color when provided', () => {
    render(Badges, { props: { badges: [{ text: 'v9', color: { r: 1, g: 2, b: 3 } }] } });
    expect(screen.getByText('v9')).toHaveStyle('background-color: rgb(1, 2, 3)');
  });

  it('renders a top-right badges container when there are badges', () => {
    render(Badges, { props: { badges: [{ text: 'v1' }] } });
    expect(screen.getByTestId('badges')).toBeInTheDocument();
  });

  it('renders nothing when there are no badges', () => {
    const { container } = render(Badges, { props: { badges: [] } });
    expect(container.querySelector('[data-testid="badges"]')).not.toBeInTheDocument();
  });
});

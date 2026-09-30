import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import EmptyState from '$lib/components/ui/EmptyState.svelte';

describe('EmptyState', () => {
  it('shows the message with the default mark', () => {
    const { container } = render(EmptyState, {
      props: { message: 'No instances yet.' }
    });
    expect(screen.getByText('No instances yet.')).toBeTruthy();
    const mark = container.querySelector('img.empty-mark');
    expect(mark?.getAttribute('src')).toBe('/icons/generic.svg');
    expect(mark?.getAttribute('aria-hidden')).toBe('true');
  });

  it('uses a custom mark when given', () => {
    const { container } = render(EmptyState, {
      props: { message: 'Nothing here.', mark: '/icons/python.svg' }
    });
    expect(container.querySelector('img.empty-mark')?.getAttribute('src')).toBe('/icons/python.svg');
  });
});

import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import KeepTimeLine from '$lib/components/instances/KeepTimeLine.svelte';

describe('KeepTimeLine', () => {
  it('shows the policy line when keep-time is configured', () => {
    render(KeepTimeLine, { props: { keepTimeSeconds: 900, keepTimeAction: 'pause' } });
    expect(screen.getByText('Idle 15 minutes then Pause')).toBeTruthy();
  });

  it('shows hours for a long keep-time', () => {
    render(KeepTimeLine, { props: { keepTimeSeconds: 7200, keepTimeAction: 'stop' } });
    expect(screen.getByText('Idle 2 hours then Stop')).toBeTruthy();
  });

  it('renders nothing when keep-time is disabled', () => {
    const { container } = render(KeepTimeLine, { props: { keepTimeSeconds: null, keepTimeAction: 'stop' } });
    expect(container.textContent).toBe('');
  });

  it('renders nothing when props are absent', () => {
    const { container } = render(KeepTimeLine, { props: {} });
    expect(container.textContent).toBe('');
  });

  it('renders nothing when the action is missing', () => {
    const { container } = render(KeepTimeLine, { props: { keepTimeSeconds: 600, keepTimeAction: null } });
    expect(container.textContent).toBe('');
  });
});

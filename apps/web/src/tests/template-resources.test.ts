import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import TemplateResources from '$lib/components/forms/TemplateResources.svelte';

const baseProps = {
  cores: 2,
  ramGb: 4,
  gpuCount: 0,
  dockerRegistry: '',
  persistentStoragePath: '',
  maxRunSeconds: null,
  timeoutAction: 'remove' as const,
  keepTimeSeconds: null,
  keepTimeAction: 'pause' as const,
};

describe('TemplateResources', () => {
  it('renders resource fields without an allocation mode selector', () => {
    const { container } = render(TemplateResources, { props: baseProps });

    expect(screen.getByLabelText('CPU Cores *')).toBeTruthy();
    expect(screen.getByLabelText('RAM (GB) *')).toBeTruthy();
    expect(screen.getByLabelText('GPU')).toBeTruthy();
    expect(container.querySelector('[data-testid="allocation-mode-select"]')).toBeNull();
    expect(screen.queryByText('Allocation Mode')).toBeNull();
  });
});

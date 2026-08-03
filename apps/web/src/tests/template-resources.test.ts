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

function modeSelect(container: HTMLElement) {
  return container.querySelector<HTMLSelectElement>('[data-testid="allocation-mode-select"]')!;
}

describe('TemplateResources allocation mode', () => {
  it('offers both shared and dedicated to admins', () => {
    const { container } = render(TemplateResources, {
      props: { ...baseProps, allocationMode: 'shared', canAllocateDedicated: true }
    });
    const select = modeSelect(container);
    expect(Array.from(select.options).map((o) => o.value)).toEqual(['shared', 'dedicated']);
    expect(select.disabled).toBe(false);
  });

  it('hides the dedicated option for managers', () => {
    const { container } = render(TemplateResources, {
      props: { ...baseProps, allocationMode: 'shared', canAllocateDedicated: false }
    });
    const select = modeSelect(container);
    expect(Array.from(select.options).map((o) => o.value)).toEqual(['shared']);
    expect(select.disabled).toBe(false);
  });

  it('locks a dedicated template for managers and preserves its value', () => {
    const { container } = render(TemplateResources, {
      props: { ...baseProps, allocationMode: 'dedicated', canAllocateDedicated: false }
    });
    const select = modeSelect(container);
    expect(select.disabled).toBe(true);
    expect(select.value).toBe('dedicated');
    expect(screen.queryByText(/set to dedicated mode by an admin/i)).toBeTruthy();
  });
});

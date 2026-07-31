import { render, screen, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import TemplatePanel from '$lib/components/templates/TemplatePanel.svelte';

const editorView = { tab: 'templates', editor: 'new' } as const;

function panelProps(overrides: Record<string, unknown> = {}) {
  return {
    view: editorView,
    configs: [],
    dirty: false,
    onnavigate: vi.fn(),
    ondelete: vi.fn(),
    ...overrides,
  };
}

describe('TemplatePanel', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders the new-template editor without effect looping', () => {
    expect(() => render(TemplatePanel, { props: panelProps() })).not.toThrow();
    expect(screen.getByText('New Template')).toBeTruthy();
    expect(screen.getByText('Create Template')).toBeTruthy();
  });

  it('drops the form and shows the list when leaving the editor view', async () => {
    const { rerender } = render(TemplatePanel, { props: panelProps() });
    await rerender({ view: { tab: 'templates', editor: 'list' } });
    await tick();
    expect(screen.getByText('+ New Template')).toBeTruthy();
  });

  it('only prompts on cancel when the form is dirty', async () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const onnavigate = vi.fn();
    const { container } = render(TemplatePanel, { props: panelProps({ onnavigate }) });

    await fireEvent.click(screen.getByText('Cancel'));
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(onnavigate).toHaveBeenCalledWith('#templates');

    const nameInput = container.querySelector('input[placeholder="e.g. AI Lab"]') as HTMLInputElement;
    await fireEvent.input(nameInput, { target: { value: 'hello' } });
    await tick();

    await fireEvent.click(screen.getByText('Cancel'));
    expect(confirmSpy).toHaveBeenCalled();
  });
});

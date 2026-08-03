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

  it('hides the dedicated allocation option for non-admins', () => {
    const { container } = render(TemplatePanel, { props: panelProps() });
    const select = container.querySelector<HTMLSelectElement>('[data-testid="allocation-mode-select"]');
    expect(select).toBeTruthy();
    expect(Array.from(select!.options).map((o) => o.value)).toEqual(['shared']);
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

  function checkboxes(container: HTMLElement) {
    return Array.from(container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'));
  }
  function secondsInputs(container: HTMLElement) {
    return Array.from(container.querySelectorAll<HTMLInputElement>('input[placeholder="e.g. 3600 (1 hour)"]'));
  }

  it.each(['usage', 'keep-time'])(
    'clearing the %s seconds input keeps the field enabled and visible',
    async (kind) => {
      const { container } = render(TemplatePanel, { props: panelProps() });

      const index = kind === 'usage' ? 0 : 1;
      const checkbox = checkboxes(container)[index];
      expect(checkbox.checked).toBe(false);
      expect(secondsInputs(container).length).toBe(0);

      await fireEvent.click(checkbox);
      await tick();
      expect(checkbox.checked).toBe(true);
      expect(secondsInputs(container).length).toBe(1);

      const input = secondsInputs(container)[0];
      await fireEvent.input(input, { target: { value: '' } });
      await tick();

      expect(secondsInputs(container).length).toBe(1);
      expect(checkboxes(container)[index].checked).toBe(true);
    }
  );

  it('re-enabling after clearing the field shows the input again', async () => {
    const { container } = render(TemplatePanel, { props: panelProps() });

    const checkbox = checkboxes(container)[0];
    await fireEvent.click(checkbox);
    await tick();

    const input = secondsInputs(container)[0];
    await fireEvent.input(input, { target: { value: '' } });
    await tick();

    await fireEvent.click(checkbox);
    await tick();
    expect(checkboxes(container)[0].checked).toBe(false);
    expect(secondsInputs(container).length).toBe(0);

    await fireEvent.click(checkboxes(container)[0]);
    await tick();
    expect(checkboxes(container)[0].checked).toBe(true);
    expect(secondsInputs(container).length).toBe(1);
    expect((secondsInputs(container)[0] as HTMLInputElement).value).toBe('3600');
  });

  function showAdvanced(container: HTMLElement) {
    const btn = Array.from(container.querySelectorAll<HTMLButtonElement>('button')).find(b => b.textContent?.includes('Show Advanced'));
    return btn;
  }
  function diniToggle(container: HTMLElement) {
    return Array.from(container.querySelectorAll<HTMLElement>('[data-testid="dini-toggle"]'));
  }
  function runtimeSelect(container: HTMLElement) {
    return container.querySelector<HTMLSelectElement>('[data-testid="runtime-select"]');
  }

  it('shows the sandbox-protection indicator when DinI is on with the runsc runtime', async () => {
    const { container } = render(TemplatePanel, { props: panelProps() });
    await fireEvent.click(showAdvanced(container)!);
    await tick();

    expect(runtimeSelect(container)).toBeTruthy();

    await fireEvent.click(diniToggle(container)[0]);
    await tick();
    const select = runtimeSelect(container)!;
    await fireEvent.change(select, { target: { value: 'runsc' } });
    await tick();

    expect(screen.queryByText('Sandboxed via gVisor')).toBeTruthy();
    expect(screen.queryByText(/runs with full host privileges/i)).toBeNull();
  });

  it('shows a high-risk warning when DinI is on without the runsc runtime', async () => {
    const { container } = render(TemplatePanel, { props: panelProps() });
    await fireEvent.click(showAdvanced(container)!);
    await tick();

    await fireEvent.click(diniToggle(container)[0]);
    await tick();

    expect(screen.queryByText(/runs with full host privileges/i)).toBeTruthy();
    expect(screen.queryByText('Sandboxed via gVisor')).toBeNull();
  });
});

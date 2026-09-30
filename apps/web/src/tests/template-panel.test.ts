import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import TemplatePanel from '$lib/components/templates/TemplatePanel.svelte';
import type { EffectiveContext, Template } from '$lib/types';

vi.mock('$lib/api/client', () => ({
  api: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn()
  }
}));

import { api } from '$lib/api/client';
const mockApi = vi.mocked(api);

const editorView = { tab: 'templates', editor: 'new' } as const;

function context(overrides: Partial<EffectiveContext> = {}): EffectiveContext {
  return {
    user_id: 'me',
    username: 'me',
    is_admin: false, tier: 0,
    can_create_template: false,
    can_manage_users: false,
    can_manage_group_instances: false,
    can_manage_docker: false,
    can_manage_registry: false,
    can_view_monitoring: false,
    can_view_audit_logs: false,
    effective_max_instances: 4,
    allowed_template_ids: ['t-own'],
    group_ids: ['g1'],
    direct_max_instances: null,
    ...overrides
  };
}

function template(overrides: Partial<Template> = {}): Template {
  return {
    id: 't1',
    name: 'Tpl',
    description: '',
    owner_id: 'someone-else',
    image: 'img:1',
    cores: 2,
    memory: 4294967296,
    gpu_count: 0,
    docker_registry: '',
    remote_type: 'kasmvnc',
    persistent_storage_path: '',
    container_runtime: 'runc',
    max_run_seconds: -1,
    timeout_action: 'remove',
    keep_time_seconds: -1,
    keep_time_action: 'pause',
    network_bandwidth_up_mbps: 0,
    network_bandwidth_down_mbps: 0,
    docker_in_instance: false,
    visibility: 'private',
    run_config: {},
    exec_config: {},
    volume_mappings: {},
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

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

  it('refreshes the effective context after creating a template', async () => {
    mockApi.post.mockResolvedValue({ data: { template: { id: 't-new' } } });
    mockApi.get.mockImplementation(async (path: string) => {
      if (path === '/auth/me') {
        return { data: { context: context({ allowed_template_ids: ['t-new'] }) } };
      }
      return { data: { templates: [] } };
    });

    const { container } = render(TemplatePanel, { props: panelProps() });
    const nameInput = container.querySelector('input[placeholder="e.g. AI Lab"]') as HTMLInputElement;
    await fireEvent.input(nameInput, { target: { value: 'New VM' } });
    await fireEvent.click(screen.getByText('Create Template'));

    await waitFor(() => {
      expect(mockApi.get).toHaveBeenCalledWith('/auth/me');
    });
  });

  it('renders the new-template editor without effect looping', () => {
    expect(() => render(TemplatePanel, { props: panelProps() })).not.toThrow();
    expect(screen.getByText('New Template')).toBeTruthy();
    expect(screen.getByText('Create Template')).toBeTruthy();
  });

  it('renders the editor without an allocation mode selector', () => {
    const { container } = render(TemplatePanel, { props: panelProps() });
    expect(container.querySelector('[data-testid="allocation-mode-select"]')).toBeNull();
    expect(screen.queryByText('Allocation Mode')).toBeNull();
  });

  it('drops the form and shows the list when leaving the editor view', async () => {
    const { rerender } = render(TemplatePanel, { props: panelProps() });
    await rerender({ view: { tab: 'templates', editor: 'list' }, ctx: context({ can_create_template: true }) });
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

  function unlimitedFor(container: HTMLElement, labelText: string) {
    return container.querySelector<HTMLInputElement>(`input[aria-label="${labelText} unlimited"]`);
  }
  function limitValueInputs(container: HTMLElement) {
    return Array.from(container.querySelectorAll<HTMLInputElement>('input[placeholder="e.g. 3600 (1 hour)"]'));
  }

  it.each([
    ['usage', 'Usage Limit (seconds)', 'Timeout Action'],
    ['keep-time', 'Idle Keep Time (seconds)', 'Keep Time Action']
  ])(
    'the %s limit shares the unlimited vocabulary',
    async (_kind, labelText, actionLabel) => {
      const { container } = render(TemplatePanel, { props: panelProps() });

      const checkbox = unlimitedFor(container, labelText)!;
      expect(checkbox.checked).toBe(true);
      expect(limitValueInputs(container).length).toBe(0);

      await fireEvent.click(checkbox);
      await tick();
      expect(unlimitedFor(container, labelText)!.checked).toBe(false);
      expect(limitValueInputs(container).length).toBe(1);

      const input = limitValueInputs(container)[0];
      await fireEvent.input(input, { target: { value: '600' } });
      await tick();
      expect(screen.getByText(actionLabel)).toBeTruthy();

      await fireEvent.click(unlimitedFor(container, labelText)!);
      await tick();
      expect(unlimitedFor(container, labelText)!.checked).toBe(true);
      expect(limitValueInputs(container).length).toBe(0);
    }
  );

  function showAdvanced(container: HTMLElement) {
    const btn = Array.from(container.querySelectorAll<HTMLButtonElement>('button')).find(b => b.textContent?.includes('Step 3 of 3'));
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

  it('defaults the image to the plain variant and swaps it when DinI toggles', async () => {
    const { container } = render(TemplatePanel, { props: panelProps() });
    const imageInput = () => container.querySelector<HTMLInputElement>('input[placeholder="tsukisama9292/ow-kasmvnc-ubuntu:jammy"]')!;

    expect(imageInput().value).toBe('tsukisama9292/ow-kasmvnc-ubuntu:jammy');

    await fireEvent.click(showAdvanced(container)!);
    await tick();

    await fireEvent.click(diniToggle(container)[0]);
    await tick();
    expect(imageInput().value).toBe('tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy');

    await fireEvent.click(diniToggle(container)[0]);
    await tick();
    expect(imageInput().value).toBe('tsukisama9292/ow-kasmvnc-ubuntu:jammy');
  });

  it('keeps a custom image when DinI toggles', async () => {
    const { container } = render(TemplatePanel, { props: panelProps() });
    const imageInput = () => container.querySelector<HTMLInputElement>('input[placeholder="tsukisama9292/ow-kasmvnc-ubuntu:jammy"]')!;

    await fireEvent.input(imageInput(), { target: { value: 'registry.example.com/team/custom:latest' } });
    await tick();

    await fireEvent.click(showAdvanced(container)!);
    await tick();
    await fireEvent.click(diniToggle(container)[0]);
    await tick();
    await fireEvent.click(diniToggle(container)[0]);
    await tick();

    expect(imageInput().value).toBe('registry.example.com/team/custom:latest');
  });

  it('marks templates the user may launch and gates edit/delete on ownership', () => {
    const ctx = context({ user_id: 'me', can_create_template: true, allowed_template_ids: ['t-own'] });
    const configs = [
      template({ id: 't-own', name: 'Mine', owner_id: 'me' }),
      template({ id: 't-other', name: 'Others', owner_id: 'someone-else' })
    ];
    render(TemplatePanel, {
      props: panelProps({ view: { tab: 'templates', editor: 'list' }, configs, ctx })
    });

    expect(screen.getAllByText('May launch')).toHaveLength(1);
    expect(screen.getAllByText(/outside your whitelist/)).toHaveLength(1);
    expect(screen.getAllByText('Edit')).toHaveLength(1);
    expect(screen.getAllByText('Delete')).toHaveLength(1);
    expect(screen.getAllByText('+ New Template')).toHaveLength(1);
  });

  it('lets the system admin edit every template but launch only whitelisted ones', () => {
    const ctx = context({ is_admin: true, tier: 2, allowed_template_ids: ['t1'] });
    const configs = [
      template({ id: 't1', name: 'A', owner_id: 'a' }),
      template({ id: 't2', name: 'B', owner_id: 'b' })
    ];
    render(TemplatePanel, {
      props: panelProps({ view: { tab: 'templates', editor: 'list' }, configs, ctx })
    });

    expect(screen.getAllByText('May launch')).toHaveLength(1);
    expect(screen.getAllByText(/outside your whitelist/)).toHaveLength(1);
    expect(screen.getAllByText('Edit')).toHaveLength(2);
    expect(screen.getAllByText('Delete')).toHaveLength(2);
  });

  it('marks public templates as launchable and hidden templates as not (the API excludes hidden from the whitelist)', () => {
    const ctx = context({ user_id: 'me', can_create_template: true, allowed_template_ids: ['t-own'] });
    const configs = [
      template({ id: 't-own', name: 'Private', owner_id: 'me' }),
      template({ id: 't-public', name: 'Public', owner_id: 'someone-else', visibility: 'public' }),
      template({ id: 't-hidden', name: 'Hidden', owner_id: 'someone-else', visibility: 'hidden' })
    ];
    render(TemplatePanel, {
      props: panelProps({ view: { tab: 'templates', editor: 'list' }, configs, ctx })
    });

    expect(screen.getAllByText('May launch')).toHaveLength(2);
    expect(screen.getAllByText(/outside your whitelist/)).toHaveLength(1);
  });

  it('renders family marks as local vectors with no emoji on template cards', () => {
    const ctx = context({ allowed_template_ids: ['t-own'] });
    const configs = [template({ id: 't-own', name: 'Ubuntu box', owner_id: 'me' })];
    const { container } = render(TemplatePanel, {
      props: panelProps({ view: { tab: 'templates', editor: 'list' }, configs, ctx })
    });
    const marks = container.querySelectorAll('img.catalog-mark');
    expect(marks.length).toBe(1);
    expect(marks[0].getAttribute('src')).toBe('/icons/ubuntu.svg');
    expect(document.body.textContent).not.toMatch(/🐧|🐍|🧠|⚙|📦/);
  });

  it('shows descriptions and resource facts on template cards', () => {
    const ctx = context({ allowed_template_ids: ['t-own'] });
    const configs = [
      template({
        id: 't-own',
        name: 'Rust box',
        description: 'Systems hacking',
        owner_id: 'me',
        cores: 4,
        gpu_count: 1,
        persistent_storage_path: '/data'
      })
    ];
    render(TemplatePanel, {
      props: panelProps({ view: { tab: 'templates', editor: 'list' }, configs, ctx })
    });
    expect(screen.getByText('Systems hacking')).toBeTruthy();
    expect(screen.getByText(/4 CPU/)).toBeTruthy();
    expect(screen.getByText(/1 GPU/)).toBeTruthy();
    expect(screen.getByText(/persistent storage/)).toBeTruthy();
  });

  it('guides the editor in three numbered steps', () => {
    render(TemplatePanel, { props: panelProps() });
    expect(screen.getByText('Step 1 of 3 — Basics')).toBeTruthy();
    expect(screen.getByText('Step 2 of 3 — Resources')).toBeTruthy();
    expect(screen.getByText(/Step 3 of 3 — Advanced/)).toBeTruthy();
  });

  it('shows a visibility selector defaulting to private in the new-template editor', async () => {
    const { container } = render(TemplatePanel, { props: panelProps() });
    const select = Array.from(container.querySelectorAll<HTMLSelectElement>('select'))
      .find((s) => Array.from(s.options).some((o) => o.value === 'hidden'));

    expect(select).toBeTruthy();
    expect(select!.value).toBe('private');
    expect(Array.from(select!.options).map((o) => o.value)).toEqual(['private', 'public', 'hidden']);
  });
});

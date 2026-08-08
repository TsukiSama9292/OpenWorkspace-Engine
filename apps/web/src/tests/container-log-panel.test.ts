import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import ContainerLogPanel from '$lib/components/instances/ContainerLogPanel.svelte';
import type { ContainerLogLine, Instance } from '$lib/types';

vi.mock('$lib/api/instance-logs', () => ({
  streamInstanceLogs: vi.fn()
}));

import { streamInstanceLogs } from '$lib/api/instance-logs';
import type { LogStreamCallbacks } from '$lib/api/instance-logs';
const mockStream = vi.mocked(streamInstanceLogs);

function instance(overrides: Partial<Instance> = {}): Instance {
  return {
    id: 'i1',
    name: 'dev-1',
    template_id: 't1',
    template_name: 'Dev VM',
    remote_type: 'kasmvnc',
    owner_id: 'u1',
    owner_username: 'alice',
    owner_group_ids: [],
    owner_tier: 0,
    status: 'running',
    instance_number: 1,
    container_id: 'c1',
    mount_persistent: false,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

function captureStream() {
  let callbacks: LogStreamCallbacks | undefined;
  mockStream.mockImplementation((_id, _opts, cb) => {
    callbacks = cb;
    return { abort: vi.fn() };
  });
  return {
    onLog: (line: ContainerLogLine) => callbacks?.onLog(line),
    onEnd: (reason: string) => callbacks?.onEnd(reason),
    onError: (message: string) => callbacks?.onError(message)
  };
}

beforeEach(() => {
  localStorage.clear();
  mockStream.mockReturnValue({ abort: vi.fn() });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('ContainerLogPanel — font size', () => {
  it('defaults the font size to 13px, clamps with A-/A+, and persists', async () => {
    const { container } = render(ContainerLogPanel, { props: { instance: instance() } });
    await waitFor(() => expect(mockStream).toHaveBeenCalled());

    const body = container.querySelector('.logs-body') as HTMLElement;
    expect(body.style.fontSize).toBe('13px');

    await fireEvent.click(screen.getByLabelText('Increase log font size'));
    await fireEvent.click(screen.getByLabelText('Increase log font size'));
    expect(body.style.fontSize).toBe('15px');

    await fireEvent.click(screen.getByLabelText('Increase log font size'));
    expect(body.style.fontSize).toBe('16px');

    await fireEvent.click(screen.getByLabelText('Decrease log font size'));
    expect(body.style.fontSize).toBe('15px');
    expect(localStorage.getItem('ow-log-font-size')).toBe('15');
  });

  it('loads a persisted font size', async () => {
    localStorage.setItem('ow-log-font-size', '16');
    const { container } = render(ContainerLogPanel, { props: { instance: instance() } });
    await waitFor(() => expect(mockStream).toHaveBeenCalled());

    expect((container.querySelector('.logs-body') as HTMLElement).style.fontSize).toBe('16px');
  });
});

describe('ContainerLogPanel — line layout', () => {
  it('switches the line-layout mode with the Wrap toggle (default on)', async () => {
    const { container } = render(ContainerLogPanel, { props: { instance: instance() } });
    await waitFor(() => expect(mockStream).toHaveBeenCalled());

    const body = container.querySelector('.logs-body') as HTMLElement;
    expect(body.classList.contains('nowrap')).toBe(false);

    await fireEvent.click(screen.getByRole('checkbox', { name: /Wrap/ }));
    expect(body.classList.contains('nowrap')).toBe(true);

    await fireEvent.click(screen.getByRole('checkbox', { name: /Wrap/ }));
    expect(body.classList.contains('nowrap')).toBe(false);
  });

  it('renders line numbers and stream gutters instead of O/E letterboxes', async () => {
    const stream = captureStream();
    const { container } = render(ContainerLogPanel, { props: { instance: instance() } });
    await waitFor(() => expect(mockStream).toHaveBeenCalled());

    stream.onLog({ stream: 'stdout', text: 'hello' });
    stream.onLog({ stream: 'stderr', text: 'boom' });
    await tick();

    const lines = container.querySelectorAll('.log-line');
    expect(lines.length).toBe(2);
    expect(container.querySelector('.log-line .log-line-num')?.textContent).toBe('1');
    expect(container.querySelector('.log-line.stderr .log-line-num')?.textContent).toBe('2');
    expect(container.querySelector('.log-line .log-gutter')).toBeTruthy();
    expect(container.querySelector('.log-line.stderr')).toBeTruthy();
    expect(screen.queryByText('O')).toBeNull();
    expect(screen.queryByText('E')).toBeNull();
  });
});

describe('ContainerLogPanel — follow & scroll', () => {
  it('pins to the newest line while following, pauses on upward scroll, resumes at the bottom', async () => {
    const stream = captureStream();
    const { container } = render(ContainerLogPanel, { props: { instance: instance() } });
    await waitFor(() => expect(mockStream).toHaveBeenCalled());

    const body = container.querySelector('.logs-body') as HTMLElement;
    Object.defineProperty(body, 'scrollHeight', { configurable: true, value: 1000 });
    Object.defineProperty(body, 'clientHeight', { configurable: true, value: 500 });

    stream.onLog({ stream: 'stdout', text: 'line 1' });
    await tick();
    expect(body.scrollTop).toBe(1000);

    body.scrollTop = 10;
    await fireEvent.scroll(body);
    expect(screen.getByText(/paused — scroll to bottom to resume/)).toBeTruthy();

    stream.onLog({ stream: 'stdout', text: 'line 2' });
    await tick();
    expect(body.scrollTop).toBe(10);

    body.scrollTop = 1000;
    await fireEvent.scroll(body);
    expect(screen.getByText('streaming')).toBeTruthy();

    stream.onLog({ stream: 'stdout', text: 'line 3' });
    await tick();
    expect(body.scrollTop).toBe(1000);
  });

  it('labels follow-off as static, not as paused-by-scroll', async () => {
    render(ContainerLogPanel, { props: { instance: instance() } });
    await waitFor(() => expect(mockStream).toHaveBeenCalled());
    expect(screen.getByText('streaming')).toBeTruthy();

    await fireEvent.click(screen.getByRole('checkbox', { name: /Follow/ }));
    expect(screen.getByText('static')).toBeTruthy();
    expect(screen.queryByText(/paused — scroll to bottom to resume/)).toBeNull();
  });
});

describe('ContainerLogPanel — modal & states', () => {
  it('toggles the fullscreen class on the modal', async () => {
    const { container } = render(ContainerLogPanel, { props: { instance: instance() } });
    await waitFor(() => expect(mockStream).toHaveBeenCalled());

    const modal = container.querySelector('.logs-modal') as HTMLElement;
    expect(modal.classList.contains('fullscreen')).toBe(false);

    await fireEvent.click(screen.getByText('Fullscreen'));
    expect(modal.classList.contains('fullscreen')).toBe(true);

    await fireEvent.click(screen.getByText('Exit'));
    expect(modal.classList.contains('fullscreen')).toBe(false);
  });

  it('shows waiting-for-output while streaming and the ended label on stream close', async () => {
    const stream = captureStream();
    const { container } = render(ContainerLogPanel, { props: { instance: instance() } });
    await waitFor(() => expect(mockStream).toHaveBeenCalled());

    expect(screen.getByText('Waiting for output…')).toBeTruthy();

    stream.onEnd('stopped');
    await tick();

    expect(screen.getByText(/Session ended — the instance was stopped/)).toBeTruthy();
    expect(container.querySelector('.logs-body')).toBeTruthy();
  });
});

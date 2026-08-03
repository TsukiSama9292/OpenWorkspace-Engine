import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import AdminSettings from '$lib/components/AdminSettings.svelte';

const MOCK_SETTINGS = {
  max_cpu_cores: 8,
  max_ram_bytes: 17179869184,
  host_instance_limit: 0,
  shared_max_cpu: 0,
  shared_max_ram: 0,
};

function mockFetch(ok: boolean, status: number, body: unknown) {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
    ok,
    status,
    text: () => Promise.resolve(JSON.stringify(body))
  }));
}

describe('AdminSettings', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('loads and displays all five values', async () => {
    mockFetch(true, 200, { settings: MOCK_SETTINGS });

    render(AdminSettings);

    await waitFor(() => {
      expect((screen.getByLabelText('Max CPU Cores') as HTMLInputElement).value).toBe('8');
      expect((screen.getByLabelText('Max RAM Bytes') as HTMLInputElement).value).toBe('17179869184');
      expect((screen.getByLabelText('Instance Limit (0 = unlimited)') as HTMLInputElement).value).toBe('0');
      expect((screen.getByLabelText('Shared Max CPU (0 = off)') as HTMLInputElement).value).toBe('0');
      expect((screen.getByLabelText('Shared Max RAM (0 = off)') as HTMLInputElement).value).toBe('0');
    });
  });

  it('surfaces a load error from the API', async () => {
    mockFetch(false, 403, { error: 'Forbidden' });

    render(AdminSettings);

    await waitFor(() => {
      expect(screen.getByText('Forbidden')).toBeTruthy();
    });
  });

  it('round-trips updated values to the API on save', async () => {
    const fetchMock = vi.fn();
    fetchMock.mockResolvedValueOnce({
      ok: true,
      status: 200,
      text: () => Promise.resolve(JSON.stringify({ settings: MOCK_SETTINGS }))
    });
    fetchMock.mockResolvedValueOnce({
      ok: true,
      status: 200,
      text: () => Promise.resolve(JSON.stringify({
        settings: { ...MOCK_SETTINGS, max_cpu_cores: 16, host_instance_limit: 4 }
      }))
    });
    vi.stubGlobal('fetch', fetchMock);

    render(AdminSettings);

    await waitFor(() => {
      expect((screen.getByLabelText('Max CPU Cores') as HTMLInputElement).value).toBe('8');
    });

    const cpu = screen.getByLabelText('Max CPU Cores') as HTMLInputElement;
    await fireEvent.input(cpu, { target: { value: '16' } });
    const limit = screen.getByLabelText('Instance Limit (0 = unlimited)') as HTMLInputElement;
    await fireEvent.input(limit, { target: { value: '4' } });

    await fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      const putCall = fetchMock.mock.calls.find(
        ([, options]) => (options as RequestInit).method === 'PUT'
      );
      expect(putCall).toBeTruthy();
      const [, options] = putCall as [string, RequestInit];
      expect(options.method).toBe('PUT');
      expect(JSON.parse(options.body as string)).toEqual({
        max_cpu_cores: 16,
        max_ram_bytes: 17179869184,
        host_instance_limit: 4,
        shared_max_cpu: 0,
        shared_max_ram: 0
      });
    });

    await waitFor(() => {
      expect((screen.getByLabelText('Max CPU Cores') as HTMLInputElement).value).toBe('16');
      expect(screen.getByText('Saved')).toBeTruthy();
    });
  });

  it('surfaces a save error from the API', async () => {
    const fetchMock = vi.fn();
    fetchMock.mockResolvedValueOnce({
      ok: true,
      status: 200,
      text: () => Promise.resolve(JSON.stringify({ settings: MOCK_SETTINGS }))
    });
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 400,
      text: () => Promise.resolve(JSON.stringify({ error: 'Invalid negative value' }))
    });
    vi.stubGlobal('fetch', fetchMock);

    render(AdminSettings);

    await waitFor(() => {
      expect((screen.getByLabelText('Max CPU Cores') as HTMLInputElement).value).toBe('8');
    });

    await fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(screen.getByText('Invalid negative value')).toBeTruthy();
    });
  });
});

import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import AdminSettings from '$lib/components/AdminSettings.svelte';

const MOCK_SETTINGS = {
  host_instance_limit: 5,
  host_cpu_cores: -1,
  host_memory_mb: 16384,
  host_gpu_count: 0
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

  it('loads and displays the host caps as tri-state inputs', async () => {
    mockFetch(true, 200, { settings: MOCK_SETTINGS });

    render(AdminSettings);

    await waitFor(() => {
      expect((screen.getByLabelText('Global instance limit value') as HTMLInputElement).value).toBe('5');
    });
    expect((screen.getByLabelText('Global instance limit mode') as HTMLSelectElement).value).toBe('custom');
    expect((screen.getByLabelText('Host CPU (cores) mode') as HTMLSelectElement).value).toBe('unlimited');
    expect((screen.getByLabelText('Host memory (GB) value') as HTMLInputElement).value).toBe('16');
    expect((screen.getByLabelText('Host GPU mode') as HTMLSelectElement).value).toBe('disabled');
  });

  it('surfaces a load error from the API', async () => {
    mockFetch(false, 403, { error: 'Forbidden' });

    render(AdminSettings);

    await waitFor(() => {
      expect(screen.getByText('Forbidden')).toBeTruthy();
    });
  });

  it('round-trips updated host caps to the API on save', async () => {
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
        settings: { ...MOCK_SETTINGS, host_instance_limit: 4 }
      }))
    });
    vi.stubGlobal('fetch', fetchMock);

    render(AdminSettings);

    await waitFor(() => {
      expect((screen.getByLabelText('Global instance limit value') as HTMLInputElement).value).toBe('5');
    });

    const limit = screen.getByLabelText('Global instance limit value') as HTMLInputElement;
    await fireEvent.input(limit, { target: { value: '4' } });

    const cpuMode = screen.getByLabelText('Host CPU (cores) mode') as HTMLSelectElement;
    await fireEvent.change(cpuMode, { target: { value: 'custom' } });
    const cpuValue = screen.getByLabelText('Host CPU (cores) value') as HTMLInputElement;
    await fireEvent.input(cpuValue, { target: { value: '8' } });

    const gpuMode = screen.getByLabelText('Host GPU mode') as HTMLSelectElement;
    await fireEvent.change(gpuMode, { target: { value: 'custom' } });
    const gpuValue = screen.getByLabelText('Host GPU value') as HTMLInputElement;
    await fireEvent.input(gpuValue, { target: { value: '2' } });

    await fireEvent.click(screen.getByText('Save Changes'));

    await waitFor(() => {
      const putCall = fetchMock.mock.calls.find(
        ([, options]) => (options as RequestInit).method === 'PUT'
      );
      expect(putCall).toBeTruthy();
      const [, options] = putCall as [string, RequestInit];
      expect(options.method).toBe('PUT');
      expect(JSON.parse(options.body as string)).toEqual({
        host_instance_limit: 4,
        host_cpu_cores: 8,
        host_memory_mb: 16384,
        host_gpu_count: 2
      });
    });

    await waitFor(() => {
      expect((screen.getByLabelText('Global instance limit value') as HTMLInputElement).value).toBe('4');
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
      expect((screen.getByLabelText('Global instance limit value') as HTMLInputElement).value).toBe('5');
    });

    await fireEvent.click(screen.getByText('Save Changes'));

    await waitFor(() => {
      expect(screen.getByText('Invalid negative value')).toBeTruthy();
    });
  });
});

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Template } from '$lib/types';
import {
  createInitialFormState,
  formStateFromTemplate,
  loadTemplate,
  submitTemplate,
  updateTemplate,
  type TemplateFormState
} from '$lib/templates/template-form';

function jsonResponse(body: unknown) {
  return {
    ok: true,
    text: () => Promise.resolve(JSON.stringify(body))
  };
}

const template: Template = {
  id: 'tpl-1',
  name: ' Dev VM ',
  description: 'My dev box',
  image: 'img:1',
  cores: 4,
  memory: 8589934592,
  gpu_count: 1,
  docker_registry: 'reg.example.com',
  remote_type: 'kasmvnc',
  persistent_storage_path: '/data/dev',
  container_runtime: 'docker',
  max_run_seconds: 7200,
  timeout_action: 'stop',
  keep_time_seconds: 3600,
  keep_time_action: 'pause',
  network_bandwidth_up_mbps: 100,
  network_bandwidth_down_mbps: 50,
  docker_in_instance: true,
  run_config: {
    hostname: 'devbox',
    dns: ['8.8.8.8', '1.1.1.1'],
    shm_size: 268435456,
    network_mode: 'bridge',
    environment: ['FOO=bar', 'EMPTY=']
  },
  exec_config: { go: { cmd: 'bash -c "echo hi"' } },
  volume_mappings: { '/host/path': '/container/path' },
  created_at: '2024-01-01T00:00:00Z',
  updated_at: '2024-01-01T00:00:00Z'
};

const populatedState: TemplateFormState = {
  name: '  Dev  ',
  description: 'Notes',
  image: 'img:1',
  cores: 2,
  ramGb: 4,
  gpuCount: 0,
  dockerRegistry: 'reg',
  persistentStoragePath: '/data',
  remoteType: 'ttyd',
  hostname: 'host1',
  dns: '8.8.8.8, 1.1.1.1',
  shmSize: '268435456',
  networkMode: 'bridge',
  containerRuntime: 'docker',
  maxRunSeconds: 3600,
  timeoutAction: 'stop',
  keepTimeSeconds: 3600,
  keepTimeAction: 'stop',
  bandwidthUpMbps: 50,
  bandwidthDownMbps: 25,
  dockerInInstance: true,
  envVars: [{ key: 'FOO', value: 'bar' }],
  execCommand: 'bash',
  volumeMappings: [{ host: '/h', container: '/c' }],
  showAdvanced: true,
  loading: false,
  error: ''
};

describe('template-form', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  describe('createInitialFormState', () => {
    it('returns the default empty form state', () => {
      const state = createInitialFormState();
      expect(state.name).toBe('');
      expect(state.image).toBe('tsukisama9292/ow-kasmvnc-ubuntu:jammy');
      expect(state.cores).toBe(2);
      expect(state.ramGb).toBe(4);
      expect(state.gpuCount).toBe(0);
      expect(state.remoteType).toBe('kasmvnc');
      expect(state.maxRunSeconds).toBeNull();
      expect(state.timeoutAction).toBe('remove');
      expect(state.keepTimeSeconds).toBeNull();
      expect(state.keepTimeAction).toBe('pause');
      expect(state.dockerInInstance).toBe(false);
      expect(state.envVars).toEqual([{ key: '', value: '' }]);
      expect(state.volumeMappings).toEqual([{ host: '', container: '' }]);
    });
  });

  describe('submitTemplate', () => {
    it('POSTs the built payload and returns the new id', async () => {
      const mockFetch = vi.fn().mockResolvedValue(jsonResponse({ template: { id: 'tpl-new' } }));
      vi.stubGlobal('fetch', mockFetch);

      const result = await submitTemplate(populatedState);

      expect(result).toEqual({ id: 'tpl-new' });
      expect(mockFetch).toHaveBeenCalledWith('/api/templates', expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          name: 'Dev',
          description: 'Notes',
          image: 'img:1',
          cores: 2,
          memory: 4294967296,
          gpu_count: 0,
          container_runtime: 'docker',
          docker_registry: 'reg',
          remote_type: 'ttyd',
          run_config: {
            hostname: 'host1',
            dns: ['8.8.8.8', '1.1.1.1'],
            shm_size: 268435456,
            network_mode: 'bridge',
            environment: ['FOO=bar']
          },
          exec_config: { go: { cmd: 'bash' } },
          volume_mappings: { '/h': '/c' },
          persistent_storage_path: '/data',
          max_run_seconds: 3600,
          timeout_action: 'stop',
          keep_time_seconds: 3600,
          keep_time_action: 'stop',
          network_bandwidth_up_mbps: 50,
          network_bandwidth_down_mbps: 25,
          docker_in_instance: true
        })
      }));
    });

    it('sends null for empty optional fields and omits empty config sections', async () => {
      const mockFetch = vi.fn().mockResolvedValue(jsonResponse({ template: { id: 'tpl-new' } }));
      vi.stubGlobal('fetch', mockFetch);

      await submitTemplate({ ...createInitialFormState(), name: 'X' });

      expect(mockFetch).toHaveBeenCalledWith('/api/templates', expect.objectContaining({
        body: JSON.stringify({
          name: 'X',
          description: null,
          image: 'tsukisama9292/ow-kasmvnc-ubuntu:jammy',
          cores: 2,
          memory: 4294967296,
          gpu_count: 0,
          container_runtime: '',
          docker_registry: null,
          remote_type: 'kasmvnc',
          run_config: {},
          exec_config: {},
          volume_mappings: {},
          persistent_storage_path: null,
          max_run_seconds: null,
          timeout_action: 'remove',
          keep_time_seconds: null,
          keep_time_action: 'pause',
          network_bandwidth_up_mbps: 0,
          network_bandwidth_down_mbps: 0,
          docker_in_instance: false
        })
      }));
    });

    it('rejects an empty name without calling the API', async () => {
      const mockFetch = vi.fn();
      vi.stubGlobal('fetch', mockFetch);

      const result = await submitTemplate(createInitialFormState());

      expect(result).toEqual({ error: 'Name is required' });
      expect(mockFetch).not.toHaveBeenCalled();
    });

    it('surfaces API errors', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: false,
        status: 400,
        text: () => Promise.resolve(JSON.stringify({ error: 'Bad image' }))
      }));

      const result = await submitTemplate({ ...createInitialFormState(), name: 'X' });
      expect(result).toEqual({ error: 'Bad image' });
    });

    it('rejects a negative upload bandwidth without calling the API', async () => {
      const mockFetch = vi.fn();
      vi.stubGlobal('fetch', mockFetch);

      const result = await submitTemplate({ ...createInitialFormState(), name: 'X', bandwidthUpMbps: -1 });
      expect(result).toEqual({ error: 'Upload bandwidth must be >= 0 (0 = unlimited)' });
      expect(mockFetch).not.toHaveBeenCalled();
    });

    it('rejects a negative download bandwidth without calling the API', async () => {
      const mockFetch = vi.fn();
      vi.stubGlobal('fetch', mockFetch);

      const result = await submitTemplate({ ...createInitialFormState(), name: 'X', bandwidthDownMbps: -5 });
      expect(result).toEqual({ error: 'Download bandwidth must be >= 0 (0 = unlimited)' });
      expect(mockFetch).not.toHaveBeenCalled();
    });

    it('sends null keep-time and the pause action when keep-time is disabled', async () => {
      const mockFetch = vi.fn().mockResolvedValue(jsonResponse({ template: { id: 'tpl-new' } }));
      vi.stubGlobal('fetch', mockFetch);

      await submitTemplate({ ...createInitialFormState(), name: 'X' });

      const body = JSON.parse(mockFetch.mock.calls[0][1].body as string);
      expect(body.keep_time_seconds).toBeNull();
      expect(body.keep_time_action).toBe('pause');
    });

    it('sends keep-time duration and action when enabled', async () => {
      const mockFetch = vi.fn().mockResolvedValue(jsonResponse({ template: { id: 'tpl-new' } }));
      vi.stubGlobal('fetch', mockFetch);

      await submitTemplate({
        ...createInitialFormState(),
        name: 'X',
        keepTimeSeconds: 600,
        keepTimeAction: 'stop'
      });

      const body = JSON.parse(mockFetch.mock.calls[0][1].body as string);
      expect(body.keep_time_seconds).toBe(600);
      expect(body.keep_time_action).toBe('stop');
    });
  });

  describe('formStateFromTemplate', () => {
    it('maps a Template into editable form state', () => {
      const state = formStateFromTemplate(template);
      expect(state).toEqual({
        name: ' Dev VM ',
        description: 'My dev box',
        image: 'img:1',
        cores: 4,
        ramGb: 8,
        gpuCount: 1,
        dockerRegistry: 'reg.example.com',
        persistentStoragePath: '/data/dev',
        remoteType: 'kasmvnc',
        hostname: 'devbox',
        dns: '8.8.8.8, 1.1.1.1',
        shmSize: '268435456',
        networkMode: 'bridge',
        containerRuntime: 'docker',
        maxRunSeconds: 7200,
        timeoutAction: 'stop',
        keepTimeSeconds: 3600,
        keepTimeAction: 'pause',
        bandwidthUpMbps: 100,
        bandwidthDownMbps: 50,
        dockerInInstance: true,
        envVars: [
          { key: 'FOO', value: 'bar' },
          { key: 'EMPTY', value: '' }
        ],
        execCommand: 'bash -c "echo hi"',
        volumeMappings: [{ host: '/host/path', container: '/container/path' }],
        showAdvanced: false,
        loading: false,
        error: ''
      });
    });

    it('defaults missing config sections', () => {
      const state = formStateFromTemplate({ ...template, run_config: {}, exec_config: {}, volume_mappings: {} });
      expect(state.hostname).toBe('');
      expect(state.dns).toBe('');
      expect(state.shmSize).toBe('');
      expect(state.networkMode).toBe('');
      expect(state.envVars).toEqual([{ key: '', value: '' }]);
      expect(state.execCommand).toBe('');
      expect(state.volumeMappings).toEqual([{ host: '', container: '' }]);
    });

    it('maps auto-sleep fields, defaulting a missing duration to off', () => {
      const state = formStateFromTemplate({ ...template, max_run_seconds: null, timeout_action: 'remove' });
      expect(state.maxRunSeconds).toBeNull();
      expect(state.timeoutAction).toBe('remove');
    });

    it('defaults missing bandwidth fields to zero', () => {
      const { network_bandwidth_up_mbps, network_bandwidth_down_mbps, ...rest } = template;
      const state = formStateFromTemplate(rest as Template);
      expect(state.bandwidthUpMbps).toBe(0);
      expect(state.bandwidthDownMbps).toBe(0);
    });

    it('defaults a missing docker_in_instance to off', () => {
      const { docker_in_instance, ...rest } = template;
      const state = formStateFromTemplate(rest as Template);
      expect(state.dockerInInstance).toBe(false);
    });

    it('prefills keep-time fields from a template', () => {
      const state = formStateFromTemplate({ ...template, keep_time_seconds: 600, keep_time_action: 'stop' });
      expect(state.keepTimeSeconds).toBe(600);
      expect(state.keepTimeAction).toBe('stop');
    });

    it('maps a missing keep-time duration to off with the pause default', () => {
      const state = formStateFromTemplate({ ...template, keep_time_seconds: null, keep_time_action: 'pause' });
      expect(state.keepTimeSeconds).toBeNull();
      expect(state.keepTimeAction).toBe('pause');
    });
  });

  describe('loadTemplate', () => {
    it('GETs the template and returns its form state', async () => {
      const mockFetch = vi.fn().mockResolvedValue(jsonResponse({ template }));
      vi.stubGlobal('fetch', mockFetch);

      const result = await loadTemplate('tpl-1');

      expect(result.state?.name).toBe(' Dev VM ');
      expect(result.error).toBeUndefined();
      expect(mockFetch).toHaveBeenCalledWith('/api/templates/tpl-1', expect.objectContaining({ method: 'GET' }));
    });

    it('surfaces API errors', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: false,
        status: 404,
        text: () => Promise.resolve(JSON.stringify({ error: 'Not found' }))
      }));

      const result = await loadTemplate('tpl-1');
      expect(result).toEqual({ error: 'Not found' });
    });

    it('reports a missing template body', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ template: null })));

      const result = await loadTemplate('tpl-1');
      expect(result).toEqual({ error: 'Template not found' });
    });
  });

  describe('updateTemplate', () => {
    it('PUTs the built payload to the template id', async () => {
      const mockFetch = vi.fn().mockResolvedValue(jsonResponse({ template: { id: 'tpl-1' } }));
      vi.stubGlobal('fetch', mockFetch);

      const result = await updateTemplate('tpl-1', populatedState);

      expect(result).toEqual({});
      expect(mockFetch).toHaveBeenCalledWith('/api/templates/tpl-1', expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({
          name: 'Dev',
          description: 'Notes',
          image: 'img:1',
          cores: 2,
          memory: 4294967296,
          gpu_count: 0,
          container_runtime: 'docker',
          docker_registry: 'reg',
          remote_type: 'ttyd',
          run_config: {
            hostname: 'host1',
            dns: ['8.8.8.8', '1.1.1.1'],
            shm_size: 268435456,
            network_mode: 'bridge',
            environment: ['FOO=bar']
          },
          exec_config: { go: { cmd: 'bash' } },
          volume_mappings: { '/h': '/c' },
          persistent_storage_path: '/data',
          max_run_seconds: 3600,
          timeout_action: 'stop',
          keep_time_seconds: 3600,
          keep_time_action: 'stop',
          network_bandwidth_up_mbps: 50,
          network_bandwidth_down_mbps: 25,
          docker_in_instance: true
        })
      }));
    });

    it('rejects an empty name without calling the API', async () => {
      const mockFetch = vi.fn();
      vi.stubGlobal('fetch', mockFetch);

      const result = await updateTemplate('tpl-1', createInitialFormState());
      expect(result).toEqual({ error: 'Name is required' });
      expect(mockFetch).not.toHaveBeenCalled();
    });

    it('rejects a negative upload bandwidth without calling the API', async () => {
      const mockFetch = vi.fn();
      vi.stubGlobal('fetch', mockFetch);

      const result = await updateTemplate('tpl-1', { ...createInitialFormState(), name: 'X', bandwidthUpMbps: -3 });
      expect(result).toEqual({ error: 'Upload bandwidth must be >= 0 (0 = unlimited)' });
      expect(mockFetch).not.toHaveBeenCalled();
    });
  });
});

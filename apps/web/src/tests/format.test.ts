import { describe, it, expect } from 'vitest';
import {
  formatMemory,
  buildRunConfig,
  buildExecConfig,
  buildVolumeMappings,
  createEmptyEnvVar,
  createEmptyVolume
} from '$lib/utils/format';

describe('formatMemory', () => {
  it('returns dash for null or undefined', () => {
    expect(formatMemory(null)).toBe('—');
    expect(formatMemory(undefined)).toBe('—');
    expect(formatMemory(0)).toBe('—');
  });

  it('formats bytes as MB when below 1 GB', () => {
    expect(formatMemory(512 * 1024 * 1024)).toBe('512 MB');
    expect(formatMemory(100 * 1024 * 1024)).toBe('100 MB');
  });

  it('formats bytes as GB when 1 GB or more', () => {
    expect(formatMemory(1024 * 1024 * 1024)).toBe('1 GB');
    expect(formatMemory(4 * 1024 * 1024 * 1024)).toBe('4 GB');
    expect(formatMemory(8.5 * 1024 * 1024 * 1024)).toBe('8.5 GB');
  });
});

describe('buildRunConfig', () => {
  it('returns empty object when all fields are empty', () => {
    expect(buildRunConfig({ hostname: '', dns: '', shmSize: '', networkMode: '', envVars: [] })).toEqual({});
  });

  it('builds config with hostname', () => {
    const result = buildRunConfig({ hostname: 'test-host', dns: '', shmSize: '', networkMode: '', envVars: [] });
    expect(result).toEqual({ hostname: 'test-host' });
  });

  it('parses comma-separated DNS entries', () => {
    const result = buildRunConfig({ hostname: '', dns: '1.1.1.1, 8.8.8.8', shmSize: '', networkMode: '', envVars: [] });
    expect(result.dns).toEqual(['1.1.1.1', '8.8.8.8']);
  });

  it('parses SHM size as integer', () => {
    const result = buildRunConfig({ hostname: '', dns: '', shmSize: '67108864', networkMode: '', envVars: [] });
    expect(result.shm_size).toBe(67108864);
  });

  it('includes environment variables with key=value format', () => {
    const result = buildRunConfig({
      hostname: '', dns: '', shmSize: '', networkMode: '',
      envVars: [{ key: 'FOO', value: 'bar' }, { key: '', value: 'skip' }]
    });
    expect(result.environment).toEqual(['FOO=bar']);
  });
});

describe('buildExecConfig', () => {
  it('returns empty object for empty command', () => {
    expect(buildExecConfig('')).toEqual({});
    expect(buildExecConfig('   ')).toEqual({});
  });

  it('wraps command in go object', () => {
    expect(buildExecConfig('bash -c echo hello')).toEqual({ go: { cmd: 'bash -c echo hello' } });
  });

  it('trims whitespace from command', () => {
    expect(buildExecConfig('  echo hello  ')).toEqual({ go: { cmd: 'echo hello' } });
  });
});

describe('buildVolumeMappings', () => {
  it('returns empty object for empty input', () => {
    expect(buildVolumeMappings([])).toEqual({});
  });

  it('filters out incomplete mappings', () => {
    expect(buildVolumeMappings([{ host: '/host', container: '' }])).toEqual({});
    expect(buildVolumeMappings([{ host: '', container: '/container' }])).toEqual({});
  });

  it('builds host-to-container mapping', () => {
    expect(buildVolumeMappings([
      { host: '/data', container: '/app/data' },
      { host: '/config', container: '/app/config' }
    ])).toEqual({ '/data': '/app/data', '/config': '/app/config' });
  });
});

describe('createEmptyEnvVar', () => {
  it('returns empty key and value', () => {
    expect(createEmptyEnvVar()).toEqual({ key: '', value: '' });
  });
});

describe('createEmptyVolume', () => {
  it('returns empty host and container', () => {
    expect(createEmptyVolume()).toEqual({ host: '', container: '' });
  });
});

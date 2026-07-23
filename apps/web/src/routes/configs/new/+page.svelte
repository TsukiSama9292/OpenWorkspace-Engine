<script>
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import { buildRunConfig, buildExecConfig, buildVolumeMappings, createEmptyEnvVar, createEmptyVolume } from './utils.js';
  import './new-config.css';

  let name = $state('');
  let description = $state('');
  let image = $state('kasmweb/desktop:1.19.0-rolling-daily');
  let cores = $state(2);
  let ramGb = $state(4);
  let gpuCount = $state(0);
  let dockerRegistry = $state('');
  let persistentStoragePath = $state('');

  let hostname = $state('');
  let dns = $state('');
  let shmSize = $state('');
  let networkMode = $state('');
  let envVars = $state([createEmptyEnvVar()]);

  let execCommand = $state('');
  let volumeMappings = $state([createEmptyVolume()]);

  let showAdvanced = $state(false);
  let loading = $state(false);
  let error = $state('');

  const STORAGE_HINT = '/data/persistent/{workspace_name}/{user_id}';

  function addEnvVar() { envVars = [...envVars, createEmptyEnvVar()]; }
  function removeEnvVar(i) { envVars = envVars.filter((_, idx) => idx !== i); }
  function addVolume() { volumeMappings = [...volumeMappings, createEmptyVolume()]; }
  function removeVolume(i) { volumeMappings = volumeMappings.filter((_, idx) => idx !== i); }

  async function createConfig() {
    if (!name.trim()) { error = 'Name is required'; return; }
    loading = true;
    error = '';

    const body = {
      name: name.trim(),
      description: description || null,
      image,
      cores,
      memory: ramGb * 1024 * 1024 * 1024,
      gpu_count: gpuCount,
      docker_registry: dockerRegistry || null,
      run_config: buildRunConfig({ hostname, dns, shmSize, networkMode, envVars }),
      exec_config: buildExecConfig(execCommand),
      volume_mappings: buildVolumeMappings(volumeMappings),
      persistent_storage_path: persistentStoragePath || null,
    };

    const res = await api.post('/configs', body);
    loading = false;
    if (res.error) { error = res.error; }
    else if (res.data?.config) { goto(`/configs/${res.data.config.id}/`); }
    else { error = 'Failed to create config'; }
  }
</script>

<div class="new-config">
  <h1>New Config</h1>

  <form onsubmit={createConfig}>
    <label>
      Name *
      <input type="text" bind:value={name} placeholder="e.g. AI Lab" required />
    </label>

    <label>
      Description
      <input type="text" bind:value={description} placeholder="Optional description" />
    </label>

    <label>
      Image *
      <input type="text" bind:value={image} placeholder="kasmweb/desktop:1.19.0-rolling-daily" />
    </label>

    <div class="row">
      <label>
        CPU Cores *
        <input type="number" bind:value={cores} min="1" max="64" />
      </label>
      <label>
        RAM (GB) *
        <input type="number" bind:value={ramGb} min="1" max="256" />
      </label>
      <label>
        GPU
        <input type="number" bind:value={gpuCount} min="0" max="8" />
      </label>
    </div>

    <label>
      Docker Registry
      <input type="text" bind:value={dockerRegistry} placeholder="https://index.docker.io/v1/" />
    </label>

    <label>
      Persistent Storage Path
      <input type="text" bind:value={persistentStoragePath} placeholder={STORAGE_HINT} />
      <span class="hint">Template variables: {'{'}workspace_name{'}'}, {'{'}user_id{'}'}</span>
    </label>

    <button type="button" class="toggle-advanced" onclick={() => showAdvanced = !showAdvanced}>
      {showAdvanced ? '▾ Hide Advanced' : '▸ Show Advanced'}
    </button>

    {#if showAdvanced}
      <div class="advanced-section">
        <h2>Run Config</h2>
        <label>
          Hostname
          <input type="text" bind:value={hostname} placeholder="kasm-ubuntu" />
        </label>
        <label>
          DNS (comma-separated)
          <input type="text" bind:value={dns} placeholder="1.1.1.1, 8.8.8.8" />
        </label>
        <div class="row">
          <label>
            SHM Size (bytes)
            <input type="number" bind:value={shmSize} placeholder="67108864" />
          </label>
          <label>
            Network Mode
            <input type="text" bind:value={networkMode} placeholder="bridge" />
          </label>
        </div>

        <h3>Environment Variables</h3>
        {#each envVars as _, i}
          <div class="kv-row">
            <input type="text" bind:value={envVars[i].key} placeholder="KEY" />
            <input type="text" bind:value={envVars[i].value} placeholder="value" />
            <button type="button" class="remove-btn" onclick={() => removeEnvVar(i)}>×</button>
          </div>
        {/each}
        <button type="button" class="add-btn" onclick={addEnvVar}>+ Add Variable</button>

        <h2>Exec Config</h2>
        <label>
          Post-start Command
          <input type="text" bind:value={execCommand} placeholder="bash -c 'echo hello'" />
        </label>

        <h2>Volume Mappings</h2>
        {#each volumeMappings as _, i}
          <div class="kv-row">
            <input type="text" bind:value={volumeMappings[i].host} placeholder="/host/path" />
            <input type="text" bind:value={volumeMappings[i].container} placeholder="/container/path" />
            <button type="button" class="remove-btn" onclick={() => removeVolume(i)}>×</button>
          </div>
        {/each}
        <button type="button" class="add-btn" onclick={addVolume}>+ Add Volume</button>
      </div>
    {/if}

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <div class="actions">
      <a href="/">Cancel</a>
      <button type="submit" disabled={loading}>
        {loading ? 'Creating...' : 'Create Config'}
      </button>
    </div>
  </form>
</div>

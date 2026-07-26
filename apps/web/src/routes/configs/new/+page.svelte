<script lang="ts">
  import { goto } from '$app/navigation';
  import { submitConfig, createInitialFormState } from './config-create';
  import ConfigBasics from '$lib/components/forms/ConfigBasics.svelte';
  import ConfigResources from '$lib/components/forms/ConfigResources.svelte';
  import ConfigAdvanced from '$lib/components/forms/ConfigAdvanced.svelte';

  let form = $state(createInitialFormState());

  async function onSubmit() {
    form.loading = true;
    form.error = '';
    const result = await submitConfig(form);
    form.loading = false;
    if (result.error) form.error = result.error;
  }
</script>

<div class="max-w-xl mx-auto">
  <h1 class="text-2xl font-bold text-surface-800 mb-6">New Config</h1>

  <form class="flex flex-col gap-4" onsubmit={onSubmit}>
    <ConfigBasics bind:name={form.name} bind:description={form.description} bind:image={form.image} />
    <ConfigResources bind:cores={form.cores} bind:ramGb={form.ramGb} bind:gpuCount={form.gpuCount} bind:dockerRegistry={form.dockerRegistry} bind:persistentStoragePath={form.persistentStoragePath} />

    <button type="button" class="text-sm text-surface-500 hover:text-surface-700 bg-transparent border-none cursor-pointer text-left p-0" onclick={() => form.showAdvanced = !form.showAdvanced}>
      {form.showAdvanced ? '▾ Hide Advanced' : '▸ Show Advanced'}
    </button>

    {#if form.showAdvanced}
      <ConfigAdvanced bind:hostname={form.hostname} bind:dns={form.dns} bind:shmSize={form.shmSize} bind:networkMode={form.networkMode} bind:envVars={form.envVars} bind:execCommand={form.execCommand} bind:volumeMappings={form.volumeMappings} />
    {/if}

    {#if form.error}
      <p class="text-error-500 text-sm m-0">{form.error}</p>
    {/if}

    <div class="flex justify-end gap-3 mt-2">
      <a href="/" class="px-4 py-2 text-surface-500 no-underline hover:text-surface-700">Cancel</a>
      <button
        type="submit"
        disabled={form.loading}
        class="px-4 py-2 bg-primary-500 text-white border-none rounded cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed hover:bg-primary-600 transition-colors"
      >
        {form.loading ? 'Creating...' : 'Create Config'}
      </button>
    </div>
  </form>
</div>

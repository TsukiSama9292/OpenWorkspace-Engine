<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { loadTemplate, updateTemplate, formStateFromTemplate } from './template-edit';
  import TemplateBasics from '$lib/components/forms/TemplateBasics.svelte';
  import TemplateResources from '$lib/components/forms/TemplateResources.svelte';
  import TemplateAdvanced from '$lib/components/forms/TemplateAdvanced.svelte';

  let form = $state<ReturnType<typeof formStateFromTemplate> | null>(null);
  let loading = $state(true);
  let error = $state('');

  onMount(async () => {
    const id = $page.params.id;
    if (!id) { error = 'No template ID'; loading = false; return; }
    const result = await loadTemplate(id);
    if (result.error) { error = result.error; loading = false; return; }
    form = result.state!;
    loading = false;
  });

  async function onSubmit() {
    if (!form) return;
    const id = $page.params.id;
    if (!id) return;
    form.loading = true;
    form.error = '';
    const result = await updateTemplate(id, form);
    if (result.error) form.error = result.error;
  }
</script>

<div class="max-w-xl mx-auto">
  <h1 class="text-2xl font-bold text-surface-800 mb-6">Edit Template</h1>

  {#if loading}
    <p class="text-surface-500">Loading template...</p>
  {:else if error}
    <p class="text-error-500">{error}</p>
  {:else if form}
    <form class="flex flex-col gap-4" onsubmit={onSubmit}>
      <TemplateBasics bind:name={form.name} bind:description={form.description} bind:image={form.image} bind:remoteType={form.remoteType} />
      <TemplateResources bind:cores={form.cores} bind:ramGb={form.ramGb} bind:gpuCount={form.gpuCount} bind:dockerRegistry={form.dockerRegistry} bind:persistentStoragePath={form.persistentStoragePath} />

      <button type="button" class="text-sm text-surface-500 hover:text-surface-700 bg-transparent border-none cursor-pointer text-left p-0" onclick={() => form.showAdvanced = !form.showAdvanced}>
        {form.showAdvanced ? '▾ Hide Advanced' : '▸ Show Advanced'}
      </button>

      {#if form.showAdvanced}
        <TemplateAdvanced bind:hostname={form.hostname} bind:dns={form.dns} bind:shmSize={form.shmSize} bind:networkMode={form.networkMode} bind:containerRuntime={form.containerRuntime} bind:envVars={form.envVars} bind:execCommand={form.execCommand} bind:volumeMappings={form.volumeMappings} />
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
          {form.loading ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </form>
  {/if}
</div>

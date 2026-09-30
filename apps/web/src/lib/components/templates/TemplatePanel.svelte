<script lang="ts">
  import { api } from '$lib/api/client';
  import { auth } from '$lib/stores/auth';
  import { formatMemory } from '$lib/utils/format';
  import { familyArtwork } from '$lib/dashboard/artwork';
  import { loadTemplate, submitTemplate, updateTemplate, createInitialFormState, reconcileDefaultImage, type TemplateFormState } from '$lib/templates/template-form';
  import { isTemplatesEditor, serializeDashboardHash, createDirtySnapshot, isFormDirty, confirmDiscardChanges, type DashboardView } from '$lib/templates/dashboard-view';
  import { mayCreateTemplate, mayEditTemplate, mayLaunchTemplate, templateLockReason } from '$lib/permissions';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import type { Template, EffectiveContext } from '$lib/types';
  import TemplateBasics from '$lib/components/forms/TemplateBasics.svelte';
  import TemplateResources from '$lib/components/forms/TemplateResources.svelte';
  import TemplateAdvanced from '$lib/components/forms/TemplateAdvanced.svelte';

  let {
    view,
    configs = $bindable(),
    dirty = $bindable(),
    onnavigate = () => {},
    ondelete = () => {},
    ctx = null
  }: {
    view: DashboardView;
    configs: Template[];
    dirty: boolean;
    onnavigate: (hash: string) => void;
    ondelete: (config: Template) => void;
    ctx?: EffectiveContext | null;
  } = $props();

  let form = $state<TemplateFormState | null>(null);
  let initialSnapshot = $state<Record<string, unknown> | null>(null);
  let loading = $state(false);
  let loadError = $state('');

  let savedScroll = 0;
  let wasEditor = false;

  const computedDirty = $derived(form !== null && initialSnapshot !== null && isFormDirty(form, initialSnapshot));

  $effect(() => {
    dirty = computedDirty;
  });

  $effect(() => {
    if (!isTemplatesEditor(view) && wasEditor) {
      const main = document.querySelector<HTMLElement>('.main-content');
      if (main) main.scrollTop = savedScroll;
    }
    wasEditor = isTemplatesEditor(view);
  });

  $effect(() => {
    const v = view;
    if (!isTemplatesEditor(v)) {
      form = null;
      initialSnapshot = null;
      loadError = '';
      return;
    }
    if (v.editor === 'new') {
      const fresh = createInitialFormState();
      form = fresh;
      initialSnapshot = createDirtySnapshot(fresh);
      loadError = '';
      return;
    }
    form = null;
    loading = true;
    loadError = '';
    let cancelled = false;
    const id = v.templateId;
    loadTemplate(id).then((result) => {
      if (cancelled) return;
      if (result.error) {
        loadError = result.error;
      } else if (result.state) {
        form = result.state;
        initialSnapshot = createDirtySnapshot(result.state);
      }
      loading = false;
    });
    return () => { cancelled = true; };
  });

  $effect(() => {
    if (!form) return;
    if (!isTemplatesEditor(view)) return;
    const next = reconcileDefaultImage(form.image, form.remoteType, form.dockerInInstance);
    if (form.image !== next) form.image = next;
  });

  async function refresh() {
    const res = await api.get<{ templates: Template[] }>('/templates');
    if (res.data?.templates) configs = res.data.templates;
  }

  async function onSubmit() {
    if (!form) return;
    const v = view;
    if (!isTemplatesEditor(v)) return;
    form.loading = true;
    form.error = '';
    const result = v.editor === 'edit'
      ? await updateTemplate(v.templateId, form)
      : await submitTemplate(form);
    form.loading = false;
    if (result.error) {
      form.error = result.error;
      return;
    }
    await refresh();
    // Creating a template whitelists the Admin group on it, so the effective
    // context (allowed_template_ids) must be re-fetched or the new template
    // stays locked on the Instances page.
    await auth.check();
    onnavigate('#templates');
  }

  function captureScroll() {
    const main = document.querySelector<HTMLElement>('.main-content');
    if (main) savedScroll = main.scrollTop;
  }

  function openNew() {
    captureScroll();
    onnavigate('#templates/new');
  }

  function openEdit(templateId: string) {
    captureScroll();
    onnavigate(serializeDashboardHash({ tab: 'templates', editor: 'edit', templateId }));
  }

  function onCancel() {
    if (dirty && !confirmDiscardChanges()) return;
    onnavigate('#templates');
  }

  function backToTemplates() {
    onnavigate('#templates');
  }
</script>

{#if isTemplatesEditor(view)}
  <div class="max-w-xl mx-auto">
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-surface-100 m-0">{view.editor === 'edit' ? 'Edit Template' : 'New Template'}</h1>
      <button type="button" class="px-4 py-2 text-surface-400 bg-transparent border-none cursor-pointer hover:text-surface-100" onclick={onCancel}>Back</button>
    </div>

    {#if loading}
      <p class="text-surface-400">Loading template...</p>
    {:else if loadError}
      <p class="text-error-500">Could not load template: {loadError}</p>
      <button class="px-4 py-2 mt-4 bg-primary-500 text-white border-none rounded cursor-pointer hover:bg-primary-600 transition-colors" onclick={backToTemplates}>Back to Templates</button>
    {:else if form}
      <form class="flex flex-col gap-4" onsubmit={onSubmit}>
        <section aria-label="Step 1 of 3: basics">
          <p class="editor-step">Step 1 of 3 — Basics</p>
          <TemplateBasics bind:name={form.name} bind:description={form.description} bind:image={form.image} bind:remoteType={form.remoteType} bind:visibility={form.visibility} />
        </section>
        <section aria-label="Step 2 of 3: resources">
          <p class="editor-step">Step 2 of 3 — Resources</p>
          <TemplateResources bind:cores={form.cores} bind:ramGb={form.ramGb} bind:gpuCount={form.gpuCount} bind:dockerRegistry={form.dockerRegistry} bind:persistentStoragePath={form.persistentStoragePath} bind:maxRunSeconds={form.maxRunSeconds} bind:timeoutAction={form.timeoutAction} bind:keepTimeSeconds={form.keepTimeSeconds} bind:keepTimeAction={form.keepTimeAction} />
        </section>

        <button type="button" class="text-sm text-surface-400 hover:text-surface-100 bg-transparent border-none cursor-pointer text-left p-0" onclick={() => { if (form) form.showAdvanced = !form.showAdvanced; }}>
          {form.showAdvanced ? '▾ Step 3 of 3 — Advanced' : '▸ Step 3 of 3 — Advanced (optional)'}
        </button>

        {#if form.showAdvanced}
          <TemplateAdvanced bind:hostname={form.hostname} bind:dns={form.dns} bind:shmSize={form.shmSize} bind:networkMode={form.networkMode} bind:containerRuntime={form.containerRuntime} bind:dockerInInstance={form.dockerInInstance} bind:bandwidthUpMbps={form.bandwidthUpMbps} bind:bandwidthDownMbps={form.bandwidthDownMbps} bind:envVars={form.envVars} bind:execCommand={form.execCommand} bind:volumeMappings={form.volumeMappings} />
        {/if}

        {#if form.error}
          <p class="text-error-500 text-sm m-0">{form.error}</p>
        {/if}

        <div class="flex justify-end gap-3 mt-2">
          <button type="button" class="px-4 py-2 text-surface-400 bg-transparent border-none cursor-pointer hover:text-surface-100" onclick={onCancel}>Cancel</button>
          <button
            type="submit"
            disabled={form.loading}
            class="px-4 py-2 bg-primary-500 text-white border-none rounded cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed hover:bg-primary-600 transition-colors"
          >
            {form.loading
              ? (view.editor === 'edit' ? 'Saving...' : 'Creating...')
              : (view.editor === 'edit' ? 'Save Changes' : 'Create Template')}
          </button>
        </div>
      </form>
    {/if}
  </div>
{:else}
  <div class="templates-header">
    {#if mayCreateTemplate(ctx)}
      <button class="btn-create" onclick={openNew}>+ New Template</button>
    {/if}
  </div>
  {#if configs?.length === 0}
    <EmptyState message="No templates yet. Create one to get started." />
  {:else}
    <div class="catalog-grid">
      {#each configs ?? [] as config (config.id)}
        {@const launchable = mayLaunchTemplate(ctx, config)}
        {@const art = familyArtwork(config.name)}
        <div class="catalog-card" class:locked={!launchable}>
          <div class={art.coverClass}>
            <img class="catalog-mark" src={art.iconSrc} alt={`${art.family} mark`} loading="lazy" />
          </div>
          <div class="catalog-body">
            <h3 class="catalog-name">{config.name}</h3>
            <p class="catalog-desc">{config.description || 'No description yet.'}</p>
            <p class="catalog-facts">{config.cores} CPU · {formatMemory(config.memory)} · {config.gpu_count} GPU · {config.persistent_storage_path ? 'persistent storage' : 'ephemeral storage'}</p>
            {#if launchable}
              <span class="launchable-badge">May launch</span>
            {:else}
              <p class="catalog-locked-reason">{templateLockReason(ctx, config)}</p>
            {/if}
            {#if mayEditTemplate(ctx, config)}
              <div class="flex gap-2 mt-2">
                <button class="launch-btn edit" onclick={() => openEdit(config.id)}>Edit</button>
                <button class="launch-btn remove" onclick={() => ondelete(config)}>Delete</button>
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .templates-header {
    display: flex;
    justify-content: flex-end;
    margin-bottom: 1.5rem;
  }

  .editor-step {
    font-size: 0.7rem;
    font-weight: 700;
    color: #818cf8;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin: 0 0 0.5rem 0;
  }

  .launchable-badge {
    display: inline-flex;
    align-items: center;
    font-size: 0.65rem;
    font-weight: 600;
    padding: 0.2rem 0.55rem;
    border-radius: 999px;
    color: #4ade80;
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid rgba(34, 197, 94, 0.2);
  }
</style>

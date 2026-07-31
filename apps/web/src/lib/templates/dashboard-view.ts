import type { TemplateFormState } from './template-form';

export type DashboardTab = 'instances' | 'templates' | 'sessions' | 'users';

export type DashboardView =
  | { tab: 'instances' | 'sessions' | 'users' }
  | { tab: 'templates'; editor: 'list' }
  | { tab: 'templates'; editor: 'new' }
  | { tab: 'templates'; editor: 'edit'; templateId: string };

export function parseDashboardHash(hash: string): DashboardView {
  const clean = hash.replace(/^#\/?/, '').replace(/\/+$/, '');
  if (!clean) return { tab: 'instances' };
  const parts = clean.split('/').filter(Boolean);
  switch (parts[0]) {
    case 'templates':
      if (parts[1] === 'new') return { tab: 'templates', editor: 'new' };
      if (parts[1] === 'edit' && parts[2]) {
        let templateId = parts[2];
        try {
          templateId = decodeURIComponent(parts[2]);
        } catch {
          templateId = parts[2];
        }
        return { tab: 'templates', editor: 'edit', templateId };
      }
      return { tab: 'templates', editor: 'list' };
    case 'sessions':
      return { tab: 'sessions' };
    case 'users':
      return { tab: 'users' };
    default:
      return { tab: 'instances' };
  }
}

export function serializeDashboardHash(view: DashboardView): string {
  switch (view.tab) {
    case 'templates':
      if (view.editor === 'new') return '#templates/new';
      if (view.editor === 'edit') return `#templates/edit/${encodeURIComponent(view.templateId)}`;
      return '#templates';
    default:
      return `#${view.tab}`;
  }
}

export function isTemplatesEditor(view: DashboardView): view is Extract<DashboardView, { tab: 'templates'; editor: 'new' | 'edit' }> {
  return view.tab === 'templates' && view.editor !== 'list';
}

export function confirmDiscardChanges(): boolean {
  return confirm('Discard unsaved changes?');
}

export function createDirtySnapshot(state: TemplateFormState): Record<string, unknown> {
  return {
    name: state.name,
    description: state.description,
    image: state.image,
    cores: Number(state.cores) || 0,
    ramGb: Number(state.ramGb) || 0,
    gpuCount: Number(state.gpuCount) || 0,
    dockerRegistry: state.dockerRegistry,
    persistentStoragePath: state.persistentStoragePath,
    remoteType: state.remoteType,
    hostname: state.hostname,
    dns: state.dns,
    shmSize: state.shmSize ? Number(state.shmSize) : '',
    networkMode: state.networkMode,
    containerRuntime: state.containerRuntime,
    maxRunSeconds: state.maxRunSeconds,
    timeoutAction: state.timeoutAction,
    envVars: state.envVars.map(e => ({ key: e.key, value: e.value })),
    execCommand: state.execCommand,
    volumeMappings: state.volumeMappings.map(v => ({ host: v.host, container: v.container })),
  };
}

export function isFormDirty(state: TemplateFormState, initialSnapshot: Record<string, unknown>): boolean {
  return JSON.stringify(createDirtySnapshot(state)) !== JSON.stringify(initialSnapshot);
}

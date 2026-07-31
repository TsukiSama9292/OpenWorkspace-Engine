import { describe, it, expect, vi, afterEach } from 'vitest';
import {
  parseDashboardHash,
  serializeDashboardHash,
  isTemplatesEditor,
  confirmDiscardChanges,
  createDirtySnapshot,
  isFormDirty,
  type DashboardView
} from '$lib/templates/dashboard-view';
import { createInitialFormState, type TemplateFormState } from '$lib/templates/template-form';

describe('dashboard view helpers', () => {
  describe('parseDashboardHash', () => {
    it.each([
      ['', { tab: 'instances' }],
      ['#', { tab: 'instances' }],
      ['#instances', { tab: 'instances' }],
      ['#sessions', { tab: 'sessions' }],
      ['#users', { tab: 'users' }]
    ])('parses %s', (hash, expected) => {
      expect(parseDashboardHash(hash)).toEqual(expected);
    });

    it('parses templates list', () => {
      expect(parseDashboardHash('#templates')).toEqual({ tab: 'templates', editor: 'list' });
    });

    it('parses the new-template editor', () => {
      expect(parseDashboardHash('#templates/new')).toEqual({ tab: 'templates', editor: 'new' });
    });

    it('parses the edit-template editor', () => {
      expect(parseDashboardHash('#templates/edit/abc-123')).toEqual({
        tab: 'templates',
        editor: 'edit',
        templateId: 'abc-123'
      });
    });

    it('falls back to instances for unknown hashes', () => {
      expect(parseDashboardHash('#bogus')).toEqual({ tab: 'instances' });
      expect(parseDashboardHash('#templates/edit/')).toEqual({ tab: 'templates', editor: 'list' });
    });

    it('tolerates leading and trailing slashes', () => {
      expect(parseDashboardHash('#/templates/new/')).toEqual({ tab: 'templates', editor: 'new' });
    });

    it('keeps a malformed encoded id instead of throwing', () => {
      expect(parseDashboardHash('#templates/edit/%ZZ')).toEqual({
        tab: 'templates',
        editor: 'edit',
        templateId: '%ZZ'
      });
    });
  });

  describe('serializeDashboardHash', () => {
    it.each<[DashboardView, string]>([
      [{ tab: 'instances' }, '#instances'],
      [{ tab: 'sessions' }, '#sessions'],
      [{ tab: 'users' }, '#users'],
      [{ tab: 'templates', editor: 'list' }, '#templates'],
      [{ tab: 'templates', editor: 'new' }, '#templates/new'],
      [{ tab: 'templates', editor: 'edit', templateId: 'abc' }, '#templates/edit/abc']
    ])('serializes %o to %s', (view, hash) => {
      expect(serializeDashboardHash(view)).toBe(hash);
    });

    it('round-trips every view', () => {
      const views: DashboardView[] = [
        { tab: 'instances' },
        { tab: 'sessions' },
        { tab: 'users' },
        { tab: 'templates', editor: 'list' },
        { tab: 'templates', editor: 'new' },
        { tab: 'templates', editor: 'edit', templateId: 'id-with spaces' }
      ];
      for (const view of views) {
        expect(parseDashboardHash(serializeDashboardHash(view))).toEqual(view);
      }
    });
  });

  describe('isTemplatesEditor', () => {
    it('detects when the templates editor is open', () => {
      expect(isTemplatesEditor({ tab: 'templates', editor: 'list' })).toBe(false);
      expect(isTemplatesEditor({ tab: 'templates', editor: 'new' })).toBe(true);
      expect(isTemplatesEditor({ tab: 'templates', editor: 'edit', templateId: 'a' })).toBe(true);
      expect(isTemplatesEditor({ tab: 'instances' })).toBe(false);
    });
  });

  describe('confirmDiscardChanges', () => {
    afterEach(() => {
      vi.restoreAllMocks();
    });

    it('asks the browser whether to discard changes', () => {
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      expect(confirmDiscardChanges()).toBe(true);
      expect(window.confirm).toHaveBeenCalledWith('Discard unsaved changes?');
    });
  });

  describe('dirty detection', () => {
    const base = createInitialFormState();
    const baseSnapshot = createDirtySnapshot(base);

    function snapshotIsDirty(state: TemplateFormState, snapshot: ReturnType<typeof createDirtySnapshot>) {
      return isFormDirty(state, snapshot);
    }

    it('ignores UI-only state', () => {
      const uiOnly = { ...base, showAdvanced: true, loading: true, error: 'boom' };
      expect(snapshotIsDirty(uiOnly, baseSnapshot)).toBe(false);
    });

    it('detects a changed name', () => {
      expect(snapshotIsDirty({ ...base, name: 'Other' }, baseSnapshot)).toBe(true);
    });

    it('detects a changed env var', () => {
      const state = { ...base, envVars: [{ key: 'FOO', value: 'bar' }] };
      expect(snapshotIsDirty(state, baseSnapshot)).toBe(true);
    });

    it('detects a changed volume mapping', () => {
      const state = { ...base, volumeMappings: [{ host: '/a', container: '/b' }] };
      expect(snapshotIsDirty(state, baseSnapshot)).toBe(true);
    });

    it('treats string and number representations of the same value as equal', () => {
      const stringState = {
        ...base,
        cores: '2' as unknown as number,
        shmSize: '268435456'
      };
      const numberState = { ...base, shmSize: '268435456' };
      expect(isFormDirty(stringState, createDirtySnapshot(numberState))).toBe(false);
    });

    it('produces stable snapshots across equal states', () => {
      expect(createDirtySnapshot(base)).toEqual(baseSnapshot);
    });
  });
});

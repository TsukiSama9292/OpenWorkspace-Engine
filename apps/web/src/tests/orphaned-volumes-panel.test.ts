import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import OrphanedVolumesPanel from '$lib/components/volumes/OrphanedVolumesPanel.svelte';
import type { EffectiveContext, PersistentVolume } from '$lib/types';

vi.mock('$lib/api/rbac-actions', () => ({
  listOrphanedVolumes: vi.fn(),
  cleanupOrphanedVolume: vi.fn()
}));

import { listOrphanedVolumes, cleanupOrphanedVolume } from '$lib/api/rbac-actions';
const mockList = vi.mocked(listOrphanedVolumes);
const mockCleanup = vi.mocked(cleanupOrphanedVolume);

function context(overrides: Partial<EffectiveContext> = {}): EffectiveContext {
  return {
    user_id: 'me',
    username: 'me',
    is_admin: false, tier: 0,
    can_create_template: false,
    can_manage_users: false,
    can_manage_group_instances: false,
    can_manage_docker: false,
    can_manage_registry: false,
    effective_max_instances: 4,
    allowed_template_ids: [],
    group_ids: [],
    direct_max_instances: null,
    ...overrides
  };
}

const volume: PersistentVolume = {
  id: 'v1',
  host_path: '/data/openworkspace/tpl-1/alice',
  owner_id: 'u1',
  owner_username: 'alice',
  status: 'orphaned',
  created_at: '2026-08-01T00:00:00Z'
};

const deletedOwnerVolume: PersistentVolume = {
  id: 'v2',
  host_path: '/data/openworkspace/tpl-2/bob',
  owner_id: null,
  owner_username: null,
  status: 'orphaned',
  created_at: '2026-07-15T00:00:00Z'
};

describe('OrphanedVolumesPanel', () => {
  beforeEach(() => {
    mockList.mockResolvedValue({ volumes: [] });
    mockCleanup.mockResolvedValue({});
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('hides the view entirely from a non-privileged context', async () => {
    render(OrphanedVolumesPanel, { props: { ctx: context() } });

    await waitFor(() => {
      expect(screen.queryByText('Orphaned Volumes')).toBeNull();
      expect(screen.queryByText('Clean Up')).toBeNull();
    });
    expect(mockList).not.toHaveBeenCalled();
  });

  it('hides the view from a group-instance manager who lacks the volumes permission', async () => {
    render(OrphanedVolumesPanel, {
      props: { ctx: context({ can_manage_group_instances: true }) }
    });

    await waitFor(() => {
      expect(screen.queryByText('Orphaned Volumes')).toBeNull();
    });
    expect(mockList).not.toHaveBeenCalled();
  });

  it('renders the view for a system admin', async () => {
    mockList.mockResolvedValue({ volumes: [volume] });

    render(OrphanedVolumesPanel, { props: { ctx: context({ is_admin: true, tier: 2 }) } });

    await waitFor(() => {
      expect(screen.getByText('Orphaned Volumes')).toBeTruthy();
      expect(screen.getByText('Clean Up')).toBeTruthy();
    });
    expect(mockList).toHaveBeenCalledTimes(1);
  });

  it('renders the view for a can_manage_users holder', async () => {
    mockList.mockResolvedValue({ volumes: [volume] });

    render(OrphanedVolumesPanel, { props: { ctx: context({ can_manage_users: true }) } });

    await waitFor(() => {
      expect(screen.getByText(volume.host_path)).toBeTruthy();
    });
  });

  it('lists host path, owner, and since-when from the volumes payload', async () => {
    mockList.mockResolvedValue({ volumes: [volume, deletedOwnerVolume] });

    render(OrphanedVolumesPanel, { props: { ctx: context({ is_admin: true, tier: 2 }) } });

    await waitFor(() => {
      expect(screen.getByText(volume.host_path)).toBeTruthy();
      expect(screen.getByText(deletedOwnerVolume.host_path)).toBeTruthy();
      expect(screen.getByText('alice')).toBeTruthy();
      expect(screen.getByText('deleted user')).toBeTruthy();
      expect(screen.getByText(new Date(volume.created_at).toLocaleDateString())).toBeTruthy();
      expect(screen.getByText(new Date(deletedOwnerVolume.created_at).toLocaleDateString())).toBeTruthy();
    });
  });

  it('shows an empty state when no volumes are orphaned', async () => {
    mockList.mockResolvedValue({ volumes: [] });

    render(OrphanedVolumesPanel, { props: { ctx: context({ is_admin: true, tier: 2 }) } });

    await waitFor(() => {
      expect(screen.getByText('No orphaned volumes.')).toBeTruthy();
    });
  });

  it('surfaces a load error', async () => {
    mockList.mockResolvedValue({ error: 'Failed to load volumes' });

    render(OrphanedVolumesPanel, { props: { ctx: context({ is_admin: true, tier: 2 }) } });

    await waitFor(() => {
      expect(screen.getByText('Failed to load volumes')).toBeTruthy();
    });
  });

  it('does not call cleanup until the two-step confirmation is completed', async () => {
    mockList.mockResolvedValue({ volumes: [volume] });

    render(OrphanedVolumesPanel, { props: { ctx: context({ is_admin: true, tier: 2 }) } });

    await waitFor(() => {
      expect(screen.getByText(volume.host_path)).toBeTruthy();
    });

    await fireEvent.click(screen.getByText('Clean Up'));
    await waitFor(() => {
      expect(screen.getByText('Thorough Cleanup')).toBeTruthy();
    });

    await fireEvent.click(screen.getByText('Permanently Delete'));
    expect(mockCleanup).not.toHaveBeenCalled();

    const input = screen.getByLabelText('Host Path') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'wrong-path' } });
    await fireEvent.click(screen.getByText('Permanently Delete'));
    expect(mockCleanup).not.toHaveBeenCalled();

    await fireEvent.input(input, { target: { value: volume.host_path } });
    await fireEvent.click(screen.getByText('Permanently Delete'));

    await waitFor(() => {
      expect(mockCleanup).toHaveBeenCalledWith('v1');
    });
  });

  it('refreshes the list after a successful cleanup', async () => {
    mockList
      .mockResolvedValueOnce({ volumes: [volume] })
      .mockResolvedValue({ volumes: [] });

    render(OrphanedVolumesPanel, { props: { ctx: context({ is_admin: true, tier: 2 }) } });

    await waitFor(() => {
      expect(screen.getByText(volume.host_path)).toBeTruthy();
    });

    await fireEvent.click(screen.getByText('Clean Up'));
    const input = screen.getByLabelText('Host Path') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: volume.host_path } });
    await fireEvent.click(screen.getByText('Permanently Delete'));

    await waitFor(() => {
      expect(mockCleanup).toHaveBeenCalledWith('v1');
    });
    await waitFor(() => {
      expect(screen.queryByText(volume.host_path)).toBeNull();
      expect(screen.getByText('No orphaned volumes.')).toBeTruthy();
    });
  });

  it('shows a cleanup error and keeps the volume listed', async () => {
    mockList.mockResolvedValue({ volumes: [volume] });
    mockCleanup.mockResolvedValue({ error: 'Cleanup denied' });

    render(OrphanedVolumesPanel, { props: { ctx: context({ is_admin: true, tier: 2 }) } });

    await waitFor(() => {
      expect(screen.getByText(volume.host_path)).toBeTruthy();
    });

    await fireEvent.click(screen.getByText('Clean Up'));
    const input = screen.getByLabelText('Host Path') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: volume.host_path } });
    await fireEvent.click(screen.getByText('Permanently Delete'));

    await waitFor(() => {
      expect(screen.getByText('Cleanup denied')).toBeTruthy();
    });
    expect(screen.getAllByText(volume.host_path).length).toBeGreaterThan(0);
  });

  it('searches volumes by host path', async () => {
    mockList.mockResolvedValue({ volumes: [volume, deletedOwnerVolume] });

    render(OrphanedVolumesPanel, { props: { ctx: context({ can_manage_users: true }) } });

    await waitFor(() => {
      expect(screen.getByText(volume.host_path)).toBeTruthy();
      expect(screen.getByText(deletedOwnerVolume.host_path)).toBeTruthy();
    });

    const input = screen.getByPlaceholderText('Search host path or owner...') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'alice' } });

    await waitFor(() => {
      expect(screen.getByText(volume.host_path)).toBeTruthy();
      expect(screen.queryByText(deletedOwnerVolume.host_path)).toBeNull();
    });
  });

  it('searches volumes by owner name', async () => {
    mockList.mockResolvedValue({ volumes: [volume, deletedOwnerVolume] });

    render(OrphanedVolumesPanel, { props: { ctx: context({ can_manage_users: true }) } });

    await waitFor(() => {
      expect(screen.getByText(volume.host_path)).toBeTruthy();
    });

    const input = screen.getByPlaceholderText('Search host path or owner...') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'bob' } });

    await waitFor(() => {
      expect(screen.queryByText(volume.host_path)).toBeNull();
      expect(screen.getByText(deletedOwnerVolume.host_path)).toBeTruthy();
    });
  });
});

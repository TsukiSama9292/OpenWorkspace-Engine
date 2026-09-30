/** Shared status-to-dot mapping for every state badge and dot. Unknown states render unstyled. */
export function statusBadgeClass(status: string | null | undefined): string {
  switch (status) {
    case 'running':
      return 'dot-active';
    case 'paused':
      return 'dot-paused';
    case 'stopped':
    case 'orphaned':
      return 'dot-stopped';
    case 'error':
      return 'dot-error';
    case 'starting':
      return 'dot-starting';
    default:
      return '';
  }
}

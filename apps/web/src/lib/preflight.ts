import { formatBytes } from '$lib/utils/format';
import type { PreflightRejection, PreflightRejectionScope } from '$lib/types';

const SCOPE_COPY: Record<PreflightRejectionScope, string> = {
  template_not_allowed: 'Template not allowed',
  template_hidden: 'Template is hidden',
  user_instance: 'Your instance limit reached',
  host_instance: 'Host instance limit reached',
  host_resource_cpu: 'Host CPU cap reached',
  host_resource_memory: 'Host memory cap reached',
  host_resource_gpu: 'Host GPU cap reached',
  group_pool_cpu: 'Group CPU pool cap reached',
  group_pool_memory: 'Group memory pool cap reached',
  group_pool_gpu: 'Group GPU pool cap reached',
  member_quota_cpu: 'Personal CPU quota reached',
  member_quota_memory: 'Personal memory quota reached',
  member_quota_gpu: 'Personal GPU quota reached',
};

const MEMORY_SCOPES: ReadonlySet<PreflightRejectionScope> = new Set([
  'host_resource_memory',
  'group_pool_memory',
  'member_quota_memory',
]);

const NO_NUMBERS_SCOPES: ReadonlySet<PreflightRejectionScope> = new Set([
  'template_not_allowed',
  'template_hidden',
]);

function formatValue(scope: PreflightRejectionScope, value: number): string {
  if (value < 0) return 'unlimited';
  if (value === 0) return '0';
  return MEMORY_SCOPES.has(scope) ? formatBytes(value * 1024 * 1024) : String(value);
}

export function preflightTitle(rejection: PreflightRejection): string {
  return SCOPE_COPY[rejection.scope] ?? 'Launch rejected';
}

export function preflightNumbers(rejection: PreflightRejection): string | null {
  if (NO_NUMBERS_SCOPES.has(rejection.scope)) return null;
  const current = formatValue(rejection.scope, rejection.current);
  const limit = formatValue(rejection.scope, rejection.limit);
  return `Current ${current} / limit ${limit} (requested ${rejection.requested})`;
}

export function preflightMessage(rejection: PreflightRejection, error: string): string {
  const numbers = preflightNumbers(rejection);
  const reason = numbers ? `${preflightTitle(rejection)}: ${numbers}` : preflightTitle(rejection);
  return error ? `${reason} — ${error}` : reason;
}

import type { PreflightRejection, PreflightRejectionScope } from '$lib/types';

const SCOPE_COPY: Record<PreflightRejectionScope, string> = {
  template_not_allowed: 'Template not allowed',
  user_instance: 'Your instance limit reached',
  host_instance: 'Host instance limit reached',
};

export function preflightTitle(rejection: PreflightRejection): string {
  return SCOPE_COPY[rejection.scope] ?? 'Launch rejected';
}

export function preflightNumbers(rejection: PreflightRejection): string | null {
  if (rejection.scope === 'template_not_allowed') return null;
  return `Current ${rejection.current} / limit ${rejection.limit} (requested ${rejection.requested})`;
}

export function preflightMessage(rejection: PreflightRejection, error: string): string {
  const numbers = preflightNumbers(rejection);
  const reason = numbers ? `${preflightTitle(rejection)}: ${numbers}` : preflightTitle(rejection);
  return error ? `${reason} — ${error}` : reason;
}

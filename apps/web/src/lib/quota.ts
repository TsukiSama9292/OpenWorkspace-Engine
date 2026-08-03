import type { QuotaPayload, QuotaScope } from '$lib/types';
import { formatMemory } from '$lib/utils/format';

export type QuotaUnit = 'count' | 'cores' | 'memory';

export interface QuotaScopeInfo {
  label: string;
  guidance: string;
  unit: QuotaUnit;
}

const SCOPE_INFO: Record<QuotaScope, QuotaScopeInfo> = {
  user_instance: {
    label: '實例數量已達上限',
    guidance: '請先停止或刪除一個實例。',
    unit: 'count',
  },
  user_cpu: {
    label: '個人 CPU 配額已達上限',
    guidance: '請先停止一個實例以釋放 CPU，或請管理員調高配額。',
    unit: 'cores',
  },
  user_ram: {
    label: '個人記憶體配額已達上限',
    guidance: '請先停止一個實例以釋放記憶體，或請管理員調高配額。',
    unit: 'memory',
  },
  host_instance: {
    label: '伺服器實例總數已達上限',
    guidance: '請先停止或刪除一個實例，或請管理員調高總實例上限。',
    unit: 'count',
  },
  host_dedicated_cpu: {
    label: '主機專用 CPU 池已達上限',
    guidance: '請停止或刪除一個專用（dedicated）實例以釋放 CPU，或請管理員調高主機容量。',
    unit: 'cores',
  },
  host_dedicated_ram: {
    label: '主機專用記憶體池已達上限',
    guidance: '請停止或刪除一個專用（dedicated）實例以釋放記憶體，或請管理員調高主機容量。',
    unit: 'memory',
  },
  host_shared_cpu: {
    label: '主機共享 CPU 總額已達上限',
    guidance: '請停止或刪除一個共享（shared）實例，或請管理員調高共享上限。',
    unit: 'cores',
  },
  host_shared_ram: {
    label: '主機共享記憶體總額已達上限',
    guidance: '請停止或刪除一個共享（shared）實例，或請管理員調高共享上限。',
    unit: 'memory',
  },
};

export function quotaScopeInfo(scope: string): QuotaScopeInfo | undefined {
  return SCOPE_INFO[scope as QuotaScope];
}

export function isQuotaPayload(value: unknown): value is QuotaPayload {
  if (!value || typeof value !== 'object') return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.scope === 'string' &&
    !!SCOPE_INFO[v.scope as QuotaScope] &&
    typeof v.current === 'number' &&
    typeof v.limit === 'number' &&
    typeof v.requested === 'number'
  );
}

function formatValue(unit: QuotaUnit, value: number): string {
  return unit === 'memory' ? formatMemory(value) : String(value);
}

export function formatQuotaNumbers(quota: QuotaPayload): string {
  const info = quotaScopeInfo(quota.scope);
  if (!info) return '';
  return (
    `目前 ${formatValue(info.unit, quota.current)} / ` +
    `上限 ${formatValue(info.unit, quota.limit)}` +
    `（本次請求 ${formatValue(info.unit, quota.requested)}）`
  );
}

export function quotaMessage(quota: QuotaPayload): string {
  const info = quotaScopeInfo(quota.scope);
  if (!info) return '';
  return `${info.label}：${formatQuotaNumbers(quota)}${info.guidance}`;
}

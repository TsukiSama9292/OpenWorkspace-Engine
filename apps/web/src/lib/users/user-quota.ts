export interface UserQuotaForm {
  instance_limit: string;
  max_cpu_cores: string;
  max_ram_bytes: string;
}

export interface UserQuotaOverrides {
  instance_limit: number | null;
  max_cpu_cores: number | null;
  max_ram_bytes: number | null;
}

export interface UserQuotaRow {
  instance_limit: number | null;
  max_cpu_cores: number | null;
  max_ram_bytes: number | null;
  effective_instance_limit: number;
  effective_max_cpu_cores: number;
  effective_max_ram_bytes: number;
  quota_exempt: boolean;
}

export interface UserRow extends UserQuotaRow {
  id: string;
  username: string;
  role: string;
  created_at: string;
}

export function emptyQuotaForm(): UserQuotaForm {
  return { instance_limit: '', max_cpu_cores: '', max_ram_bytes: '' };
}

export function quotaFormFromUser(user: UserQuotaRow): UserQuotaForm {
  return {
    instance_limit: user.instance_limit?.toString() ?? '',
    max_cpu_cores: user.max_cpu_cores?.toString() ?? '',
    max_ram_bytes: user.max_ram_bytes?.toString() ?? '',
  };
}

// An empty field sends NULL, which restores the role default (inherit).
export function buildQuotaOverrides(form: UserQuotaForm): UserQuotaOverrides {
  return {
    instance_limit: form.instance_limit === '' ? null : Number(form.instance_limit),
    max_cpu_cores: form.max_cpu_cores === '' ? null : Number(form.max_cpu_cores),
    max_ram_bytes: form.max_ram_bytes === '' ? null : Number(form.max_ram_bytes),
  };
}

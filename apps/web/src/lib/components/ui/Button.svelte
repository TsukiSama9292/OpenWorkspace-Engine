<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    variant?: 'primary' | 'secondary' | 'ghost' | 'error' | 'warning' | 'success';
    size?: 'sm' | 'md' | 'lg';
    disabled?: boolean;
    type?: 'button' | 'submit' | 'reset';
    onclick?: () => void;
    children: Snippet;
  }

  let {
    variant = 'primary',
    size = 'md',
    disabled = false,
    type = 'button',
    onclick,
    children
  }: Props = $props();

  const sizeClasses = {
    sm: 'px-3 py-1.5 text-xs',
    md: 'px-4 py-2 text-sm',
    lg: 'px-5 py-2.5 text-base'
  };

  const variantClasses = {
    primary: 'bg-primary-500 text-primary-contrast-500 hover:bg-primary-600',
    secondary: 'bg-surface-200 text-surface-800 border border-surface-300 hover:bg-surface-300',
    ghost: 'bg-transparent text-surface-600 hover:bg-surface-100',
    error: 'bg-error-500 text-error-contrast-500 hover:bg-error-600',
    warning: 'bg-warning-500 text-warning-contrast-500 hover:bg-warning-600',
    success: 'bg-success-500 text-success-contrast-500 hover:bg-success-600'
  };
</script>

<button
  {type}
  {disabled}
  class="inline-flex items-center justify-center gap-2 rounded-lg font-medium cursor-pointer border border-transparent transition-all duration-150 whitespace-nowrap disabled:opacity-50 disabled:cursor-not-allowed font-sans {sizeClasses[size]} {variantClasses[variant]}"
  onclick={onclick}
>
  {@render children()}
</button>

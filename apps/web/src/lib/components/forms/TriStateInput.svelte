<script lang="ts">
  import type { TriState, TriStateMode } from '$lib/tri-state';

  let {
    label,
    value = $bindable(),
    unit = '',
    placeholder = ''
  }: {
    label: string;
    value: TriState;
    unit?: string;
    placeholder?: string;
  } = $props();

  let draft = $state(String(value.value > 0 ? value.value : ''));

  function onModeChange(event: Event) {
    const mode = (event.currentTarget as HTMLSelectElement).value as TriStateMode;
    if (mode === 'unlimited') value = { mode, value: 1 };
    else if (mode === 'disabled') value = { mode, value: 0 };
    else value = { mode, value: value.value > 0 ? value.value : 1 };
  }

  function onCustomInput(event: Event) {
    draft = (event.currentTarget as HTMLInputElement).value;
    const n = Number.parseInt(draft, 10);
    if (Number.isFinite(n) && n > 0) value = { mode: 'custom', value: n };
  }
</script>

<label class="tri-field">
  <span class="tri-label">{label}</span>
  <div class="tri-row">
    <select
      aria-label="{label} mode"
      class="tri-select"
      value={value.mode}
      onchange={onModeChange}
    >
      <option value="unlimited">Unlimited</option>
      <option value="disabled">Disabled (0)</option>
      <option value="custom">Custom</option>
    </select>
    {#if value.mode === 'custom'}
      <div class="tri-custom">
        <input
          type="number"
          class="tri-input"
          min="1"
          value={draft}
          aria-label="{label} value"
          oninput={onCustomInput}
          placeholder={placeholder}
        />
        {#if unit}
          <span class="tri-unit">{unit}</span>
        {/if}
      </div>
    {/if}
  </div>
</label>

<style>
  .tri-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .tri-label {
    font-size: 0.7rem;
    font-weight: 600;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .tri-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .tri-select {
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 0.5rem 0.6rem;
    color: #f4f4f5;
    font-size: 0.8rem;
    font-family: inherit;
    outline: none;
    flex: 1;
    min-width: 0;
  }

  .tri-custom {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .tri-input {
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 0.5rem 0.6rem;
    color: #f4f4f5;
    font-size: 0.8rem;
    font-family: inherit;
    outline: none;
    width: 100%;
  }

  .tri-unit {
    font-size: 0.72rem;
    color: #71717a;
    white-space: nowrap;
  }
</style>

<script lang="ts">
  let {
    label,
    value = $bindable(),
    unit = '',
    min = 1,
    placeholder = ''
  }: {
    label: string;
    value: number;
    unit?: string;
    min?: number;
    placeholder?: string;
  } = $props();

  let unlimited = $state(value === -1);
  let draft = $state(String(value > 0 ? value : ''));

  function fallbackValue(): number {
    return Math.max(1, min);
  }

  function onUnlimitedChange(event: Event) {
    unlimited = (event.currentTarget as HTMLInputElement).checked;
    if (unlimited) {
      value = -1;
    } else {
      const n = Number.parseInt(draft, 10);
      value = Number.isFinite(n) && n > 0 ? n : fallbackValue();
      draft = String(value);
    }
  }

  function onCustomInput(event: Event) {
    draft = (event.currentTarget as HTMLInputElement).value;
    const n = Number.parseInt(draft, 10);
    if (Number.isFinite(n) && n >= min) value = n;
  }
</script>

<label class="unlimited-field">
  <span class="unlimited-label">{label}</span>
  <div class="unlimited-row">
    <label class="unlimited-toggle">
      <input
        type="checkbox"
        checked={unlimited}
        onchange={onUnlimitedChange}
        aria-label="{label} unlimited"
      />
      <span class="unlimited-toggle-text">Unlimited</span>
    </label>
    {#if !unlimited}
      <div class="unlimited-custom">
        <input
          type="number"
          class="unlimited-input"
          min={min}
          value={draft}
          aria-label={label}
          oninput={onCustomInput}
          placeholder={placeholder}
        />
        {#if unit}
          <span class="unlimited-unit">{unit}</span>
        {/if}
      </div>
    {/if}
  </div>
</label>

<style>
  .unlimited-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .unlimited-label {
    font-size: 0.7rem;
    font-weight: 600;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .unlimited-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .unlimited-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    white-space: nowrap;
  }

  .unlimited-toggle-text {
    font-size: 0.78rem;
    color: #d4d4d8;
  }

  .unlimited-custom {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .unlimited-input {
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

  .unlimited-input:focus {
    border-color: #818cf8;
  }

  .unlimited-unit {
    font-size: 0.72rem;
    color: #71717a;
    white-space: nowrap;
  }
</style>

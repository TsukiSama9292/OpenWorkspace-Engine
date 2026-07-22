<script>
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';

  let name = $state('');
  let loading = $state(false);
  let error = $state('');

  async function createInstance() {
    loading = true;
    error = '';
    const res = await api.post('/instances', { name });
    loading = false;
    if (res.error) {
      error = res.error;
    } else if (res.data?.instance.status === 'error') {
      error = 'Instance created but container failed to start';
    } else if (res.data) {
      goto(`/instances/${res.data.instance.id}/`);
    } else {
      error = 'Failed to create instance';
    }
  }
</script>

<div class="new-instance">
  <h1>New Instance</h1>
  <form onsubmit={createInstance}>
    <label>
      Name
      <input
        type="text"
        bind:value={name}
        placeholder="my-workspace"
        required
      />
    </label>
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <div class="actions">
      <a href="/">Cancel</a>
      <button type="submit" disabled={loading}>
        {loading ? 'Creating...' : 'Create'}
      </button>
    </div>
  </form>
</div>

<style>
  .new-instance {
    max-width: 480px;
    margin: 0 auto;
  }
  h1 {
    color: var(--text-primary, #fff);
    margin-bottom: 1.5rem;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    color: var(--text-secondary, #aaa);
  }
  input {
    padding: 0.75rem;
    border: 1px solid var(--border, #333);
    border-radius: 4px;
    background: var(--bg-primary, #0f0f1a);
    color: var(--text-primary, #fff);
    font-size: 1rem;
  }
  .actions {
    display: flex;
    gap: 1rem;
    justify-content: flex-end;
    margin-top: 1rem;
  }
  a {
    color: var(--text-secondary, #aaa);
    text-decoration: none;
    padding: 0.5rem 1rem;
  }
  button {
    padding: 0.5rem 1rem;
    border: none;
    border-radius: 4px;
    background: var(--accent, #6366f1);
    color: white;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .error {
    color: #ef4444;
    margin: 0;
  }
</style>

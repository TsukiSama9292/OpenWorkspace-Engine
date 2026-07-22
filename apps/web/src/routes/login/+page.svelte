<script>
  import { auth } from '$lib/stores/auth';
  import { goto } from '$app/navigation';

  let username = $state('');
  let password = $state('');
  let error = $state('');
  let loading = $state(false);

  async function handleLogin() {
    loading = true;
    error = '';
    const success = await auth.login(username, password);
    loading = false;
    if (success) {
      goto('/');
    } else {
      error = 'Invalid credentials';
    }
  }
</script>

<div class="login-container">
  <div class="login-box">
    <h1>OpenWorkspace</h1>
    <form onsubmit={handleLogin}>
      <input
        type="text"
        placeholder="Username"
        bind:value={username}
        required
      />
      <input
        type="password"
        placeholder="Password"
        bind:value={password}
        required
      />
      {#if error}
        <p class="error">{error}</p>
      {/if}
      <button type="submit" disabled={loading}>
        {loading ? 'Logging in...' : 'Login'}
      </button>
    </form>
  </div>
</div>

<style>
  .login-container {
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 100vh;
    background: var(--bg-primary, #0f0f1a);
  }
  .login-box {
    background: var(--bg-secondary, #1a1a2e);
    padding: 2rem;
    border-radius: 8px;
    width: 320px;
  }
  h1 {
    text-align: center;
    margin-bottom: 1.5rem;
    color: var(--text-primary, #fff);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  input {
    padding: 0.75rem;
    border: 1px solid var(--border, #333);
    border-radius: 4px;
    background: var(--bg-primary, #0f0f1a);
    color: var(--text-primary, #fff);
    font-size: 1rem;
  }
  button {
    padding: 0.75rem;
    border: none;
    border-radius: 4px;
    background: var(--accent, #6366f1);
    color: white;
    font-size: 1rem;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .error {
    color: #ef4444;
    text-align: center;
    margin: 0;
  }
</style>

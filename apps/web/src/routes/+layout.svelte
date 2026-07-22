<script>
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { auth, isAuthenticated } from '$lib/stores/auth';
  import { theme } from '$lib/stores/theme';

  let { children } = $props();
  let authChecked = $state(false);

  onMount(() => {
    theme.init();
    auth.check().then(() => { authChecked = true; });
  });

  let showNav = $derived($isAuthenticated && $page.url.pathname !== '/login/');

  $effect(() => {
    if (authChecked && !$isAuthenticated && $page.url.pathname !== '/login/') {
      goto('/login');
    }
  });
</script>

{#if showNav}
  <nav class="navbar">
    <a href="/" class="brand">OpenWorkspace</a>
    <div class="links">
      <a href="/">Dashboard</a>
      <button onclick={() => auth.logout()}>Logout</button>
    </div>
  </nav>
{/if}

<main class:with-nav={showNav}>
  {@render children()}
</main>

<style>
  .navbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem 1.5rem;
    background: var(--bg-secondary, #1a1a2e);
    border-bottom: 1px solid var(--border, #333);
  }
  .brand {
    font-weight: bold;
    font-size: 1.1rem;
    color: var(--text-primary, #fff);
    text-decoration: none;
  }
  .links {
    display: flex;
    gap: 1rem;
    align-items: center;
  }
  .links a, .links button {
    color: var(--text-secondary, #aaa);
    text-decoration: none;
    background: none;
    border: none;
    cursor: pointer;
    font: inherit;
  }
  .links a:hover, .links button:hover {
    color: var(--text-primary, #fff);
  }
  main.with-nav {
    padding: 1.5rem;
  }
</style>

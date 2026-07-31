<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { auth, isAuthenticated } from '$lib/stores/auth';

  let { children } = $props();
  let authChecked = $state(false);

  onMount(() => {
    auth.check().then(() => { authChecked = true; });
  });

  let showNav = $derived(
    $isAuthenticated
    && $page.url.pathname !== '/'
    && $page.url.pathname !== '/login/'
    && !$page.url.pathname.startsWith('/kasmvnc/')
  );

  $effect(() => {
    if (authChecked && !$isAuthenticated && $page.url.pathname !== '/login/') {
      goto('/login');
    }
  });
</script>

{#if showNav}
  <nav>
    <a href="/" class="nav-brand">OpenWorkspace</a>
    <div class="nav-links">
      <a href="/">Dashboard</a>
      <button onclick={() => auth.logout()}>Logout</button>
    </div>
  </nav>
{/if}

<main class={showNav ? 'has-nav' : ''}>
  {@render children()}
</main>

<style>
  :global(body) {
    margin: 0;
  }

  nav {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem 1.5rem;
    background: rgba(18, 18, 22, 0.65);
    backdrop-filter: blur(20px);
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }

  .nav-brand {
    font-weight: 700;
    font-size: 1.1rem;
    color: #f4f4f5;
    text-decoration: none;
  }

  .nav-links {
    display: flex;
    gap: 1.5rem;
    align-items: center;
  }

  .nav-links a,
  .nav-links button {
    color: #71717a;
    text-decoration: none;
    background: none;
    border: none;
    font: inherit;
    cursor: pointer;
    font-size: 0.85rem;
    transition: color 0.2s;
  }

  .nav-links a:hover,
  .nav-links button:hover {
    color: #f4f4f5;
  }

  main.has-nav {
    padding: 1.5rem;
  }
</style>

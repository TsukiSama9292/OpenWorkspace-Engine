<script lang="ts">
  import { auth } from '$lib/stores/auth';
  import { goto } from '$app/navigation';

  let username = $state('');
  let password = $state('');
  let error = $state('');
  let loading = $state(false);

  async function onSubmit() {
    loading = true;
    error = '';
    const success = await auth.login(username, password);
    if (success) goto('/');
    loading = false;
    if (!success) error = 'Invalid credentials';
  }
</script>

<div class="app-container">
  <div class="ambient-glow"></div>

  <div class="login-wrapper">
    <div class="panel-visual">
      <div class="brand-tag">
        <span class="status-pulse"></span>
        <span>OpenWorkspace</span>
      </div>

      <div class="visual-content">
        <h1 class="hero-title">Your Cloud Desktop,<br />Accessible Anywhere</h1>
        <p class="hero-desc">Log in from any browser to access your personal instance. No installation needed — your files and tools are ready whenever you are.</p>

      </div>

      <div class="panel-footer">
        <span>BROWSER-BASED</span>
        <span>•</span>
        <span>ZERO INSTALL REQUIRED</span>
      </div>
    </div>

    <div class="panel-form">
      <div class="form-header">
        <h2>Welcome Back</h2>
        <p>Sign in to access your instance</p>
      </div>

      <form class="form" onsubmit={onSubmit}>
        <div class="input-group">
          <label for="acc">Account</label>
          <input
            id="acc"
            type="text"
            placeholder="name@domain.com"
            value={username}
            oninput={(e) => { username = (e.target as HTMLInputElement).value; }}
            required
          />
        </div>

        <div class="input-group">
          <label for="pwd">Password</label>
          <input
            id="pwd"
            type="password"
            placeholder="••••••••"
            value={password}
            oninput={(e) => { password = (e.target as HTMLInputElement).value; }}
            required
          />
        </div>

        {#if error}
          <div class="error-badge">{error}</div>
        {/if}

        <button class="btn-primary" type="submit" disabled={loading}>
          {#if loading}
            <span>Authenticating...</span>
          {:else}
            <span>Continue</span>
            <svg class="arrow-icon" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M6.75 3.75L11.25 8.25L6.75 12.75" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          {/if}
        </button>

        <div class="divider"><span>OR</span></div>

        <button class="btn-secondary" type="button">SSH Key / Single Sign-On</button>
      </form>
    </div>
  </div>
</div>

<style>
  :global(body) {
    margin: 0;
    background-color: #09090b;
    color: #f4f4f5;
    font-family: 'Plus Jakarta Sans', -apple-system, BlinkMacSystemFont, sans-serif;
  }

  .app-container {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    overflow: hidden;
    background: radial-gradient(circle at 50% 0%, #18181b 0%, #09090b 100%);
  }

  .ambient-glow {
    position: absolute;
    width: 600px;
    height: 600px;
    background: radial-gradient(circle, rgba(99, 102, 241, 0.2) 0%, rgba(0, 0, 0, 0) 70%);
    top: 20%;
    left: 20%;
    filter: blur(80px);
    pointer-events: none;
  }

  .login-wrapper {
    position: relative;
    display: grid;
    grid-template-columns: 1.2fr 1fr;
    width: 100%;
    max-width: 960px;
    height: 580px;
    background: linear-gradient(180deg, rgba(24, 24, 30, 0.85) 0%, rgba(14, 14, 18, 0.9) 100%);
    backdrop-filter: blur(24px);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-top: 1px solid rgba(255, 255, 255, 0.22);
    border-radius: 20px;
    box-shadow: 0 30px 60px -12px rgba(0, 0, 0, 0.9), 0 0 50px -10px rgba(99, 102, 241, 0.25);
    overflow: hidden;
  }

  .panel-visual {
    padding: 3rem;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    background: linear-gradient(135deg, rgba(255, 255, 255, 0.03) 0%, rgba(255, 255, 255, 0) 100%);
    border-right: 1px solid rgba(255, 255, 255, 0.05);
  }

  .brand-tag {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    color: #a1a1aa;
  }

  .oss-badge {
    font-size: 0.65rem;
    font-family: monospace;
    font-weight: 500;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(99, 102, 241, 0.15);
    border: 1px solid rgba(99, 102, 241, 0.3);
    color: #818cf8;
    letter-spacing: 0.05em;
  }

  .status-pulse {
    width: 6px;
    height: 6px;
    background: #6366f1;
    border-radius: 50%;
    box-shadow: 0 0 10px #6366f1;
  }

  .hero-title {
    font-size: 2.2rem;
    font-weight: 700;
    line-height: 1.2;
    letter-spacing: -0.02em;
    margin: 0 0 1rem 0;
    background: linear-gradient(to bottom right, #ffffff, #a1a1aa);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
  }

  .hero-desc {
    color: #a1a1aa;
    font-size: 0.85rem;
    line-height: 1.4;
    margin-bottom: 2.5rem;
  }



  .panel-footer {
    display: flex;
    gap: 8px;
    font-size: 0.72rem;
    color: #94a3b8;
    font-family: monospace;
  }

  .panel-form {
    padding: 3rem;
    display: flex;
    flex-direction: column;
    justify-content: center;
  }

  .form-header h2 {
    font-size: 1.5rem;
    font-weight: 600;
    margin: 0;
  }

  .form-header p {
    font-size: 0.85rem;
    color: #a1a1aa;
    margin: 0.4rem 0 2rem 0;
  }

  .form { display: flex; flex-direction: column; gap: 1.25rem; }

  .input-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .input-group label {
    font-size: 0.75rem;
    font-weight: 600;
    color: #e4e4e7;
    letter-spacing: 0.02em;
  }

  .input-group input {
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-top: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 8px;
    padding: 0.75rem 1rem;
    color: #ffffff;
    font-size: 0.9rem;
    outline: none;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .input-group input::placeholder {
    color: #71717a;
  }

  .input-group input:focus {
    border-color: #818cf8;
    background: rgba(0, 0, 0, 0.6);
    box-shadow: 0 0 0 3px rgba(129, 140, 248, 0.25);
  }

  .btn-primary {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    background: linear-gradient(135deg, #6366f1 0%, #4f46e5 100%);
    color: #ffffff;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-top: 1px solid rgba(255, 255, 255, 0.35);
    border-radius: 8px;
    padding: 0.8rem;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 4px 16px rgba(99, 102, 241, 0.35);
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .arrow-icon {
    width: 16px;
    height: 16px;
    transition: transform 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .btn-primary:hover:not(:disabled) {
    background: linear-gradient(135deg, #818cf8 0%, #6366f1 100%);
    box-shadow: 0 6px 24px rgba(99, 102, 241, 0.5);
    transform: translateY(-1px);
  }

  .btn-primary:hover:not(:disabled) .arrow-icon {
    transform: translateX(3px);
  }

  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

  .divider {
    display: flex;
    align-items: center;
    text-align: center;
    color: #3f3f46;
    font-size: 0.7rem;
    margin: 0.5rem 0;
  }

  .divider::before, .divider::after {
    content: '';
    flex: 1;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .divider span { padding: 0 8px; }

  .btn-secondary {
    background: transparent;
    color: #a1a1aa;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    padding: 0.6rem;
    font-size: 0.8rem;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-secondary:hover {
    border-color: rgba(255, 255, 255, 0.2);
    color: #fff;
  }

  .error-badge {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    color: #f87171;
    font-size: 0.8rem;
    padding: 0.5rem;
    border-radius: 6px;
    text-align: center;
  }
</style>

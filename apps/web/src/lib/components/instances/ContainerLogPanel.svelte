<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { streamInstanceLogs } from '$lib/api/instance-logs';
  import { ansiToHtml } from '$lib/logs/ansi';
  import type { ContainerLogLine, Instance } from '$lib/types';

  let {
    instance,
    onclose = () => {}
  }: {
    instance: Instance;
    onclose?: () => void;
  } = $props();

  function renderHtml(node: HTMLElement, html: string) {
    node.innerHTML = html;
    return {
      update(nextHtml: string) {
        node.innerHTML = nextHtml;
      }
    };
  }

  const MAX_LINES = 2000;

  let lines = $state<ContainerLogLine[]>([]);
  let follow = $state(true);
  let tail = $state(200);
  let ended = $state<string | null>(null);
  let error = $state('');
  let streaming = $state(true);

  let abortStream: (() => void) | null = null;

  const REASON_LABELS: Record<string, string> = {
    stopped: 'Session ended — the instance was stopped.',
    paused: 'Session ended — the instance was paused.',
    deleted: 'Session ended — the instance was deleted.',
    eof: 'Log stream ended.'
  };

  function append(line: ContainerLogLine) {
    lines = [...lines, line];
    if (lines.length > MAX_LINES) {
      lines = lines.slice(lines.length - MAX_LINES);
    }
  }

  function startStream() {
    if (abortStream) abortStream();
    abortStream = null;
    lines = [];
    ended = null;
    error = '';
    streaming = true;
    abortStream = streamInstanceLogs(
      instance.id,
      { tail, follow },
      {
        onLog: (line) => append(line),
        onEnd: (reason) => {
          streaming = false;
          ended = reason;
        },
        onError: (message) => {
          streaming = false;
          error = message;
        }
      }
    ).abort;
  }

  function onFollowChange() {
    startStream();
  }

  function onReload() {
    startStream();
  }

  function close() {
    if (abortStream) abortStream();
    onclose();
  }

  const endLabel = $derived(ended ? REASON_LABELS[ended] ?? `Session ended (${ended}).` : '');

  onMount(startStream);
  onDestroy(() => {
    if (abortStream) abortStream();
  });
</script>

<div class="modal-overlay" onclick={close} role="presentation"></div>
<div class="logs-modal">
  <div class="logs-header">
    <div>
      <h3 class="logs-title">Logs — {instance.name}</h3>
      <p class="logs-sub">
        {#if streaming}
          <span class="live-dot"></span> streaming
        {:else}
          <span class="static-dot"></span> static
        {/if}
      </p>
    </div>
    <div class="logs-actions">
      <label class="logs-toggle">
        <input type="checkbox" bind:checked={follow} onchange={onFollowChange} />
        <span>Follow</span>
      </label>
      <button class="modal-cancel" onclick={onReload}>Reload</button>
      <button class="modal-cancel" onclick={close}>&times;</button>
    </div>
  </div>

  {#if error}
    <div class="error-badge logs-error">{error}</div>
  {:else}
    <div class="logs-body">
      {#if lines.length === 0 && streaming}
        <p class="empty-text">Waiting for output…</p>
      {:else if lines.length === 0}
        <p class="empty-text">No log output.</p>
      {:else}
        {#each lines as line, i (i)}
          <div class="log-line" class:stderr={line.stream === 'stderr'}>
            <span class="log-stream">{line.stream === 'stderr' ? 'E' : 'O'}</span>
            <span class="log-text" use:renderHtml={ansiToHtml(line.text)}></span>
          </div>
        {/each}
      {/if}
    </div>
    {#if ended}
      <div class="logs-ended">{endLabel}</div>
    {/if}
  {/if}
</div>

<style>
  .logs-modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(760px, calc(100vw - 2rem));
    max-height: 80vh;
    background: rgba(15, 15, 20, 0.97);
    backdrop-filter: blur(24px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-top: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 16px;
    z-index: 201;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .logs-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 1rem 1.25rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .logs-title {
    font-size: 0.95rem;
    font-weight: 700;
    margin: 0;
  }

  .logs-sub {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.7rem;
    color: #71717a;
    margin: 0.2rem 0 0;
  }

  .live-dot,
  .static-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .live-dot {
    background: #22c55e;
    box-shadow: 0 0 8px #22c55e;
    animation: pulse 1.5s ease-in-out infinite;
  }

  .static-dot {
    background: #52525b;
  }

  .logs-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .logs-toggle {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 0.72rem;
    font-weight: 600;
    color: #a1a1aa;
    cursor: pointer;
    user-select: none;
  }

  .logs-error {
    margin: 1rem 1.25rem 0;
  }

  .logs-body {
    flex: 1;
    overflow-y: auto;
    padding: 0.9rem 1.25rem;
    font-family: 'JetBrains Mono', ui-monospace, 'SF Mono', Menlo, monospace;
    font-size: 0.76rem;
    line-height: 1.5;
    min-height: 220px;
  }

  .log-line {
    display: flex;
    gap: 8px;
    white-space: pre-wrap;
    word-break: break-word;
    font-variant-numeric: tabular-nums;
  }

  .log-stream {
    flex-shrink: 0;
    width: 14px;
    text-align: center;
    font-size: 0.6rem;
    font-weight: 700;
    color: #3b82f6;
    border: 1px solid rgba(59, 130, 246, 0.3);
    border-radius: 4px;
    margin-top: 2px;
    height: 16px;
    line-height: 14px;
  }

  .log-line.stderr .log-stream {
    color: #f87171;
    border-color: rgba(248, 113, 113, 0.3);
  }

  .log-text {
    min-width: 0;
    color: #d4d4d8;
  }

  .logs-ended {
    padding: 0.7rem 1.25rem;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    font-size: 0.75rem;
    color: #a78bfa;
    background: rgba(139, 92, 246, 0.06);
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
</style>

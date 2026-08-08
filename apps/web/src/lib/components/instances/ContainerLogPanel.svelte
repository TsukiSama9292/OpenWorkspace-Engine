<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { streamInstanceLogs } from '$lib/api/instance-logs';
  import { ansiToHtml } from '$lib/logs/ansi';
  import {
    shouldAutoscroll,
    clampFontSize,
    loadLogFontSize,
    saveLogFontSize
  } from '$lib/logs/log-helpers';
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
  let lineStart = $state(0);
  let live = $state(true);
  let wrap = $state(true);
  let pinned = $state(true);
  let fullscreen = $state(false);
  let fontSize = $state(loadLogFontSize());
  let tail = $state(200);
  let ended = $state<string | null>(null);
  let error = $state('');
  let streaming = $state(true);
  let bodyEl: HTMLElement | undefined = $state();

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
      const dropped = lines.length - MAX_LINES;
      lines = lines.slice(dropped);
      lineStart += dropped;
    }
  }

  function startStream() {
    if (abortStream) abortStream();
    abortStream = null;
    lines = [];
    lineStart = 0;
    pinned = true;
    ended = null;
    error = '';
    streaming = true;
    abortStream = streamInstanceLogs(
      instance.id,
      { tail, follow: live },
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

  function onScroll() {
    if (!bodyEl) return;
    pinned = shouldAutoscroll(bodyEl.scrollTop, bodyEl.scrollHeight, bodyEl.clientHeight);
  }

  function changeFont(delta: number) {
    const next = clampFontSize(fontSize + delta);
    if (next !== fontSize) {
      fontSize = next;
      saveLogFontSize(next);
    }
  }

  function close() {
    if (abortStream) abortStream();
    onclose();
  }

  $effect(() => {
    if (!bodyEl) return;
    void lines;
    void wrap;
    void fontSize;
    if (pinned) {
      bodyEl.scrollTop = bodyEl.scrollHeight;
    }
  });

  const endLabel = $derived(ended ? REASON_LABELS[ended] ?? `Session ended (${ended}).` : '');
  const following = $derived(streaming && live && pinned);

  onMount(startStream);
  onDestroy(() => {
    if (abortStream) abortStream();
  });
</script>

<div class="modal-overlay" onclick={close} role="presentation"></div>
<div class="logs-modal" class:fullscreen>
  <div class="logs-header">
    <div class="logs-title-wrap">
      <h3 class="logs-title" title={instance.name}>Logs — {instance.name}</h3>
      <p class="logs-sub" class:paused={streaming && !pinned} aria-live="polite">
        {#if !streaming}
          <span class="static-dot" aria-hidden="true"></span> static
        {:else if following}
          <span class="live-dot" aria-hidden="true"></span> streaming
        {:else if live}
          <span class="paused-dot" aria-hidden="true"></span> paused — scroll to bottom to resume
        {:else}
          <span class="static-dot" aria-hidden="true"></span> static
        {/if}
      </p>
    </div>
    <div class="logs-actions">
      <label class="logs-toggle">
        <input type="checkbox" bind:checked={live} onchange={startStream} />
        <span>Follow</span>
      </label>
      <label class="logs-toggle">
        <input type="checkbox" bind:checked={wrap} />
        <span>Wrap</span>
      </label>
      <div class="font-control" role="group" aria-label="Log font size">
        <button class="modal-cancel font-btn" onclick={() => changeFont(-1)} aria-label="Decrease log font size">A−</button>
        <button class="modal-cancel font-btn" onclick={() => changeFont(1)} aria-label="Increase log font size">A+</button>
      </div>
      <button class="modal-cancel" onclick={() => (fullscreen = !fullscreen)}>
        {fullscreen ? 'Exit' : 'Fullscreen'}
      </button>
      <button class="modal-cancel" onclick={startStream}>Reload</button>
      <button class="modal-cancel" onclick={close} aria-label="Close">&times;</button>
    </div>
  </div>

  {#if error}
    <div class="error-badge logs-error">{error}</div>
  {:else}
    <div
      class="logs-body"
      class:nowrap={!wrap}
      bind:this={bodyEl}
      onscroll={onScroll}
      style:font-size={`${fontSize}px`}
    >
      {#if lines.length === 0 && streaming}
        <p class="empty-text">Waiting for output…</p>
      {:else if lines.length === 0}
        <p class="empty-text">No log output.</p>
      {:else}
        {#each lines as line, i (lineStart + i)}
          <div class="log-line" class:stderr={line.stream === 'stderr'}>
            <span class="log-gutter"><span class="log-line-num">{lineStart + i + 1}</span></span>
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
    width: min(900px, 92vw);
    height: min(82vh, calc(100vh - 3rem));
    background: rgba(15, 15, 20, 0.97);
    backdrop-filter: blur(24px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-top: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 16px;
    z-index: 201;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    transition: width 0.2s ease, height 0.2s ease, border-radius 0.2s ease;
  }

  .logs-modal.fullscreen {
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    transform: none;
    border-radius: 0;
  }

  .logs-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.6rem 1rem;
    padding: 1rem 1.25rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .logs-title-wrap {
    flex: 1 1 220px;
    min-width: 0;
  }

  .logs-title {
    font-size: 0.95rem;
    font-weight: 700;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .logs-sub {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.7rem;
    color: #71717a;
    margin: 0.2rem 0 0;
  }

  .logs-sub.paused {
    color: #a1a1aa;
  }

  .live-dot,
  .static-dot,
  .paused-dot {
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

  .paused-dot {
    background: #eab308;
    box-shadow: 0 0 8px rgba(234, 179, 8, 0.6);
  }

  .static-dot {
    background: #52525b;
  }

  .logs-actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
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

  .font-control {
    display: flex;
    gap: 4px;
  }

  .font-btn {
    min-width: 30px;
    padding: 0.3rem 0.5rem;
  }

  .modal-cancel {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #d4d4d8;
    padding: 0.4rem 0.7rem;
    border-radius: 6px;
    font-size: 0.72rem;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.2s;
  }

  .modal-cancel:hover {
    background: rgba(255, 255, 255, 0.1);
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
    margin-top: 0.5rem;
  }

  .logs-error {
    margin: 1rem 1.25rem 0;
  }

  .logs-body {
    flex: 1;
    overflow-y: auto;
    padding: 0.9rem 1.25rem;
    font-family: 'JetBrains Mono', ui-monospace, 'SF Mono', Menlo, monospace;
    line-height: 1.5;
    min-height: 220px;
    color: #d4d4d8;
  }

  .logs-body.nowrap {
    overflow-x: auto;
  }

  .log-line {
    display: flex;
    gap: 8px;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    font-variant-numeric: tabular-nums;
  }

  .logs-body.nowrap .log-line {
    width: max-content;
    min-width: 100%;
    white-space: pre;
    overflow-wrap: normal;
  }

  .log-gutter {
    flex-shrink: 0;
    width: 30px;
    padding-left: 8px;
    border-left: 3px solid #3b82f6;
    text-align: right;
    color: #52525b;
    user-select: none;
  }

  .log-line.stderr .log-gutter {
    border-left-color: #ef4444;
  }

  .log-line-num {
    font-size: 0.68rem;
    line-height: 1.5;
  }

  .log-text {
    min-width: 0;
    color: #d4d4d8;
  }

  .log-line.stderr .log-text {
    color: #fca5a5;
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

  @media (prefers-reduced-motion: reduce) {
    .logs-modal {
      transition: none;
    }

    .live-dot {
      animation: none;
    }
  }
</style>

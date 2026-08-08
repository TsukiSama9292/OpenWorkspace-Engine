<script lang="ts">
  import {
    DRAG_THRESHOLD_PX,
    buildPath,
    clampWindow,
    defaultWindow,
    domainFor,
    fineRegionStart,
    followWindow,
    formatAxisTime,
    formatChartTime,
    mergedSeries,
    nearestPoint,
    selectionStats,
    ticks,
    timeToX,
    valueToY,
    xToTime
  } from '$lib/chart/timeSeries';
  import type { SeriesPoint } from '$lib/types';

  let {
    fine = [] as SeriesPoint[],
    coarse = [] as SeriesPoint[],
    color = '#6366f1',
    domainMin,
    domainMax,
    format = (v: number) => String(v),
    height = 180,
    width = 600
  }: {
    fine?: SeriesPoint[];
    coarse?: SeriesPoint[];
    color?: string;
    domainMin?: number;
    domainMax?: number;
    format?: (v: number) => string;
    height?: number;
    width?: number;
  } = $props();

  let svg: SVGSVGElement | null = $state(null);

  const dataEnd = $derived.by(() => {
    const all = [...fine, ...coarse];
    return all.length ? Math.max(...all.map(p => p.t)) : 0;
  });

  let window = $state({ start: 0, end: 0 });
  let follow = $state(true);
  let hover = $state<SeriesPoint | null>(null);
  let pinned = $state<SeriesPoint | null>(null);
  let dragging = $state(false);
  let dragStartX = $state(0);
  let dragCurX = $state(0);

  const merged = $derived(mergedSeries(fine, coarse, window.start, window.end));
  const domain = $derived(domainFor([...fine, ...coarse], domainMin, domainMax));
  const boundary = $derived(fineRegionStart(fine));
  const path = $derived(
    buildPath(merged, window.start, window.end, width, height, domain.min, domain.max)
  );
  const boundaryX = $derived(
    boundary >= window.start && boundary <= window.end
      ? timeToX(boundary, window.start, window.end, width)
      : null
  );
  const axis = $derived(ticks(window.start, window.end));
  const ready = $derived(window.end > window.start);
  const readout = $derived(pinned ?? hover);

  const selection = $derived.by(() => {
    if (!dragging) return null;
    const lo = Math.min(dragStartX, dragCurX);
    const hi = Math.max(dragStartX, dragCurX);
    const startT = xToTime(lo, window.start, window.end, width);
    const endT = xToTime(hi, window.start, window.end, width);
    return {
      lo,
      hi,
      startT,
      endT,
      stats: selectionStats(merged, startT, endT)
    };
  });

  // On first data, open the full 24 h window; while following, slide it along
  // as newer samples arrive on the 5 s poll.
  $effect(() => {
    if (dataEnd <= 0) return;
    if (window.end === 0) {
      window = defaultWindow(dataEnd);
    } else if (follow && dataEnd > window.end) {
      window = followWindow(window, dataEnd);
    }
  });

  function pointerToX(e: PointerEvent): number {
    const rect = svg?.getBoundingClientRect();
    if (rect && rect.width > 0) return ((e.clientX - rect.left) / rect.width) * width;
    return e.clientX;
  }

  function onPointerDown(e: PointerEvent) {
    dragging = true;
    dragStartX = pointerToX(e);
    dragCurX = dragStartX;
    svg?.setPointerCapture?.(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    const x = pointerToX(e);
    if (dragging) {
      dragCurX = x;
    } else {
      hover = nearestPoint(merged, x, window.start, window.end, width);
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (!dragging) return;
    svg?.releasePointerCapture?.(e.pointerId);
    dragging = false;
    hover = null;
    if (Math.abs(dragCurX - dragStartX) > DRAG_THRESHOLD_PX) {
      const lo = Math.min(dragStartX, dragCurX);
      const hi = Math.max(dragStartX, dragCurX);
      window = clampWindow(
        xToTime(lo, window.start, window.end, width),
        xToTime(hi, window.start, window.end, width)
      );
      follow = false;
    } else {
      const target = nearestPoint(merged, pointerToX(e), window.start, window.end, width);
      if (target) pinned = pinned?.t === target.t ? null : target;
    }
  }

  function onPointerLeave() {
    hover = null;
  }

  function backToNow() {
    window = defaultWindow(dataEnd);
    follow = true;
    pinned = null;
  }
</script>

<div class="chart-wrap">
  {#if dataEnd > 0}
    <svg
      bind:this={svg}
      class="time-series-chart"
      data-testid="time-series-chart"
      {width}
      {height}
      viewBox="0 0 {width} {height}"
      role="img"
      aria-label="interactive resource time series"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointerleave={onPointerLeave}
    >
      {#if path.line}
        <path class="chart-area" data-testid="chart-area" d={path.area} fill={color} opacity="0.15" />
        <path
          class="chart-line"
          data-testid="chart-line"
          d={path.line}
          fill="none"
          stroke={color}
          stroke-width="1.5"
        />
      {/if}

      {#if boundaryX !== null}
        <line
          class="chart-boundary"
          data-testid="chart-boundary"
          x1={boundaryX}
          x2={boundaryX}
          y1="0"
          y2={height}
          stroke="#52525b"
          stroke-width="1"
          stroke-dasharray="4 3"
        />
        <text class="chart-boundary-label" x={boundaryX + 4} y={14} fill="#71717a" font-size="10">
          {formatAxisTime(boundary)}
        </text>
      {/if}

      {#if selection}
        <rect
          class="chart-drag-select"
          data-testid="chart-drag-select"
          x={selection.lo}
          y="0"
          width={selection.hi - selection.lo}
          height={height}
          fill="#6366f1"
          opacity="0.12"
        />
      {/if}

      {#if readout}
        {@const px = timeToX(readout.t, window.start, window.end, width)}
        <line
          class="chart-crosshair"
          data-testid="chart-crosshair"
          x1={px}
          x2={px}
          y1="0"
          y2={height}
          stroke="#a1a1aa"
          stroke-width="1"
        />
        <circle
          cx={px}
          cy={valueToY(readout.v, domain.min, domain.max, height)}
          r="3"
          fill={color}
        />
      {/if}
    </svg>

    {#if ready}
      <div class="chart-axis" data-testid="chart-axis">
        {#each axis as t (t)}
          <span class="chart-axis-tick" data-testid="chart-axis-tick">{formatAxisTime(t)}</span>
        {/each}
      </div>
    {/if}
  {:else}
    <p class="chart-empty" data-testid="chart-empty">No data yet.</p>
  {/if}

  {#if readout}
    <div class="chart-readout" data-testid="chart-readout">
      <span class="chart-readout-value">{format(readout.v)}</span>
      <span class="chart-readout-time">{formatChartTime(readout.t)}</span>
    </div>
  {/if}

  {#if selection}
    <div class="chart-stats" data-testid="chart-stats">
      <span>
        {formatChartTime(selection.startT)} &rarr; {formatChartTime(selection.endT)}
      </span>
      <span>avg {format(selection.stats.avg)}</span>
      <span>max {format(selection.stats.max)}</span>
      <span>min {format(selection.stats.min)}</span>
    </div>
  {/if}

  {#if dataEnd > 0}
    <div class="chart-toolbar">
      {#if follow}
        <span class="chart-live" data-testid="chart-live">live</span>
      {:else}
        <button type="button" class="chart-back" data-testid="chart-back-to-now" onclick={backToNow}>
          Back to now
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .chart-wrap {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .time-series-chart {
    display: block;
    width: 100%;
    height: auto;
    touch-action: none;
    cursor: crosshair;
  }

  .chart-axis {
    display: flex;
    justify-content: space-between;
    font-size: 0.65rem;
    color: #71717a;
    font-variant-numeric: tabular-nums;
  }

  .chart-empty {
    font-size: 0.85rem;
    color: #71717a;
    padding: 1rem 0;
  }

  .chart-readout {
    position: absolute;
    top: 4px;
    left: 8px;
    display: inline-flex;
    gap: 0.5rem;
    align-items: baseline;
    background: rgba(24, 24, 27, 0.9);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    padding: 0.25rem 0.6rem;
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }

  .chart-readout-value {
    color: #f4f4f5;
    font-weight: 600;
  }

  .chart-readout-time {
    color: #a1a1aa;
  }

  .chart-stats {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    font-size: 0.8rem;
    color: #d4d4d8;
    font-variant-numeric: tabular-nums;
  }

  .chart-toolbar {
    display: flex;
    justify-content: flex-end;
    align-items: center;
  }

  .chart-live {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #86efac;
  }

  .chart-back {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    color: #e4e4e7;
    font-family: inherit;
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
    transition: background 0.15s;
  }

  .chart-back:hover {
    background: rgba(255, 255, 255, 0.12);
  }
</style>

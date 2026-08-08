<script lang="ts">
  import { formatChartTime } from '$lib/chart/timeSeries';

  let {
    values = [] as number[],
    timestamps = [] as number[],
    format = (v: number) => String(v),
    color = '#6366f1',
    width = 96,
    height = 28,
    min,
    max
  }: {
    values?: number[];
    /** Parallel to `values`; when provided (same length) the sparkline gains
     *  a light hover tooltip (value + time) and a click-to-pin highlight. */
    timestamps?: number[];
    format?: (v: number) => string;
    color?: string;
    width?: number;
    height?: number;
    /** Lower bound of the value domain (e.g. 0); defaults to the series min. */
    min?: number;
    /** Upper bound of the value domain (e.g. 100 or a memory limit); defaults
     *  to the series max. With a domain, the sparkline shows magnitude relative
     *  to that bound rather than just its own local min/max shape. */
    max?: number;
  } = $props();

  const stepX = $derived(values.length > 1 ? width / (values.length - 1) : 0);

  const points = $derived.by(() => {
    const dataMin = min ?? Math.min(...values, 0);
    const dataMax = max ?? Math.max(...values, 0);
    const span = dataMax - dataMin || 1;
    return values.map((v, i) => {
      const x = i * stepX;
      const clamped = Math.min(Math.max(v, dataMin), dataMax);
      const y = height - ((clamped - dataMin) / span) * (height - 2) - 1;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    });
  });

  const line = $derived(points.length > 1 ? `M${points.join(' L')}` : '');
  const area = $derived(line ? `${line} L${width},${height} L0,${height} Z` : '');
  const interactive = $derived(timestamps.length === values.length && values.length > 0);

  let svg: SVGSVGElement | null = $state(null);
  let tipIndex = $state<number | null>(null);
  let pinIndex = $state<number | null>(null);

  const tip = $derived.by(() => {
    const i = tipIndex ?? pinIndex;
    return interactive && i !== null ? { index: i, x: i * stepX } : null;
  });

  function pointerToX(e: MouseEvent): number {
    const rect = svg?.getBoundingClientRect();
    if (rect && rect.width > 0) return ((e.clientX - rect.left) / rect.width) * width;
    return e.clientX;
  }

  function indexAt(x: number): number {
    if (!interactive) return -1;
    const i = Math.round(x / (stepX || 1));
    return Math.min(Math.max(i, 0), values.length - 1);
  }

  function onPointerMove(e: PointerEvent) {
    if (interactive) tipIndex = indexAt(pointerToX(e));
  }

  function onPointerLeave() {
    tipIndex = null;
  }

  function onClick(e: MouseEvent) {
    if (!interactive) return;
    const i = indexAt(pointerToX(e));
    pinIndex = pinIndex === i ? null : i;
  }
</script>

<div class="spark-wrap">
  {#if line}
    <svg
      bind:this={svg}
      data-testid="sparkline"
      class="sparkline"
      {width}
      {height}
      viewBox="0 0 {width} {height}"
      preserveAspectRatio="none"
      aria-hidden="true"
      onpointermove={onPointerMove}
      onpointerleave={onPointerLeave}
      onclick={onClick}
    >
      <path class="sparkline-area" d={area} fill={color} opacity="0.15" />
      <path
        class="sparkline-line"
        d={line}
        fill="none"
        stroke={color}
        stroke-width="1.5"
        vector-effect="non-scaling-stroke"
      />
      {#if interactive && pinIndex !== null}
        <line
          class="sparkline-pin"
          data-testid="spark-pin"
          x1={pinIndex * stepX}
          x2={pinIndex * stepX}
          y1="0"
          y2={height}
          stroke="#f4f4f5"
          stroke-width="1"
        />
      {/if}
    </svg>
  {/if}

  {#if tip}
    <div class="spark-tip" data-testid="spark-tip" style="left: {tip.x}px">
      <span class="spark-tip-value">{format(values[tip.index])}</span>
      <span class="spark-tip-time">{formatChartTime(timestamps[tip.index])}</span>
    </div>
  {/if}
</div>

<style>
  .spark-wrap {
    position: relative;
    display: inline-block;
  }

  .sparkline {
    display: block;
    max-width: 100%;
  }

  .spark-tip {
    position: absolute;
    top: -1.6rem;
    transform: translateX(-50%);
    display: inline-flex;
    gap: 0.4rem;
    white-space: nowrap;
    background: rgba(24, 24, 27, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    padding: 0.15rem 0.45rem;
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
    z-index: 2;
  }

  .spark-tip-value {
    color: #f4f4f5;
    font-weight: 600;
  }

  .spark-tip-time {
    color: #a1a1aa;
  }
</style>

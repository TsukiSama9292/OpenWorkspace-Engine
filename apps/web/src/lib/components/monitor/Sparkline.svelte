<script lang="ts">
  let {
    values = [] as number[],
    color = '#6366f1',
    width = 96,
    height = 28
  }: {
    values?: number[];
    color?: string;
    width?: number;
    height?: number;
  } = $props();

  const points = $derived.by(() => {
    const max = Math.max(...values, 0);
    const min = Math.min(...values, 0);
    const span = max - min || 1;
    const stepX = values.length > 1 ? width / (values.length - 1) : 0;
    return values.map((v, i) => {
      const x = i * stepX;
      const y = height - ((v - min) / span) * (height - 2) - 1;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    });
  });

  const line = $derived(points.length > 1 ? `M${points.join(' L')}` : '');
  const area = $derived(line ? `${line} L${width},${height} L0,${height} Z` : '');
</script>

{#if line}
  <svg
    data-testid="sparkline"
    class="sparkline"
    {width}
    {height}
    viewBox="0 0 {width} {height}"
    preserveAspectRatio="none"
    aria-hidden="true"
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
  </svg>
{/if}

<style>
  .sparkline {
    display: block;
    max-width: 100%;
  }
</style>

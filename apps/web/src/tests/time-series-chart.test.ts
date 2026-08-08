import { render, screen, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, it, expect } from 'vitest';
import TimeSeriesChart from '$lib/components/monitor/TimeSeriesChart.svelte';
import {
  clampWindow,
  defaultWindow,
  formatAxisTime,
  ticks,
  timeToX,
  xToTime
} from '$lib/chart/timeSeries';
import type { SeriesPoint } from '$lib/types';

// Fixed "now" so every timestamp and x-position is deterministic.
const NOW = 1_700_000_000;
const WIDTH = 600;

const fine: SeriesPoint[] = Array.from({ length: 10 }, (_, i) => ({
  t: NOW - 135 + i * 15,
  v: i * 10
}));
const coarse: SeriesPoint[] = Array.from({ length: 8 }, (_, i) => ({
  t: NOW - 86_400 + i * 10_800,
  v: 5 + i
}));

function renderChart() {
  return render(TimeSeriesChart, {
    props: { fine, coarse, format: v => String(v), width: WIDTH }
  });
}

const svg = () => screen.getByTestId('time-series-chart');

describe('TimeSeriesChart rendering', () => {
  it('renders the line and area for two or more displayed points', async () => {
    renderChart();
    await tick();
    expect(screen.getByTestId('chart-line')).toBeTruthy();
    expect(screen.getByTestId('chart-area')).toBeTruthy();
  });

  it('shows the empty state when there is no data', async () => {
    render(TimeSeriesChart, { props: { fine: [], coarse: [] } });
    await tick();
    expect(screen.getByTestId('chart-empty')).toBeTruthy();
    expect(screen.queryByTestId('chart-live')).toBeNull();
  });

  it('starts live (following) with a full 24 h window', async () => {
    renderChart();
    await tick();
    expect(screen.getByTestId('chart-live')).toBeTruthy();
    expect(screen.queryByTestId('chart-back-to-now')).toBeNull();
    expect(screen.getAllByTestId('chart-axis-tick').length).toBe(6);
  });

  it('marks the 1-hour boundary where fine data begins', async () => {
    renderChart();
    await tick();
    expect(screen.getByTestId('chart-boundary')).toBeTruthy();
  });
});

describe('TimeSeriesChart hover and pin', () => {
  it('shows a crosshair and readout with the snapped value and time on hover', async () => {
    renderChart();
    await tick();
    const target = fine[5];
    const x = timeToX(target.t, defaultWindow(NOW).start, NOW, WIDTH);
    await fireEvent.pointerMove(svg(), { clientX: x });
    const readout = screen.getByTestId('chart-readout');
    expect(readout.textContent).toContain(String(target.v));
    expect(readout.textContent).toContain(formatAxisTime(target.t));
    expect(screen.getByTestId('chart-crosshair')).toBeTruthy();
  });

  it('pins the readout on a click so it survives pointer leave', async () => {
    renderChart();
    await tick();
    const target = fine[2];
    const x = timeToX(target.t, defaultWindow(NOW).start, NOW, WIDTH);
    await fireEvent.pointerDown(svg(), { clientX: x });
    await fireEvent.pointerUp(svg(), { clientX: x });
    await fireEvent.pointerLeave(svg());
    expect(screen.getByTestId('chart-readout').textContent).toContain(String(target.v));
    expect(screen.getByTestId('chart-crosshair')).toBeTruthy();
  });

  it('clicking the same point again unpins it', async () => {
    renderChart();
    await tick();
    const target = fine[2];
    const x = timeToX(target.t, defaultWindow(NOW).start, NOW, WIDTH);
    await fireEvent.pointerDown(svg(), { clientX: x });
    await fireEvent.pointerUp(svg(), { clientX: x });
    expect(screen.getByTestId('chart-readout')).toBeTruthy();
    await fireEvent.pointerDown(svg(), { clientX: x });
    await fireEvent.pointerUp(svg(), { clientX: x });
    await fireEvent.pointerLeave(svg());
    expect(screen.queryByTestId('chart-readout')).toBeNull();
  });
});

describe('TimeSeriesChart drag to zoom', () => {
  it('highlights the selection and shows live stats while dragging', async () => {
    renderChart();
    await tick();
    await fireEvent.pointerDown(svg(), { clientX: 150 });
    await fireEvent.pointerMove(svg(), { clientX: 450 });
    expect(screen.getByTestId('chart-drag-select')).toBeTruthy();
    const stats = screen.getByTestId('chart-stats');
    expect(stats.textContent).toContain('avg');
    expect(stats.textContent).toContain('max');
    expect(stats.textContent).toContain('min');
  });

  it('zooms to the clamped selection on release and disengages follow', async () => {
    renderChart();
    await tick();
    const start = defaultWindow(NOW).start;
    const x1 = 100;
    const x2 = 200;
    await fireEvent.pointerDown(svg(), { clientX: x1 });
    await fireEvent.pointerMove(svg(), { clientX: x2 });
    await fireEvent.pointerUp(svg(), { clientX: x2 });

    const expected = clampWindow(xToTime(x1, start, NOW, WIDTH), xToTime(x2, start, NOW, WIDTH));
    const labels = ticks(expected.start, expected.end).map(formatAxisTime);
    expect(screen.getAllByTestId('chart-axis-tick').map(el => el.textContent)).toEqual(labels);
    expect(screen.getByTestId('chart-back-to-now')).toBeTruthy();
    expect(screen.queryByTestId('chart-live')).toBeNull();
  });

  it('hides the 1-hour boundary once zoomed fully inside the fine region', async () => {
    renderChart();
    await tick();
    const start = defaultWindow(NOW).start;
    const x1 = timeToX(NOW - 3_000, start, NOW, WIDTH);
    const x2 = timeToX(NOW - 1_800, start, NOW, WIDTH);
    await fireEvent.pointerDown(svg(), { clientX: x1 });
    await fireEvent.pointerMove(svg(), { clientX: x2 });
    await fireEvent.pointerUp(svg(), { clientX: x2 });
    expect(screen.queryByTestId('chart-boundary')).toBeNull();
  });
});

describe('TimeSeriesChart back to now', () => {
  it('restores the 24 h view and re-engages follow', async () => {
    renderChart();
    await tick();
    await fireEvent.pointerDown(svg(), { clientX: 100 });
    await fireEvent.pointerMove(svg(), { clientX: 200 });
    await fireEvent.pointerUp(svg(), { clientX: 200 });
    expect(screen.getByTestId('chart-back-to-now')).toBeTruthy();

    await fireEvent.click(screen.getByTestId('chart-back-to-now'));
    expect(screen.getByTestId('chart-live')).toBeTruthy();
    expect(screen.queryByTestId('chart-back-to-now')).toBeNull();
    expect(screen.getByTestId('chart-boundary')).toBeTruthy();
  });
});

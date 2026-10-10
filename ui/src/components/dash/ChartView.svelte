<script lang="ts">
  // One ECharts instance. Clicks anywhere in a category's band select it, not
  // only on the mark itself, so thin bars and lines are easy to hit.
  import { onMount } from 'svelte';
  import { echarts } from '../../lib/echarts';
  import type { EChartsCoreOption } from 'echarts/core';

  let {
    option,
    horizontal = false,
    pie = false,
    onpick,
    onresize,
  }: {
    option: EChartsCoreOption;
    horizontal?: boolean;
    pie?: boolean;
    /** Category index (axis charts) or data index (pie) that was clicked. */
    onpick?: (index: number, raw?: unknown) => void;
    onresize?: (width: number) => void;
  } = $props();

  let el: HTMLDivElement;
  let chart: ReturnType<typeof echarts.init> | null = null;

  onMount(() => {
    chart = echarts.init(el, null, { renderer: 'canvas' });
    chart.setOption(option, { notMerge: true });
    const ro = new ResizeObserver(() => {
      chart?.resize();
      onresize?.(el.clientWidth);
    });
    ro.observe(el);
    onresize?.(el.clientWidth);

    chart.on('click', (p) => {
      if (pie) onpick?.(p.dataIndex, (p.data as { raw?: unknown })?.raw);
    });
    const zr = chart.getZr();
    zr.on('click', (e) => {
      if (pie || !chart || !onpick) return;
      const pt = [e.offsetX, e.offsetY];
      if (!chart.containPixel('grid', pt)) return;
      const v = chart.convertFromPixel('grid', pt) as number[];
      const i = Math.round(horizontal ? v[1] : v[0]);
      if (Number.isFinite(i) && i >= 0) onpick(i);
    });
    zr.on('mousemove', (e) => {
      if (!chart || pie || !onpick) return;
      zr.setCursorStyle(chart.containPixel('grid', [e.offsetX, e.offsetY]) ? 'pointer' : 'default');
    });
    return () => {
      ro.disconnect();
      chart?.dispose();
      chart = null;
    };
  });

  $effect(() => {
    chart?.setOption(option, { notMerge: true, lazyUpdate: true });
  });
</script>

<div class="chart" bind:this={el}></div>

<style>
  .chart { width: 100%; height: 100%; min-height: 60px; }
</style>

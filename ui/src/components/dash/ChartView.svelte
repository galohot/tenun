<script lang="ts">
  // One ECharts instance. Clicks anywhere in a category's band select it, not
  // only on the mark itself, so thin bars and lines are easy to hit. A scatter reports the dot that
  // was hit (`onitem`), and a histogram reports the number under the pointer (`onx`).
  import { onMount } from 'svelte';
  import { echarts } from '../../lib/echarts';
  import type { EChartsCoreOption } from 'echarts/core';

  let {
    option,
    horizontal = false,
    onpick,
    onitem,
    onx,
    onresize,
  }: {
    option: EChartsCoreOption;
    horizontal?: boolean;
    /** The category position that was clicked. */
    onpick?: (index: number) => void;
    /** The series and the position in it of a dot that was clicked. */
    onitem?: (series: number, index: number) => void;
    /** The value on the horizontal number axis where the chart was clicked. */
    onx?: (x: number) => void;
    onresize?: (width: number, height: number) => void;
  } = $props();

  let el: HTMLDivElement;
  let chart: ReturnType<typeof echarts.init> | null = null;

  onMount(() => {
    chart = echarts.init(el, null, { renderer: 'canvas' });
    chart.setOption(option, { notMerge: true });
    const ro = new ResizeObserver(() => {
      chart?.resize();
      onresize?.(el.clientWidth, el.clientHeight);
    });
    ro.observe(el);
    onresize?.(el.clientWidth, el.clientHeight);

    const zr = chart.getZr();
    if (onitem) {
      chart.on('click', (e) => {
        if (e.componentType === 'series' && e.seriesIndex !== undefined && e.dataIndex !== undefined) onitem(e.seriesIndex, e.dataIndex);
      });
    }
    zr.on('click', (e) => {
      if (!chart || !(onpick || onx)) return;
      const pt = [e.offsetX, e.offsetY];
      if (!chart.containPixel('grid', pt)) return;
      const v = chart.convertFromPixel('grid', pt) as number[];
      if (onx) return onx(v[0]);
      const i = Math.round(horizontal ? v[1] : v[0]);
      if (Number.isFinite(i) && i >= 0) onpick?.(i);
    });
    zr.on('mousemove', (e) => {
      if (!chart || !(onpick || onx)) return;
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

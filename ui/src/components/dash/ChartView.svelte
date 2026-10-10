<script lang="ts">
  // One ECharts instance. Clicks anywhere in a category's band select it, not
  // only on the mark itself, so thin bars and lines are easy to hit. A scatter reports the dot that
  // was hit (`onitem`), and a histogram reports the number under the pointer (`onx`).
  import { onMount } from 'svelte';
  import { echarts } from '../../lib/echarts';
  import { animates, motion } from '../../lib/motion';
  import type { EChartsCoreOption } from 'echarts/core';

  let {
    option,
    horizontal = false,
    grids = 1,
    onpick,
    onitem,
    onx,
    oncontext,
    onresize,
  }: {
    option: EChartsCoreOption;
    horizontal?: boolean;
    /** How many plots stand above each other (separate panels). A click counts in any of them. */
    grids?: number;
    /** The category position that was clicked. */
    onpick?: (index: number) => void;
    /** The series and the position in it of a dot that was clicked. */
    onitem?: (series: number, index: number, type?: string) => void;
    /** The value on the horizontal number axis where the chart was clicked. */
    onx?: (x: number) => void;
    /** A right-click: the category, the dot or the number under the pointer. */
    oncontext?: (hit: { kind: 'index'; index: number } | { kind: 'item'; series: number; index: number } | { kind: 'x'; x: number }, e: MouseEvent) => void;
    onresize?: (width: number, height: number) => void;
  } = $props();

  /** No animation when the person asked for less motion, or while a picture of the dashboard is made. */
  const still = (o: EChartsCoreOption): EChartsCoreOption => (animates() ? o : { ...o, animation: false });

  let el: HTMLDivElement;
  let chart: ReturnType<typeof echarts.init> | null = null;

  onMount(() => {
    chart = echarts.init(el, null, { renderer: 'canvas', devicePixelRatio: motion.still ? 2 : undefined });
    chart.setOption(still(option), { notMerge: true });
    const ro = new ResizeObserver(() => {
      chart?.resize();
      onresize?.(el.clientWidth, el.clientHeight);
    });
    ro.observe(el);
    onresize?.(el.clientWidth, el.clientHeight);

    const zr = chart.getZr();
    if (onitem) {
      chart.on('click', (e) => {
        if (e.componentType === 'series' && e.seriesIndex !== undefined && e.dataIndex !== undefined) onitem(e.seriesIndex, e.dataIndex, e.dataType);
      });
    }
    // The plot a point falls in, or -1.
    const plotAt = (pt: number[]) => {
      for (let g = 0; g < grids; g++) if (chart?.containPixel({ gridIndex: g }, pt)) return g;
      return -1;
    };
    zr.on('click', (e) => {
      if (!chart || !(onpick || onx)) return;
      const pt = [e.offsetX, e.offsetY];
      const g = plotAt(pt);
      if (g < 0) return;
      const v = chart.convertFromPixel({ gridIndex: g }, pt) as number[];
      if (onx) return onx(v[0]);
      const i = Math.round(horizontal ? v[1] : v[0]);
      if (Number.isFinite(i) && i >= 0) onpick?.(i);
    });
    if (oncontext) {
      // The browser's own menu would cover ours, so it is switched off on the plot.
      chart.on('contextmenu', (e) => {
        if (onitem && e.componentType === 'series' && e.seriesIndex !== undefined && e.dataIndex !== undefined) {
          (e.event?.event as MouseEvent | undefined)?.preventDefault();
          oncontext({ kind: 'item', series: e.seriesIndex, index: e.dataIndex }, e.event!.event as MouseEvent);
        }
      });
      zr.on('contextmenu', (e) => {
        if (!chart || onitem) return;
        const pt = [e.offsetX, e.offsetY];
        if (!chart.containPixel('grid', pt)) return;
        const native = e.event as MouseEvent;
        native.preventDefault();
        const v = chart.convertFromPixel('grid', pt) as number[];
        if (onx) return oncontext({ kind: 'x', x: v[0] }, native);
        const i = Math.round(horizontal ? v[1] : v[0]);
        if (Number.isFinite(i) && i >= 0) oncontext({ kind: 'index', index: i }, native);
      });
    }
    zr.on('mousemove', (e) => {
      if (!chart || !(onpick || onx)) return;
      zr.setCursorStyle(plotAt([e.offsetX, e.offsetY]) >= 0 ? 'pointer' : 'default');
    });
    return () => {
      ro.disconnect();
      chart?.dispose();
      chart = null;
    };
  });

  $effect(() => {
    chart?.setOption(still(option), { notMerge: true, lazyUpdate: true });
  });
</script>

<div class="chart" bind:this={el}></div>

<style>
  .chart { width: 100%; height: 100%; min-height: 60px; }
</style>

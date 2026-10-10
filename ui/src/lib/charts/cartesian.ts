// Bars, lines and areas: the v1 chart types, with stack modes for bars and areas.

import { isPart, type Cell, type QueryResult } from '../api';
import { getLocale, numberFormatter, OTHER_KEY, pctChange, pctShare } from '../format';
import { isShare } from '../showas';
import {
  axisCommon,
  bySeriesPerMeasure,
  esc,
  fold,
  FOLDED,
  key,
  makeLabel,
  OTHER,
  pivot,
  tooltipBase,
  type BuildArgs,
  type Built,
} from './common';
import { normalizeShares, stackMode } from './math';

export function buildCartesian({ res, tile, colors: c, selected, width, height, tables = [] }: BuildArgs): Built {
  const dim0 = res.columns[0];
  const grain = tile.dims[0]?.grain;
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));
  // Reading the locale here makes the chart build again when it changes. Each value
  // has its own format; split series all show the first value.
  const loc = getLocale();
  const perValue = bySeriesPerMeasure(res);
  const nd = res.columns.filter((x) => x.role === 'dimension').length;
  // A value shown as a share or a rank carries its unit; it formats as a percent or "#3".
  const unitOf = (si: number) => res.columns[nd + (perValue ? si : 0)]?.unit;
  const fmtOf = (si: number) => numberFormatter(tile.formats?.[perValue ? si : 0], loc, unitOf(si));
  // The engine's "Other" (a top N): a real group that can be clicked, drawn muted.
  const isOther = (v: Cell | undefined) => !!res.other && v === OTHER_KEY;
  const partialKey = res.partial?.key;
  const label = makeLabel(res, tile);

  // Dates and date parts (months, weekdays, hours) read in order, so they are never folded.
  const temporal = dim0.dtype === 'date' || dim0.dtype === 'datetime' || isPart(grain);
  const horizontal = tile.type === 'bar' && !!tile.horizontal;
  const room = horizontal ? Math.floor((height - 16) / 26) : Math.floor(width / 44);
  // A result that already has its "Other" is never folded again.
  const { categories, series } =
    temporal || tile.type !== 'bar' || res.other ? pivot(res, tile, tables) : fold(pivot(res, tile, tables), Math.max(4, room));
  const multi = series.length > 1;
  // Bars and areas with more than one series stack, side by side, or fill the whole height (100 %).
  // The default is stacked for sums and counts and side by side for everything else. Lines ignore it.
  const mode = tile.type === 'line' ? 'side' : stackMode(tile, perValue);
  const stacked = multi && mode !== 'side';
  const percent = stacked && mode === 'percent';
  // "Same period last year": a grey series lined up with the categories.
  const earlier = res.compare && !multi ? new Map(res.compare.rows.map((r) => [key(r[0]), r[1]])) : null;
  const earlierValues = earlier ? categories.map((v) => (typeof earlier.get(key(v)) === 'number' ? (earlier.get(key(v)) as number) : null)) : null;
  const hasCompare = !!earlierValues && earlierValues.some((v) => v !== null);
  const compareName = res.compare?.label ?? '';
  // The 100 % view draws each category's share and keeps the value for the tooltip.
  const shares = percent ? normalizeShares(series.map((s) => s.values)).shares : null;
  // A share of the total (or of the group) runs from 0 to 100 %, unless a value falls outside that.
  const shareAxis =
    !percent &&
    unitOf(0) === 'percent' &&
    isShare(tile.showAs?.[0]) &&
    series.every((s) => s.values.every((v) => v === null || (v >= 0 && v <= 1.000001)));
  const valueAxis = {
    type: 'value' as const,
    ...axisCommon(c),
    axisLabel: {
      ...axisCommon(c).axisLabel,
      formatter: percent ? (v: number) => `${Math.round(v * 100)}%` : (v: number) => fmtOf(0).short(v),
    },
    splitNumber: 4,
    ...(percent || shareAxis ? { min: 0, max: 1, interval: 0.25 } : {}),
  };
  const catAxis = {
    type: 'category' as const,
    data: categories.map(label),
    ...axisCommon(c),
    splitLine: { show: false },
    axisLabel: {
      ...axisCommon(c).axisLabel,
      width: horizontal ? 120 : undefined,
      overflow: 'truncate' as const,
      // A few categories all get their label; hideOverlap still drops any that truly collide.
      ...(!horizontal && categories.length <= 8 ? { interval: 0 } : {}),
    },
    axisPointer: { type: (tile.type === 'bar' ? 'shadow' : 'line') as 'shadow' | 'line' },
    boundaryGap: tile.type === 'bar',
    inverse: horizontal,
  };

  const radius = horizontal ? [0, 4, 4, 0] : [4, 4, 0, 0];
  // Direct labels before gridlines: when every bar carries its value, the value axis goes.
  const labelled = tile.type === 'bar' && !multi && !hasCompare && categories.length <= (horizontal ? 16 : Math.floor(width / 48));
  // Name the lines at their ends only when they end far enough apart to read.
  const ends = series.map((s) => s.values.at(-1) ?? 0).sort((a, b) => a - b);
  const top = Math.max(1, ...series.flatMap((s) => s.values.map((v) => v ?? 0)));
  const nameEnds =
    tile.type === 'line' && series.length <= 4 && ends.every((v, i) => i === 0 || (v - ends[i - 1]) / top > 0.08);
  // A stacked area puts the biggest series at the base. Colours stay with their series.
  const order = series.map((_, i) => i);
  if (stacked && tile.type === 'area') {
    const total = (i: number) => series[i].values.reduce<number>((sum, v) => sum + Math.abs(v ?? 0), 0);
    const rank = (i: number) => (series[i].name === OTHER ? -1 : total(i));
    order.sort((a, b) => rank(b) - rank(a));
  }
  const built = order.map((si, pos) => {
    const s = series[si];
    const color = s.slot === null ? c.text2 : c.series[(s.slot ?? si) % c.series.length];
    const isTop = pos === order.length - 1;
    const fmt = fmtOf(si);
    const tooltip = { valueFormatter: (v: number | null) => (v === null ? '–' : fmt.tip(v)) };
    const data = s.values.map((v, ci) => ({
      value: shares ? shares[si][ci] : v,
      raw: v,
      itemStyle: isOther(categories[ci])
        ? { opacity: isSel(OTHER_KEY) ? 1 : 0.28, ...(multi ? {} : { color: c.text2 }) }
        : categories[ci] === FOLDED
          ? { opacity: selected ? 0.28 : 1, color: c.grid }
          : { opacity: (isSel(categories[ci]) ? 1 : 0.28) * (categories[ci] === partialKey ? 0.45 : 1) },
    }));
    if (tile.type === 'bar') {
      return [{
        type: 'bar',
        name: s.name,
        stack: stacked ? 'total' : undefined,
        z: 3,
        // With a comparison, the grey bar behind is the wide one and this bar sits inside it (centred on its own axis).
        barMaxWidth: hasCompare ? 12 : 24,
        barCategoryGap: '30%',
        itemStyle: {
          color,
          borderRadius: !stacked || isTop ? radius : 0,
          borderColor: stacked ? c.surface : undefined,
          borderWidth: stacked ? 1 : 0,
        },
        // A value at the bar's end when there is one series and room for it.
        label: {
          show: labelled,
          position: horizontal ? 'right' : 'top',
          color: c.text2,
          fontSize: 11,
          formatter: (p: { value: number }) => fmt.short(p.value),
        },
        // Full-digit labels can be wider than the bar slot; hide the ones that collide.
        labelLayout: { hideOverlap: true },
        tooltip,
        emphasis: { focus: 'none' },
        data,
      }];
    }
    const lineStyle = { width: 2, color, cap: 'round', join: 'round' };
    const itemStyle = { color, borderColor: c.surface, borderWidth: 2 };
    const endLabel = {
      show: categories.length > 2 && (!multi || nameEnds),
      color: c.text2,
      fontSize: 11,
      formatter: multi ? () => s.name : (p: { value: number }) => fmt.short(p.value),
    };
    // An unfinished last period: the line stops one point early and a dashed
    // copy draws the final segment, so a half month never reads as a drop.
    const unfinished = tile.type === 'line' && partialKey !== undefined && categories.at(-1) === partialKey && s.values.length > 1;
    const main = {
      type: 'line',
      name: s.name,
      stack: stacked && tile.type === 'area' ? 'total' : undefined,
      smooth: false,
      showSymbol: categories.length <= 2,
      symbol: 'circle',
      symbolSize: 8,
      lineStyle,
      itemStyle,
      areaStyle: tile.type === 'area' ? { color, opacity: multi ? 0.14 : 0.1 } : undefined,
      emphasis: { focus: 'none', scale: false },
      connectNulls: false,
      tooltip,
      data: unfinished
        ? [...s.values.slice(0, -1), null]
        : shares
          ? shares[si].map((v, i) => ({ value: v, raw: s.values[i] }))
          : s.values,
      endLabel: unfinished ? { show: false } : endLabel,
    };
    if (!unfinished) return [main];
    const n = s.values.length;
    const dashed = {
      type: 'line',
      name: s.name,
      silent: true,
      showSymbol: false,
      lineStyle: { ...lineStyle, type: [4, 4] },
      itemStyle,
      tooltip: { show: false },
      data: s.values.map((v, i) => (i >= n - 2 ? v : null)),
      endLabel,
    };
    return [main, dashed];
  }).flat();

  // The comparison series goes behind the others: grey bars (wider, so they show beside the bar in
  // front) or a 2 px dashed grey line.
  if (hasCompare && earlierValues) {
    (built as object[]).unshift(
      tile.type === 'bar'
        ? {
            type: 'bar',
            name: compareName,
            z: 2,
            // Its own (hidden) category axis, so ECharts centres it on the tick, as it does the bar in front.
            [horizontal ? 'yAxisIndex' : 'xAxisIndex']: 1,
            barMaxWidth: 22,
            barCategoryGap: '30%',
            itemStyle: { color: c.text2, opacity: 0.32, borderRadius: radius },
            emphasis: { focus: 'none' },
            tooltip: { valueFormatter: (v: number | null) => (v === null ? '–' : fmtOf(0).tip(v)) },
            data: earlierValues,
          }
        : {
            type: 'line',
            name: compareName,
            z: 1,
            smooth: false,
            showSymbol: false,
            symbol: 'none',
            lineStyle: { width: 2, color: c.text2, type: [5, 4], cap: 'round' },
            itemStyle: { color: c.text2 },
            emphasis: { focus: 'none', scale: false },
            connectNulls: false,
            tooltip: { valueFormatter: (v: number | null) => (v === null ? '–' : fmtOf(0).tip(v)) },
            data: earlierValues,
          },
    );
  }

  /**
   * Both values and the change between them, for a chart with a comparison. The grey bars sit on their
   * own axis, so the tooltip does not list them: the earlier value is read by position here.
   */
  const compareTip = (all: { marker: string; seriesName: string; value: unknown; dataIndex: number; axisValueLabel: string }[]) => {
    const num = (v: unknown) => (typeof v === 'number' ? v : null);
    const fmt = fmtOf(0);
    // The value first and the comparison last, as the legend names them.
    const ps = all.filter((p) => p.seriesName !== compareName);
    const now = num(ps[0]?.value);
    const before = earlierValues && ps[0] ? earlierValues[ps[0].dataIndex] : null;
    // A relative change of a share or a rank would read oddly, so those show the two values only.
    const change =
      now !== null && before !== null && before !== 0 && !unitOf(0)
        ? `<br/>Change <b>${now > before ? '+' : now < before ? '−' : ''}${pctChange((now - before) / Math.abs(before))}</b>`
        : '';
    const dot = `<span style="display:inline-block;margin-right:4px;border-radius:10px;width:10px;height:10px;background-color:${c.text2}"></span>`;
    return (
      `${esc(all[0]?.axisValueLabel ?? '')}<br/>` +
      ps.map((p) => `${p.marker} ${esc(p.seriesName)} <b>${num(p.value) === null ? '–' : esc(fmt.tip(num(p.value) as number))}</b>`).join('<br/>') +
      `<br/>${dot} ${esc(compareName)} <b>${before === null ? '–' : esc(fmt.tip(before))}</b>` +
      change
    );
  };

  // The category axis, and a hidden twin of it for the grey comparison bars.
  const withBehind = <T extends object>(axis: T) =>
    hasCompare && tile.type === 'bar' ? [axis, { ...axis, show: false, axisPointer: { show: false } }] : axis;
  const longest = Math.max(0, ...categories.map((x) => label(x).length));
  const quietValue = labelled ? { ...valueAxis, axisLabel: { show: false }, splitLine: { show: false } } : valueAxis;
  const option = {
    animationDuration: 350,
    grid: {
      left: 4,
      right: tile.type === 'bar' && horizontal ? 44 : nameEnds ? 72 : multi ? 12 : 44,
      top: multi || hasCompare ? 34 : 14,
      bottom: 4,
      containLabel: true,
    },
    legend: multi || hasCompare
      ? {
          type: 'scroll',
          // The comparison goes behind the others but is named last.
          data: hasCompare ? [series[0].name, compareName] : undefined,
          top: 0,
          left: 0,
          icon: tile.type === 'bar' ? 'roundRect' : 'circle',
          itemWidth: 10,
          itemHeight: 10,
          textStyle: { color: c.text, fontSize: 12 },
          pageTextStyle: { color: c.text2 },
        }
      : undefined,
    tooltip: {
      ...tooltipBase(c),
      trigger: 'axis',
      axisPointer: {
        type: tile.type === 'bar' ? 'shadow' : 'line',
        lineStyle: { color: c.text2, width: 1 },
        shadowStyle: { color: c.grid, opacity: 0.5 },
      },
      ...(percent
        ? {
            // Each row says the share and, beside it, the value it comes from.
            formatter: (ps: { marker: string; seriesName: string; value: number | null; data: { raw: number | null }; axisValueLabel: string }[]) =>
              `${esc(ps[0]?.axisValueLabel ?? '')}<br/>` +
              ps
                .map((p) => {
                  const si = series.findIndex((s) => s.name === p.seriesName);
                  const raw = p.data.raw;
                  return `${p.marker} ${esc(p.seriesName)} <b>${p.value === null ? '–' : p.value === 0 ? '0%' : pctShare(p.value)}</b>${raw === null ? '' : ` · ${esc(fmtOf(si).tip(raw))}`}`;
                })
                .join('<br/>'),
          }
        : hasCompare
          ? { formatter: compareTip }
          : { valueFormatter: (v: number | null) => (v === null ? '–' : fmtOf(0).tip(v)) }),
    },
    xAxis: horizontal ? quietValue : withBehind({ ...catAxis, axisLabel: { ...catAxis.axisLabel, rotate: !temporal && longest > 10 && categories.length > 6 ? 30 : 0 } }),
    yAxis: horizontal ? withBehind(catAxis) : quietValue,
    series: built,
  };
  return { option, categories: categories.map((v) => (v === FOLDED && !res.other ? undefined : v)), note: sizeNote(res) };
}

/**
 * V3-D12: values that differ in size by more than 20× share one axis, so the small ones look flat.
 * Say so under the chart instead of drawing a second axis.
 */
export function sizeNote(res: QueryResult): string | null {
  if (!bySeriesPerMeasure(res)) return null;
  const nd = res.columns.filter((c) => c.role === 'dimension').length;
  const peaks: number[] = [];
  for (let k = nd; k < res.columns.length; k++) {
    const peak = Math.max(0, ...res.rows.map((r) => Math.abs(Number(r[k]) || 0)));
    if (peak > 0) peaks.push(peak);
  }
  if (peaks.length < 2 || Math.max(...peaks) / Math.min(...peaks) <= 20) return null;
  return 'These values have very different sizes, so the smaller ones look flat.';
}

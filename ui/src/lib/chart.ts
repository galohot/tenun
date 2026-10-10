// Builds ECharts options from a query result. Shared by the app and the
// exported viewer, so a dashboard looks the same in both.
//
// Mark specs (dataviz method): bars ≤ 24 px thick with a 4 px rounded end and a
// square base; 2 px lines; area fill at 10 %; a 2 px surface gap between stacked
// segments; hairline solid grid; text in text colours, never in series colours.

import type { EChartsCoreOption } from 'echarts/core';
import type { Cell, QueryResult, Tile } from './api';
import { cell, compact, full } from './format';

export interface Colors {
  surface: string;
  text: string;
  text2: string;
  grid: string;
  series: string[];
  raised: string;
}

export function readColors(el: Element = document.documentElement): Colors {
  const cs = getComputedStyle(el);
  const v = (n: string) => cs.getPropertyValue(n).trim();
  return {
    surface: v('--surface'),
    raised: v('--raised'),
    text: v('--text'),
    text2: v('--axis-text'),
    grid: v('--grid-line'),
    series: [1, 2, 3, 4, 5, 6, 7, 8].map((i) => v(`--s${i}`)),
  };
}

const MAX_SERIES = 8;
const OTHER = 'Other';

/** Key used to compare a category value with a filter value. */
export const key = (v: Cell) => (v === null ? '\u0000null' : String(v));

interface Pivot {
  categories: Cell[];
  series: { name: string; raw: Cell; values: (number | null)[] }[];
}

/** Turns rows [dim0, dim1?, measure...] into categories × series. */
export function pivot(res: QueryResult, tile: Tile): Pivot {
  const nd = res.columns.filter((c) => c.role === 'dimension').length;
  const mi = nd; // first measure column
  const catIdx = new Map<string, number>();
  const categories: Cell[] = [];
  for (const r of res.rows) {
    const k = key(r[0]);
    if (!catIdx.has(k)) {
      catIdx.set(k, categories.length);
      categories.push(r[0]);
    }
  }
  if (nd < 2 || tile.dims.length < 2) {
    const values = categories.map(() => null as number | null);
    for (const r of res.rows) values[catIdx.get(key(r[0]))!] = r[mi] as number | null;
    const name = res.columns[mi]?.name ?? '';
    return { categories, series: [{ name, raw: name, values }] };
  }
  // Split series: keep the biggest seven, fold the rest into Other.
  const totals = new Map<string, { raw: Cell; total: number }>();
  for (const r of res.rows) {
    const k = key(r[1]);
    const t = totals.get(k) ?? { raw: r[1], total: 0 };
    t.total += Math.abs((r[mi] as number) ?? 0);
    totals.set(k, t);
  }
  const ranked = [...totals.entries()].sort((a, b) => b[1].total - a[1].total);
  const keep = ranked.length > MAX_SERIES ? ranked.slice(0, MAX_SERIES - 1) : ranked;
  // Stable colour per entity: order kept series by their value, not by rank.
  keep.sort((a, b) => String(a[1].raw).localeCompare(String(b[1].raw), undefined, { numeric: true }));
  const names = new Map(keep.map(([k], i) => [k, i]));
  const series: Pivot['series'] = keep.map(([, v]) => ({
    name: cell(v.raw, res.columns[1].dtype, tile.dims[1]?.grain),
    raw: v.raw,
    values: categories.map(() => null),
  }));
  const hasOther = ranked.length > keep.length;
  if (hasOther) series.push({ name: OTHER, raw: OTHER, values: categories.map(() => null) });
  for (const r of res.rows) {
    const si = names.get(key(r[1])) ?? (hasOther ? series.length - 1 : -1);
    if (si < 0) continue;
    const ci = catIdx.get(key(r[0]))!;
    const prev = series[si].values[ci] ?? 0;
    series[si].values[ci] = prev + ((r[mi] as number) ?? 0);
  }
  return { categories, series };
}

function tooltipBase(c: Colors) {
  return {
    backgroundColor: c.raised,
    borderWidth: 0,
    padding: [8, 10],
    textStyle: { color: c.text, fontSize: 12, fontFamily: 'inherit' },
    extraCssText: 'box-shadow: 0 2px 12px rgb(0 0 0 / 14%); border-radius: 8px;',
  };
}

function axisCommon(c: Colors) {
  return {
    axisLine: { show: false },
    axisTick: { show: false },
    axisLabel: { color: c.text2, fontSize: 11, hideOverlap: true },
    splitLine: { lineStyle: { color: c.grid, width: 1, type: 'solid' as const } },
  };
}

export interface BuildArgs {
  res: QueryResult;
  tile: Tile;
  colors: Colors;
  /** Category values selected by a cross-filter from this tile. */
  selected: Cell[] | null;
  /** Width in px, for choosing label density. */
  width: number;
}

export function buildOption({ res, tile, colors: c, selected, width }: BuildArgs): EChartsCoreOption {
  const dim0 = res.columns[0];
  const grain = tile.dims[0]?.grain;
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));
  const label = (v: Cell) => cell(v, dim0.dtype, grain, true);

  if (tile.type === 'donut') {
    const mi = res.columns.findIndex((x) => x.role === 'measure');
    let rows = res.rows.filter((r) => typeof r[mi] === 'number' && (r[mi] as number) > 0);
    rows.sort((a, b) => (b[mi] as number) - (a[mi] as number));
    let other = 0;
    if (rows.length > MAX_SERIES) {
      other = rows.slice(MAX_SERIES - 1).reduce((s, r) => s + (r[mi] as number), 0);
      rows = rows.slice(0, MAX_SERIES - 1);
    }
    // Stable colours: by category value, not by size.
    const order = [...rows].sort((a, b) => String(a[0]).localeCompare(String(b[0]), undefined, { numeric: true }));
    const colorOf = new Map(order.map((r, i) => [key(r[0]), c.series[i]]));
    const data = rows.map((r) => ({
      name: label(r[0]),
      value: r[mi] as number,
      raw: r[0],
      itemStyle: { color: colorOf.get(key(r[0])), opacity: isSel(r[0]) ? 1 : 0.28 },
    }));
    if (other > 0) data.push({ name: OTHER, value: other, raw: null, itemStyle: { color: c.text2, opacity: selected ? 0.28 : 0.55 } });
    const narrow = width < 420;
    return {
      animationDuration: 350,
      tooltip: { ...tooltipBase(c), trigger: 'item', valueFormatter: (v: number) => full(v) },
      legend: {
        type: 'scroll',
        orient: narrow ? 'horizontal' : 'vertical',
        [narrow ? 'bottom' : 'right']: 0,
        top: narrow ? undefined : 'middle',
        icon: 'circle',
        itemWidth: 8,
        itemHeight: 8,
        textStyle: { color: c.text, fontSize: 12 },
        pageTextStyle: { color: c.text2 },
      },
      series: [
        {
          type: 'pie',
          radius: ['56%', '82%'],
          center: narrow ? ['50%', '44%'] : ['36%', '50%'],
          padAngle: 0.6,
          itemStyle: { borderColor: c.surface, borderWidth: 2, borderRadius: 3 },
          label: { show: false },
          emphasis: { scale: false },
          data,
        },
      ],
    };
  }

  const { categories, series } = pivot(res, tile);
  const temporal = dim0.dtype === 'date' || dim0.dtype === 'datetime';
  const horizontal = tile.type === 'bar' && !!tile.horizontal;
  const stacked = series.length > 1 && tile.type !== 'line';
  const multi = series.length > 1;
  const valueAxis = {
    type: 'value' as const,
    ...axisCommon(c),
    axisLabel: { ...axisCommon(c).axisLabel, formatter: (v: number) => compact(v) },
    splitNumber: 4,
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
    },
    axisPointer: { type: (tile.type === 'bar' ? 'shadow' : 'line') as 'shadow' | 'line' },
    boundaryGap: tile.type === 'bar',
    inverse: horizontal,
  };

  const radius = horizontal ? [0, 4, 4, 0] : [4, 4, 0, 0];
  const built = series.map((s, si) => {
    const color = s.name === OTHER ? c.text2 : c.series[si % c.series.length];
    const top = si === series.length - 1;
    const data = s.values.map((v, ci) => ({
      value: v,
      itemStyle: { opacity: isSel(categories[ci]) ? 1 : 0.28 },
    }));
    if (tile.type === 'bar') {
      return {
        type: 'bar',
        name: s.name,
        stack: stacked ? 'total' : undefined,
        barMaxWidth: 24,
        barCategoryGap: '30%',
        itemStyle: {
          color,
          borderRadius: !stacked || top ? radius : 0,
          borderColor: stacked ? c.surface : undefined,
          borderWidth: stacked ? 1 : 0,
        },
        // A value at the bar's end when there is one series and room for it.
        label: {
          show: !multi && categories.length <= (horizontal ? 14 : Math.floor(width / 48)),
          position: horizontal ? 'right' : 'top',
          color: c.text2,
          fontSize: 11,
          formatter: (p: { value: number }) => compact(p.value),
        },
        emphasis: { focus: 'none' },
        data,
      };
    }
    return {
      type: 'line',
      name: s.name,
      stack: stacked && tile.type === 'area' ? 'total' : undefined,
      smooth: false,
      showSymbol: categories.length <= 2,
      symbol: 'circle',
      symbolSize: 8,
      lineStyle: { width: 2, color, cap: 'round', join: 'round' },
      itemStyle: { color, borderColor: c.surface, borderWidth: 2 },
      areaStyle: tile.type === 'area' ? { color, opacity: multi ? 0.18 : 0.1 } : undefined,
      emphasis: { focus: 'none', scale: false },
      connectNulls: false,
      data: s.values,
      // End label: the last value of a single series.
      endLabel: {
        show: !multi && categories.length > 2,
        color: c.text2,
        fontSize: 11,
        formatter: (p: { value: number }) => compact(p.value),
      },
    };
  });

  const longest = Math.max(0, ...categories.map((x) => label(x).length));
  return {
    animationDuration: 350,
    grid: {
      left: 4,
      right: tile.type === 'bar' && horizontal ? 40 : multi ? 12 : 44,
      top: multi ? 34 : 14,
      bottom: 4,
      containLabel: true,
    },
    legend: multi
      ? {
          type: 'scroll',
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
      valueFormatter: (v: number | null) => (v === null ? '–' : full(v)),
    },
    xAxis: horizontal ? valueAxis : { ...catAxis, axisLabel: { ...catAxis.axisLabel, rotate: !temporal && longest > 10 && categories.length > 6 ? 30 : 0 } },
    yAxis: horizontal ? catAxis : valueAxis,
    series: built,
  };
}

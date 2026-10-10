// Builds ECharts options from a query result. Shared by the app and the
// exported viewer, so a dashboard looks the same in both.
//
// Mark specs (dataviz method): bars ≤ 24 px thick with a 4 px rounded end and a
// square base; 2 px lines; area fill at 10 %; a 2 px surface gap between stacked
// segments; hairline solid grid; text in text colours, never in series colours.

import type { EChartsCoreOption } from 'echarts/core';
import { isPart, type Cell, type QueryResult, type Tile } from './api';
import { cell, getLocale, measureLabel, numberFormatter, OTHER_KEY, type Named } from './format';

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

/** Several values on one group: each value is its own series. */
const bySeriesPerMeasure = (res: QueryResult) => {
  const nd = res.columns.filter((c) => c.role === 'dimension').length;
  return nd < 2 && res.columns.length - nd > 1;
};

/** Turns rows [dim0, dim1?, measure...] into categories × series. */
export function pivot(res: QueryResult, tile: Tile, tables: Named[] = []): Pivot {
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
    // One series for each value column; a chart with a single value has one.
    const series: Pivot['series'] = [];
    for (let k = mi; k < Math.max(res.columns.length, mi + 1); k++) {
      const values = categories.map(() => null as number | null);
      for (const r of res.rows) values[catIdx.get(key(r[0]))!] = r[k] as number | null;
      const m = tile.measures[k - mi];
      const many = res.columns.length - mi > 1;
      const name = many && m ? measureLabel(m, tables) : (res.columns[k]?.name ?? '');
      series.push({ name, raw: name, values });
    }
    return { categories, series };
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
  /** Size in px, for label density and how many categories fit. */
  width: number;
  height: number;
  /** For the names of several values. */
  tables?: Named[];
}

export interface Built {
  option: EChartsCoreOption;
  /** The category behind each axis position; null for the folded "Other". */
  categories: (Cell | undefined)[];
}

const FOLDED = OTHER_KEY;

/** Keeps the biggest categories that fit and folds the rest into one "Other" bar. */
function fold(p: Pivot, max: number): Pivot {
  if (p.categories.length <= max || p.series.length !== 1) return p;
  const keep = max - 1;
  const values = p.series[0].values;
  const rest = values.slice(keep).reduce<number>((s, v) => s + (v ?? 0), 0);
  return {
    categories: [...p.categories.slice(0, keep), FOLDED],
    series: [{ ...p.series[0], values: [...values.slice(0, keep), rest] }],
  };
}

export function buildOption({ res, tile, colors: c, selected, width, height, tables = [] }: BuildArgs): Built {
  const dim0 = res.columns[0];
  const grain = tile.dims[0]?.grain;
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));
  // Reading the locale here makes the chart build again when it changes. Each value
  // has its own format; split series all show the first value.
  const loc = getLocale();
  const perValue = bySeriesPerMeasure(res);
  const fmtOf = (si: number) => numberFormatter(tile.formats?.[perValue ? si : 0], loc);
  // The engine's "Other" (a top N): a real group that can be clicked, drawn muted.
  const isOther = (v: Cell | undefined) => !!res.other && v === OTHER_KEY;
  const partialKey = res.partial?.key;
  const label = (v: Cell) =>
    v === FOLDED ? 'Other' : cell(v, dim0.dtype, grain, true) + (partialKey !== undefined && v === partialKey ? '*' : '');

  // Dates and date parts (months, weekdays, hours) read in order, so they are never folded.
  const temporal = dim0.dtype === 'date' || dim0.dtype === 'datetime' || isPart(grain);
  const horizontal = tile.type === 'bar' && !!tile.horizontal;
  const room = horizontal ? Math.floor((height - 16) / 26) : Math.floor(width / 44);
  // A result that already has its "Other" is never folded again.
  const { categories, series } =
    temporal || tile.type !== 'bar' || res.other ? pivot(res, tile, tables) : fold(pivot(res, tile, tables), Math.max(4, room));
  // Several values are different things, so they sit side by side instead of piling up.
  const stacked = series.length > 1 && tile.type !== 'line' && !bySeriesPerMeasure(res);
  const multi = series.length > 1;
  const valueAxis = {
    type: 'value' as const,
    ...axisCommon(c),
    axisLabel: { ...axisCommon(c).axisLabel, formatter: (v: number) => fmtOf(0).short(v) },
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
  // Direct labels before gridlines: when every bar carries its value, the value axis goes.
  const labelled = tile.type === 'bar' && !multi && categories.length <= (horizontal ? 16 : Math.floor(width / 48));
  // Name the lines at their ends only when they end far enough apart to read.
  const ends = series.map((s) => s.values.at(-1) ?? 0).sort((a, b) => a - b);
  const top = Math.max(1, ...series.flatMap((s) => s.values.map((v) => v ?? 0)));
  const nameEnds =
    tile.type === 'line' && series.length <= 4 && ends.every((v, i) => i === 0 || (v - ends[i - 1]) / top > 0.08);
  const built = series.map((s, si) => {
    const color = s.name === OTHER ? c.text2 : c.series[si % c.series.length];
    const top = si === series.length - 1;
    const fmt = fmtOf(si);
    const tooltip = { valueFormatter: (v: number | null) => (v === null ? '–' : fmt.tip(v)) };
    const data = s.values.map((v, ci) => ({
      value: v,
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
      data: unfinished ? [...s.values.slice(0, -1), null] : s.values,
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

  const longest = Math.max(0, ...categories.map((x) => label(x).length));
  const quietValue = labelled ? { ...valueAxis, axisLabel: { show: false }, splitLine: { show: false } } : valueAxis;
  const option = {
    animationDuration: 350,
    grid: {
      left: 4,
      right: tile.type === 'bar' && horizontal ? 44 : nameEnds ? 72 : multi ? 12 : 44,
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
      valueFormatter: (v: number | null) => (v === null ? '–' : fmtOf(0).tip(v)),
    },
    xAxis: horizontal ? quietValue : { ...catAxis, axisLabel: { ...catAxis.axisLabel, rotate: !temporal && longest > 10 && categories.length > 6 ? 30 : 0 } },
    yAxis: horizontal ? catAxis : quietValue,
    series: built,
  };
  return { option, categories: categories.map((v) => (v === FOLDED && !res.other ? undefined : v)) };
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

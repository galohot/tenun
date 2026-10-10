// Turns a chart option into a thumbnail: no axes, no legend, no labels, no tooltip, no motion.

import type { EChartsCoreOption } from 'echarts/core';

type Obj = Record<string, unknown>;
const list = (v: unknown): Obj[] => (Array.isArray(v) ? (v as Obj[]) : v ? [v as Obj] : []);

export function miniOption(option: EChartsCoreOption): EChartsCoreOption {
  const o = option as Obj;
  return {
    ...o,
    animation: false,
    legend: { show: false },
    tooltip: { show: false },
    title: undefined,
    dataZoom: undefined,
    grid: { left: 3, right: 3, top: 4, bottom: 3, containLabel: false },
    xAxis: list(o.xAxis).map((a) => ({ ...a, show: false })),
    yAxis: list(o.yAxis).map((a) => ({ ...a, show: false })),
    series: list(o.series).map((s) => ({
      ...s,
      label: { show: false },
      endLabel: { show: false },
      markLine: undefined,
      markPoint: undefined,
      markArea: undefined,
      silent: true,
    })),
  } as EChartsCoreOption;
}

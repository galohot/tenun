// Builds ECharts options from a query result. Shared by the app and the
// exported viewer, so a dashboard looks the same in both. This file only picks
// the builder for the tile type; each family of charts lives in `charts/`.

import { buildCartesian, sizeNote } from './charts/cartesian';
import { buildHistogram } from './charts/histogram';
import { buildScatter } from './charts/scatter';
import { buildWaterfall } from './charts/waterfall';
import type { BuildArgs, Built } from './charts/common';

export { readColors, key, pivot, type BuildArgs, type Built, type Colors } from './charts/common';
export { sizeNote };

/** The tile types that draw with ECharts. The others are small components or tables. */
export const CHART_TYPES = ['bar', 'line', 'area', 'scatter', 'histogram', 'waterfall'] as const;

export function buildOption(args: BuildArgs): Built {
  switch (args.tile.type) {
    case 'scatter':
      return buildScatter(args);
    case 'histogram':
      return buildHistogram(args);
    case 'waterfall':
      return buildWaterfall(args);
    default:
      return buildCartesian(args);
  }
}

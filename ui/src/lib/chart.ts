// Builds ECharts options from a query result. Shared by the app and the
// exported viewer, so a dashboard looks the same in both. This file only picks
// the builder for the tile type; each family of charts lives in `charts/`.

import { buildBox } from './charts/box';
import { buildCartesian, sizeNote } from './charts/cartesian';
import { buildCompare } from './charts/compare';
import { buildGantt } from './charts/gantt';
import { buildHistogram } from './charts/histogram';
import { buildPanels } from './charts/panels';
import { buildSankey } from './charts/sankey';
import { buildScatter } from './charts/scatter';
import { buildWaterfall } from './charts/waterfall';
import { bySeriesPerMeasure, type BuildArgs, type Built } from './charts/common';

export { readColors, key, pivot, type BuildArgs, type Built, type Colors } from './charts/common';
export { sizeNote };

/** The tile types that draw with ECharts. The others are small components or tables. */
export const CHART_TYPES = ['bar', 'line', 'area', 'scatter', 'histogram', 'waterfall', 'box', 'compare', 'gantt', 'sankey'] as const;

export function buildOption(args: BuildArgs): Built {
  switch (args.tile.type) {
    case 'scatter':
      return buildScatter(args);
    case 'histogram':
      return buildHistogram(args);
    case 'waterfall':
      return buildWaterfall(args);
    case 'box':
      return buildBox(args);
    case 'compare':
      return buildCompare(args);
    case 'gantt':
      return buildGantt(args);
    case 'sankey':
      return buildSankey(args);
    default:
      // Several values in separate panels (Advanced). One value, or a split, has nothing to separate.
      if (args.tile.layout === 'panels' && bySeriesPerMeasure(args.res) && args.res.columns.length - 1 >= 2) return buildPanels(args);
      return buildCartesian(args);
  }
}

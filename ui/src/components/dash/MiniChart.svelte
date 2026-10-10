<script lang="ts">
  // One chart of a small-multiples tile. It measures itself, so the number of labels and bars fits
  // its own size, and it takes the scale the other charts share.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import { buildOption, type Colors } from '../../lib/chart';
  import { withScale } from '../../lib/charts/small';
  import type { Named } from '../../lib/format';
  import { OTHER_KEY } from '../../lib/format';
  import ChartView from './ChartView.svelte';

  let {
    res,
    tile,
    colors,
    selected,
    tables = [],
    scale,
    onpick,
  }: {
    res: QueryResult;
    /** The tile with the repeat taken off: one group, one value. */
    tile: Tile;
    colors: Colors;
    selected: Cell[] | null;
    tables?: Named[];
    scale: { min: number; max: number; interval?: number } | null;
    onpick?: (value: Cell) => void;
  } = $props();

  let width = $state(200);
  let height = $state(110);
  const horizontal = $derived(tile.type === 'bar' && !!tile.horizontal);
  const built = $derived(buildOption({ res, tile, colors, selected, width, height, tables, noFold: true }));
  const option = $derived(withScale(built.option, scale, horizontal));
</script>

<ChartView
  {option}
  {horizontal}
  onresize={(w, h) => ((width = w), (height = h))}
  onpick={onpick
    ? (i) => {
        const v = built.categories[i];
        // "Other" stands for different groups in each chart, so it cannot filter the others.
        if (i < built.categories.length && v !== undefined && v !== OTHER_KEY) onpick(v);
      }
    : undefined} />

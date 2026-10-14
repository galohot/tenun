<script lang="ts">
  import { drillOptions, drillSetting, drillSettingValue } from '../../lib/drilldown';
  import { api, DATE_PARTS, isPart, type Agg, type Cell, type CondFormat, type Dim, type DType, type Grain, type MapLevel, type MapSpec, type Measure, type NumFormat, type QueryResult, type ShowAs, type Tile, type TileLine, type TileType } from '../../lib/api';
  import { app, go } from '../../lib/state.svelte';
  import { controlOf, MAX_BUTTONS } from '../../lib/params';
  import { aggNames, cell, measureLabel, OTHER_KEY, tileTitle } from '../../lib/format';
  import { isChange } from '../../lib/cells';
  import { MAX_LINES } from '../../lib/charts/lines';
  import { defaultStack, sameUnit, stackableTile, stackMode } from '../../lib/charts/math';
  import { dim0Kind, effectiveShowAs, SHOW_AS_LABELS, showAsOptions, withShowAs } from '../../lib/showas';
  import { ADVANCED_TYPES, advancedLocked, advancedOn, ganttOffered as hasTwoDates, MORE_CHARTS, SHOW_ADVANCED_SWITCH } from '../../lib/tiers';
  import { isCleared, isPeriod } from '../../lib/filters';
  import { defaultDim, defaultMeasure, dtypeOf, isRepeated, MAX_CHART_MEASURES, PIVOT_MAX_MEASURES, PIVOT_MAX_ROWS, type DashFilter } from '../../lib/tiles';
  import { reachableFrom } from '../../lib/links';
  import { addsUpHere, baseFormat, formatOf, namedMeasure, ownFormat, stepsByDate, targetUnit, tileProblem, whyNotAdded } from '../../lib/measures';
  import Icon, { type IconName } from '../Icon.svelte';
  import FieldPicker from './FieldPicker.svelte';
  import PlaceNames from '../PlaceNames.svelte';
  import Suggestions from './Suggestions.svelte';
  import CardEditor from './CardEditor.svelte';
  import TextEditor from './TextEditor.svelte';
  import { readColors } from '../../lib/chart';
  import { normalisePicture } from '../../lib/image';
  import { MAX_BANDS } from '../../lib/charts/bullet';

  let {
    tile,
    current = null,
    filters = [],
    dashGrain = null,
    onchange,
    onremove,
    onclose,
  }: {
    tile: Tile;
    /** What the tile shows now, for the previews under "Also try". */
    current?: { res: QueryResult; key: string } | null;
    filters?: DashFilter[];
    /** The grain the dashboard sets for every tile over time; null is "Auto". */
    dashGrain?: Grain | null;
    onchange: (t: Tile) => void;
    onremove: () => void;
    onclose: () => void;
  } = $props();

  const types: { id: TileType; icon: IconName; label: string }[] = [
    { id: 'bar', icon: 'bar', label: 'Bar' },
    { id: 'line', icon: 'line', label: 'Line' },
    { id: 'area', icon: 'area', label: 'Area' },
    { id: 'share', icon: 'share', label: 'Share' },
    { id: 'pie', icon: 'pie', label: 'Pie' },
    { id: 'donut', icon: 'donut', label: 'Donut' },
    { id: 'kpi', icon: 'kpi', label: 'Number' },
    { id: 'table', icon: 'table', label: 'Table' },
    { id: 'scatter', icon: 'scatter', label: 'Scatter' },
    { id: 'histogram', icon: 'histogram', label: 'Histogram' },
    { id: 'waterfall', icon: 'waterfall', label: 'Waterfall' },
    { id: 'pivot', icon: 'pivot', label: 'Pivot' },
    { id: 'map', icon: 'map', label: 'Map' },
    { id: 'filter', icon: 'filter', label: 'Filter' },
    { id: 'text', icon: 'text', label: 'Text' },
    { id: 'image', icon: 'image', label: 'Image' },
  ];
  // The second group of the picker, shown while Advanced is on (build 12).
  const moreAll = ([
    { id: 'heatmap', icon: 'heatmap', label: 'Heat map' },
    { id: 'funnel', icon: 'funnel', label: 'Funnel' },
    { id: 'box', icon: 'box', label: 'Box plot' },
    { id: 'bullet', icon: 'bullet', label: 'Bullet' },
    { id: 'compare', icon: 'compare', label: 'Compare' },
    { id: 'sankey', icon: 'sankey', label: 'Sankey' },
    { id: 'calendar', icon: 'calendarheat', label: 'Calendar' },
    { id: 'cohort', icon: 'cohort', label: 'Cohort' },
    { id: 'gantt', icon: 'gantt', label: 'Gantt' },
  ] as { id: TileType; icon: IconName; label: string }[]).filter((t) => MORE_CHARTS.includes(t.id));
  // A Gantt chart needs a start and an end, so it is offered when some table has two date columns.
  const ganttOffered = $derived(hasTwoDates(app.tables));
  const moreTypes = $derived(moreAll.filter((t) => t.id !== 'gantt' || ganttOffered || tile.type === 'gantt'));
  const stacks: { id: NonNullable<Tile['stack']>; label: string }[] = [
    { id: 'stacked', label: 'Stacked' },
    { id: 'side', label: 'Side by side' },
    { id: 'percent', label: '100 %' },
  ];
  const grains: Grain[] = ['year', 'quarter', 'month', 'week', 'day'];
  const partNames: Record<string, string> = { month_of_year: 'Month of year', weekday: 'Weekday', hour: 'Hour' };

  const TOP_PRESETS = [5, 10, 20];
  const sorts: { id: string; label: string; sort?: Tile['sort'] }[] = [
    { id: 'auto', label: 'Automatic' },
    { id: 'value-desc', label: 'Biggest first', sort: { by: 'value', desc: true } },
    { id: 'value-asc', label: 'Smallest first', sort: { by: 'value', desc: false } },
    { id: 'label-asc', label: 'A → Z', sort: { by: 'label', desc: false } },
    { id: 'label-desc', label: 'Z → A', sort: { by: 'label', desc: true } },
  ];
  // A funnel starts with the biggest stage, and a box plot with A → Z: that is its "Automatic".
  const funnelSorts: typeof sorts = [
    { id: 'auto', label: 'Biggest first' },
    { id: 'value-asc', label: 'Smallest first', sort: { by: 'value', desc: false } },
    { id: 'label-asc', label: 'A → Z', sort: { by: 'label', desc: false } },
    { id: 'label-desc', label: 'Z → A', sort: { by: 'label', desc: true } },
  ];
  const ganttSorts: typeof sorts = [
    { id: 'auto', label: 'Earliest start first' },
    { id: 'value-desc', label: 'Latest start first', sort: { by: 'value', desc: true } },
    { id: 'label-asc', label: 'A → Z', sort: { by: 'label', desc: false } },
    { id: 'label-desc', label: 'Z → A', sort: { by: 'label', desc: true } },
  ];
  const compareSorts: typeof sorts = [
    { id: 'auto', label: 'Biggest gap first' },
    { id: 'value-desc', label: 'Highest first value', sort: { by: 'value', desc: true } },
    { id: 'value-asc', label: 'Lowest first value', sort: { by: 'value', desc: false } },
    { id: 'label-asc', label: 'A → Z', sort: { by: 'label', desc: false } },
    { id: 'label-desc', label: 'Z → A', sort: { by: 'label', desc: true } },
  ];
  const boxSorts: typeof sorts = [
    { id: 'auto', label: 'A → Z' },
    { id: 'label-desc', label: 'Z → A', sort: { by: 'label', desc: true } },
    { id: 'value-desc', label: 'Highest median first', sort: { by: 'value', desc: true } },
    { id: 'value-asc', label: 'Lowest median first', sort: { by: 'value', desc: false } },
  ];

  // How a value is written. Each value has its own (V3-D9). Unset fields keep the defaults.
  const CURRENCIES = ['IDR', 'USD', 'EUR', 'SGD', 'MZN'];
  const DECIMALS = [0, 1, 2, 3, 4];
  const tableLike = $derived(tile.type === 'table' || tile.type === 'pivot');

  function setFormat(i: number, patch: Partial<NumFormat>) {
    const formats = tile.measures.map((_, j) => tile.formats?.[j] ?? null);
    // Only what differs from the format of the named measure is stored, so swapping the measure never
    // leaves a copy of the old one behind (V4-D106).
    formats[i] = ownFormat(baseFormat(tile, i), formats[i], patch);
    set({ formats: formats.some(Boolean) ? formats : undefined });
  }

  /** Removes a value with its format and its "Show as", so the lists stay in step. */
  function removeMeasure(i: number) {
    const formats = (tile.formats ?? []).filter((_, j) => j !== i);
    const showAs = (tile.showAs ?? []).filter((_, j) => j !== i);
    const cond = (tile.cond ?? []).filter((_, j) => j !== i);
    // A card's other values point at the tile's values by place: the one removed goes, the ones after move up.
    const parts = tile.card?.parts
      ?.filter((p) => p.kind !== 'extra' || p.measure !== i)
      .map((p) => (p.kind === 'extra' && p.measure > i ? { ...p, measure: p.measure - 1 } : p));
    set({
      measures: tile.measures.filter((_, j) => j !== i),
      cond: cond.some(Boolean) ? cond : undefined,
      formats: formats.some(Boolean) ? formats : undefined,
      showAs: showAs.some(Boolean) ? showAs : undefined,
      ...(parts ? { card: { ...tile.card, parts } } : {}),
    });
  }

  /** Another field for value `i`. Its "Divide by" stays. */
  function swapField(i: number, table: string, column: string | null, via?: string) {
    const ms = [...tile.measures];
    ms[i] = { ...defaultMeasure(app.tables, table, column, via), ...(ms[i]?.per ? { per: ms[i].per } : {}) };
    set({ measures: ms });
  }

  /** Value `i` is a measure of the workspace. Its table, column and way of adding up mean nothing then. */
  function swapNamed(i: number, id: string) {
    const ms = [...tile.measures];
    ms[i] = namedMeasure(id);
    set({ measures: ms, ...(isBullet && i === 1 ? { target: undefined } : {}) });
  }

  /**
   * What a field that holds a value needs to offer the measures of the workspace. Not offered where the
   * chart reads a column (a histogram, a box plot, who to follow, a date) or adds flows up (a Sankey chart
   * takes the measures that add up).
   */
  function namedProps(i: number, on = true) {
    if (!on || !namedList.length) return {};
    const list = isSankey ? namedList.filter((x) => measureInfo[x.id]?.additive) : namedList;
    // A value is a number, except in a table, which can show a date or a text as well.
    const kinds = tile.type === 'table' ? undefined : (['number'] as DType[]);
    return { measures: list, kinds, named: tile.measures[i]?.named ?? null, onnamed: (id: string) => swapNamed(i, id) };
  }

  /** Sets or clears what value `i` is divided by. */
  function setPer(i: number, per: Measure | null) {
    const { per: _old, ...rest } = tile.measures[i];
    dividing = null;
    set({ measures: tile.measures.map((x, j) => (j === i ? (per ? { ...rest, per: { table: per.table, column: per.column, agg: per.agg, ...(per.via ? { via: per.via } : {}) } } : rest) : x)) });
  }

  function setShowAs(i: number, how: string) {
    set({ showAs: withShowAs(tile.showAs, i, how as ShowAs, tile.measures.length) });
  }

  // The value whose "Divide by" picker is open.
  let dividing = $state<number | null>(null);

  // Advanced is on for this computer (V3-D1). It decides what the Inspector offers, never how a tile draws.
  const advanced = $derived(advancedOn());
  /** Off, the date chain of the first field, or a hierarchy that holds it (v5 build 01). */
  const drillChoices = $derived(drillOptions(tile, app.snap?.workspace.hierarchies ?? []));
  // A tile of an Advanced type, while the switch is off: it still draws, but its fields wait for the switch.
  const lockedType = $derived(!advanced && ADVANCED_TYPES.includes(tile.type));
  const isHeat = $derived(tile.type === 'heatmap');
  const isFunnel = $derived(tile.type === 'funnel');
  const isBox = $derived(tile.type === 'box');
  const isBullet = $derived(tile.type === 'bullet');
  const isCompare = $derived(tile.type === 'compare');
  const isSankey = $derived(tile.type === 'sankey');
  const isCalendar = $derived(tile.type === 'calendar');
  const isCohort = $derived(tile.type === 'cohort');
  const isGantt = $derived(tile.type === 'gantt');
  // A map (build 06): its place field, or for points its colour field, and its own options.
  const isMap = $derived(tile.type === 'map');
  const mapSpec = $derived<MapSpec>(tile.map ?? { level: 'province', country: 'ID' });
  const mapPoints = $derived(isMap && mapSpec.style === 'points');
  let countries = $state<{ code: string; name: string }[]>([]);
  let placesOpen = $state(false);
  $effect(() => {
    if (!isMap || countries.length) return;
    api
      .places('country')
      .then((r) => (countries = r.places.map((p) => ({ code: p.code, name: p.name }))))
      .catch(() => {});
  });
  function setMap(patch: Partial<MapSpec>) {
    set({ map: { ...mapSpec, ...patch } });
  }
  // A compare chart with one value reads two periods of a second field.
  const periodsMode = $derived(isCompare && tile.measures.length < 2);
  // Their values are fixed by the chart: a start and an end date, or the entity to follow.
  const plainValue = $derived(isGantt || isCohort);

  const m = $derived(tile.measures[0]);
  const rels = $derived(app.snap!.workspace.relationships);
  // The table the other fields are read from. A named measure has none: it adds up the tables of its formula,
  // and a field is related when some one of those tables can reach it.
  const baseTable = $derived(m && !m.named ? m.table : null);
  const measureInfo = $derived(app.snap!.measure_info ?? {});
  const reach = $derived.by(() => {
    if (!m) return null;
    if (!m.named) return reachableFrom(m.table, rels, app.linkHealth, app.linkTables);
    const homes = measureInfo[m.named]?.tables ?? [];
    if (!homes.length) return null;
    return new Set(homes.flatMap((h) => [...reachableFrom(h, rels, app.linkHealth, app.linkTables)]));
  });
  /** The measures of the workspace a value can be (Advanced makes them; a dashboard that uses one works without the switch). */
  const namedList = $derived(app.snap!.workspace.measures ?? []);
  /** The ways a value can be added up: a number column can be summed, any other column only counted. */
  const aggsOf = (x: Measure | undefined): Agg[] =>
    !x || x.column === null
      ? ['count']
      : dtypeOf(app.tables, x.table, x.column) === 'number'
        ? [
            'sum', 'avg', 'min', 'max', 'median', 'p90',
            // Spread is Advanced; a value that already has it keeps it in the list.
            ...(advanced || x.agg === 'stdev' || x.agg === 'variance' ? (['stdev', 'variance'] as Agg[]) : []),
            'count', 'count_distinct',
          ]
        : ['count', 'count_distinct', 'min', 'max'];
  // A Sankey chart adds links together, so it takes a sum or a count.
  const aggs = $derived(isSankey ? aggsOf(m).filter((a) => a === 'sum' || a === 'count') : aggsOf(m));

  function set(patch: Partial<Tile>) {
    onchange({ ...tile, ...patch });
  }

  // "Top N" keeps the biggest groups and shows the rest as one Other bar.
  let customTop = $state(false);
  // The tile prop is a new object after every change, so reset only when the id changes.
  let lastId = '';
  $effect(() => {
    if (tile.id !== lastId) {
      lastId = tile.id;
      customTop = false;
      dividing = null;
    }
  });
  const topKind = $derived(tile.type === 'bar' || isBullet || tile.type === 'table' || tile.type === 'share' || tile.type === 'donut' || tile.type === 'pie' || tile.type === 'pivot' || isHeat);
  const topChoice = $derived(
    !tile.top ? 'all' : tile.top.param ? `param:${tile.top.param}` : customTop || !TOP_PRESETS.includes(tile.top.n) ? 'custom' : String(tile.top.n),
  );

  function setTop(choice: string) {
    customTop = choice === 'custom';
    const { param: _p, ...own } = tile.top ?? { n: 10 };
    if (choice === 'all') set({ top: undefined });
    // A number parameter gives N (v5 build 05); `n` keeps its value now, for a file without the parameter.
    else if (choice.startsWith('param:')) {
      const p = numberParams.find((x) => x.id === choice.slice(6));
      if (p) set({ top: { ...own, n: typeof p.value === 'number' ? Math.max(1, Math.round(p.value)) : own.n, param: p.id } });
    } else if (choice === 'custom') set({ top: { ...own, n: tile.top?.n ?? 15 } });
    else set({ top: { ...own, n: Number(choice) } });
  }

  function setTopN(raw: string) {
    const n = Math.round(Number(raw));
    if (Number.isFinite(n) && n >= 1) set({ top: { ...tile.top, n: Math.min(n, 1000), param: undefined } });
  }

  const sortId = $derived.by(() => {
    if (!tile.sort) return 'auto';
    if (tile.sort.by === 'manual') return 'manual';
    const id = `${tile.sort.by}-${(tile.sort.desc ?? tile.sort.by === 'value') ? 'desc' : 'asc'}`;
    // What a funnel and a box plot do when nothing is chosen is their first choice.
    return (isFunnel && id === 'value-desc') || (isBox && id === 'label-asc') || (isGantt && id === 'value-asc') ? 'auto' : id;
  });
  const sortChoices = $derived([
    ...(isFunnel ? funnelSorts : isBox ? boxSorts : isGantt ? ganttSorts : isCompare ? compareSorts : sorts),
    // Dragging the groups into an order is Advanced; a tile that has one keeps showing it.
    ...(advanced || tile.sort?.by === 'manual' ? [{ id: 'manual', label: 'Manual order' }] : []),
  ] as typeof sorts);

  function setDim(i: number, table: string, column: string, via?: string) {
    const dims = [...tile.dims];
    dims[i] = defaultDim(app.tables, table, column, via);
    const isDate = dims[0] && ['date', 'datetime'].includes(dtypeOf(app.tables, dims[0].table, dims[0].column) ?? '');
    // A calendar has one square for each day.
    if (i === 0 && isCalendar && dims[0]) dims[0] = { ...dims[0], grain: 'day' };
    const patch: Partial<Tile> = { dims };
    // A first group-by picks a sensible chart for the field. A chart that was chosen by name keeps its type.
    if (i === 0 && !tile.dims.length && ['bar', 'line', 'area', 'share', 'donut', 'pie', 'kpi'].includes(tile.type)) {
      patch.type = isDate ? 'line' : 'bar';
      patch.horizontal = !isDate;
    }
    set(patch);
  }

  function removeDim(i: number) {
    set({ dims: tile.dims.filter((_, j) => j !== i) });
  }

  // A table takes any number of values; a bar, line or area takes up to four, and so does a pivot.
  const isPivot = $derived(tile.type === 'pivot');
  // A number card (build 08) shows other values of the tile beside its value.
  const manyValues = $derived(tile.type === 'table' || isPivot || tile.type === 'bar' || tile.type === 'line' || tile.type === 'area' || (tile.type === 'kpi' && !!tile.card));
  const isScatter = $derived(tile.type === 'scatter');
  const isHistogram = $derived(tile.type === 'histogram');
  // Rows by one column field: a pivot, or a heat map (which has one row field and one value).
  const pivotLike = $derived(isPivot || isHeat);
  // A histogram and a box plot read one number column, so there is nothing to add up.
  const numberOnly = $derived(isHistogram || isBox);
  const isText = $derived(tile.type === 'text');
  const isImage = $derived(tile.type === 'image');
  const isFilter = $derived(tile.type === 'filter');
  const isParam = $derived(tile.type === 'param');
  // A note, a picture, a filter and a parameter's control have no values to pick.
  const noFields = $derived(isText || isImage || isFilter || isParam);
  // Parameters (v5 build 05): the one a parameter tile moves, and the number ones a Top N, a target or a line can take.
  const allParams = $derived(app.snap?.workspace.params ?? []);
  const numberParams = $derived(allParams.filter((p) => p.kind !== 'choice'));
  const paramDef = $derived(allParams.find((p) => p.id === tile.param?.id));
  const paramControl = $derived(paramDef ? controlOf(paramDef, tile.param?.style) : 'slider');
  const controlChoices = $derived<[NonNullable<Tile['param']>['style'], string][]>(
    !paramDef
      ? []
      : paramDef.kind !== 'choice'
        ? [['slider', 'Slider'], ['input', 'Number box']]
        : (paramDef.choices ?? []).length <= MAX_BUTTONS
          ? [['buttons', 'Buttons'], ['dropdown', 'Dropdown']]
          : [['dropdown', 'Dropdown']],
  );
  const paramName = (id: string | undefined) => allParams.find((p) => p.id === id)?.name ?? 'a deleted parameter';
  /** A target that takes a parameter: its value is the parameter's now (the tile keeps it too, for a file without it). */
  function setTargetParam(id: string) {
    const p = numberParams.find((x) => x.id === id);
    if (!p) return set({ target: tile.target && tile.target.value > 0 ? { value: tile.target.value } : undefined });
    set({ target: { value: typeof p.value === 'number' ? p.value : (tile.target?.value ?? 0), param: p.id } });
  }
  const filterStyles: { id: NonNullable<Tile['filter']>['style']; label: string; types: DType[] | null }[] = [
    { id: 'list', label: 'List', types: null },
    { id: 'dropdown', label: 'Dropdown', types: null },
    { id: 'range', label: 'Range', types: ['number'] },
    { id: 'dates', label: 'Dates', types: ['date', 'datetime'] },
  ];
  const filterField = $derived(tile.dims[0] ? dtypeOf(app.tables, tile.dims[0].table, tile.dims[0].column) : undefined);
  /** The styles that suit the field: a range needs numbers and dates need a date. */
  const styleChoices = $derived(filterStyles.filter((s) => !s.types || !filterField || s.types.includes(filterField)));

  /** A new field starts with a style that suits it and nothing selected. */
  function setFilterField(table: string, column: string | null, via?: string) {
    if (!column) return;
    const t = dtypeOf(app.tables, table, column);
    const style = t === 'number' ? 'range' : t === 'date' || t === 'datetime' ? 'dates' : 'list';
    set({ dims: [{ table, column, ...(via ? { via } : {}) }], h: tile.h === 1 ? 3 : tile.h, filter: { style } });
  }
  function setFilterStyle(style: NonNullable<Tile['filter']>['style']) {
    // The selection of one style means nothing in another. A dropdown is one row high; a list needs more.
    const height = style === 'dropdown' ? 1 : tile.h === 1 ? 3 : tile.h;
    set({
      h: height,
      filter: {
        style,
        ...(style === 'list' || style === 'dropdown' ? { single: tile.filter?.single } : {}),
        ...(tile.filter?.everyPage ? { everyPage: true } : {}),
      },
    });
  }
  const maxValues = $derived(tile.type === 'table' ? Infinity : isPivot ? PIVOT_MAX_MEASURES : MAX_CHART_MEASURES);
  const chartKind = $derived(tile.type === 'bar' || tile.type === 'line' || tile.type === 'area');
  const repeated = $derived(isRepeated(tile));
  const showSplit = $derived(tile.type === 'bar' || tile.type === 'line' || tile.type === 'area' || tile.type === 'table' || isScatter);
  // Several values draw one series each; otherwise the series are the groups of a split.
  const perValue = $derived(tile.measures.length > 1);
  /** What the engine could not follow in the result on show: a table the same for every group does not add up. */
  const parts = $derived(current?.res.parts);
  /** Why the chart type on show cannot draw these values, in words (V4-D99, V4-D112): shares of rows that are in two groups, for one. */
  const refusedType = $derived(tileProblem(tile, parts, stepsByDate(tile, current?.res), current?.res?.rows));
  /** The same words for a type the tile is not, as the tooltip of its button: a share of rows that are in two groups, for one. The button stays: the tile says it too. */
  const chipWhy = (id: TileType): string | null => (id === tile.type ? null : tileProblem({ ...tile, type: id }, parts, stepsByDate(tile, current?.res), current?.res?.rows));
  const stackAble = $derived(stackableTile({ measures: tile.measures.slice(0, MAX_CHART_MEASURES), showAs: tile.showAs }, perValue, parts));
  // The choice on show: the saved one, or the default for these values.
  const stackNow = $derived(stackMode(tile, perValue, parts));
  const stackDefault = $derived(defaultStack(tile, perValue));
  // Stacking needs more than one series: a split, or several values.
  const hasSeries = $derived((tile.type === 'bar' || tile.type === 'area') && !!tile.measures.length && (!!tile.dims[1] || tile.measures.length > 1));

  // Reference lines (build 07): on bars, lines, areas and scatter charts, at most four.
  const LINE_MENU: { id: TileLine['kind']; label: string }[] = [
    { id: 'target', label: 'Target…' },
    { id: 'average', label: 'Average' },
    { id: 'median', label: 'Median' },
    { id: 'min', label: 'Minimum' },
    { id: 'max', label: 'Maximum' },
    { id: 'trend', label: 'Trend' },
  ];
  const lineChart = $derived(tile.type === 'bar' || tile.type === 'line' || tile.type === 'area' || isScatter);
  const valueChart = $derived(lineChart || isHistogram || tile.type === 'waterfall');
  // A scatter line sits at a value of Y (level) or of X (upright).
  let lineAxis = $state<'x' | 'y'>('y');
  let askTarget = $state(false);
  let targetValue = $state('');
  // A line is read in the unit of the value it is drawn for: Y of a scatter chart, the first value of the others.
  const lineUnit = $derived(targetUnit(tile, isScatter && lineAxis === 'y' ? 1 : 0));
  let targetLabel = $state('');
  const lines = $derived((tile.lines ?? []).filter((l) => l.kind !== 'band'));
  const hasLine = (kind: TileLine['kind']) =>
    lines.some((l) => l.kind === kind && (kind === 'trend' || !isScatter || !l.axis || l.axis === lineAxis));

  function lineWords(l: TileLine): string {
    const value = l.param ? ` from ${paramName(l.param)}` : l.value === undefined ? '' : ` ${cell(l.value, 'number', null, false, formatOf(tile, 0))}`;
    const of = !isScatter || l.kind === 'trend' ? '' : l.axis === 'x' ? ' of X (across)' : l.axis === 'y' ? ' of Y (up)' : l.kind === 'median' ? ' of X and Y' : ' of Y (up)';
    const name = l.label?.trim() || LINE_MENU.find((o) => o.id === l.kind)?.label.replace('…', '') || l.kind;
    return `${name}${l.kind === 'target' ? value : ''}${of}`;
  }

  function setLines(next: TileLine[]) {
    const band = (tile.lines ?? []).filter((l) => l.kind === 'band');
    const all = [...next, ...band];
    set({ lines: all.length ? all : undefined });
  }

  function pickLine(id: string) {
    if (id === 'target') {
      askTarget = true;
      return;
    }
    if (!id || lines.length >= MAX_LINES) return;
    const kind = id as TileLine['kind'];
    setLines([...lines, { kind, ...(isScatter && kind !== 'trend' ? { axis: lineAxis } : {}) }]);
  }

  /** A target line at a number parameter's value (v5 build 05). Not set: a typed value. */
  let targetParam = $state('');
  function addTarget() {
    const p = numberParams.find((x) => x.id === targetParam);
    const v = p ? (typeof p.value === 'number' ? p.value : 0) : Number(targetValue.replace(/\s/g, '')) / lineUnit.scale;
    if ((!p && targetValue.trim() === '') || !Number.isFinite(v) || lines.length >= MAX_LINES) return;
    const label = targetLabel.trim();
    setLines([...lines, { kind: 'target', value: v, ...(p ? { param: p.id } : {}), ...(label ? { label } : {}), ...(isScatter ? { axis: lineAxis } : {}) }]);
    askTarget = false;
    targetValue = '';
    targetLabel = '';
    targetParam = '';
  }

  // How the cells of a value look in a table or pivot.
  function setCond(i: number, next: CondFormat | null) {
    const cond = tile.measures.map((_, j) => tile.cond?.[j] ?? null);
    cond[i] = next;
    set({ cond: cond.some(Boolean) ? cond : undefined });
  }

  function setThreshold(i: number, raw: string) {
    const f = tile.cond?.[i];
    if (!f) return;
    const v = Number(raw.replace(/\s/g, ''));
    const { threshold: _old, ...rest } = f;
    setCond(i, raw.trim() !== '' && Number.isFinite(v) ? { ...rest, threshold: v } : rest);
  }

  /** A bin width of 0 or less, or an empty box, means the engine picks one. */
  function setBinWidth(raw: string) {
    const w = Number(raw);
    set({ bins: raw.trim() !== '' && Number.isFinite(w) && w > 0 ? { width: w } : undefined });
  }
  // ---- Advanced (build 13) ----

  /** A new type keeps the fields that suit it. A Gantt chart and a cohort grid read other kinds of fields than the rest. */
  function pickType(id: TileType) {
    if (id === 'gantt' && !(tile.measures.length === 2 && tile.measures[0].agg === 'min' && tile.measures[1].agg === 'max')) {
      return set({ type: id, measures: [], sort: undefined });
    }
    if (id === 'cohort' && tile.type !== 'cohort') return set({ type: id, measures: [], sort: undefined, top: undefined });
    if (id === 'calendar' && tile.dims[0]) return set({ type: id, dims: [{ ...tile.dims[0], grain: 'day' }, ...tile.dims.slice(1)] });
    set({ type: id });
  }

  /** A start or an end date for a Gantt chart: the earliest start and the latest end of each row. */
  function setDateValue(i: number, table: string, column: string | null, via?: string) {
    if (column === null) return;
    const ms = [...tile.measures];
    ms[i] = { table, column, agg: i === 0 ? 'min' : 'max', ...(via ? { via } : {}) };
    set({ measures: ms });
  }

  /** The column that names who to follow in a cohort grid. It is counted by its different values. */
  function setEntity(table: string, column: string | null, via?: string) {
    if (column === null) return;
    set({ measures: [{ table, column, agg: 'count_distinct', ...(via ? { via } : {}) }, ...tile.measures.slice(1)] });
  }

  /** The value a cohort adds up instead of the share that is active. */
  function setCohortSum(table: string, column: string | null, via?: string) {
    if (!tile.measures[0]) return;
    set({ measures: [tile.measures[0], { table, column, agg: 'sum', ...(via ? { via } : {}) }] });
  }

  // The periods a compare chart can take: every value of its second field, oldest first.
  let periodChoices = $state<Cell[]>([]);
  let periodsFor = '';
  $effect(() => {
    const d = periodsMode ? tile.dims[1] : undefined;
    const id = d ? `${d.table}|${d.column}|${d.grain ?? ''}` : '';
    if (id === periodsFor) return;
    periodsFor = id;
    periodChoices = [];
    if (!d) return;
    api
      .query({ measures: [{ table: d.table, column: null, agg: 'count' }], dimensions: [d], limit: 400 })
      .then((r) => {
        if (periodsFor !== id) return;
        const vs = r.rows.map((x) => x[0]).filter((v) => v !== null);
        periodChoices = vs.sort((a, b) => (typeof a === 'number' && typeof b === 'number' ? a - b : String(a).localeCompare(String(b), undefined, { numeric: true })));
      })
      .catch(() => (periodChoices = []));
  });
  const dim1Type = $derived(tile.dims[1] ? dtypeOf(app.tables, tile.dims[1].table, tile.dims[1].column) : undefined);
  const periodText = (v: Cell) => cell(v, dim1Type ?? 'text', tile.dims[1]?.grain);
  /** The two periods on show: the saved ones, or the two the engine took. */
  const periodsNow = $derived<Cell[]>(tile.periods?.length === 2 ? tile.periods : (current?.res.pair?.periods ?? []));
  function setPeriod(i: 0 | 1, raw: string) {
    const pick = periodChoices.find((v) => String(v) === raw);
    if (pick === undefined) return;
    const next = [periodsNow[0] ?? periodChoices.at(-2) ?? pick, periodsNow[1] ?? periodChoices.at(-1) ?? pick];
    next[i] = pick;
    set({ periods: next });
  }

  // A trend column in a table: the date columns it can take its periods from, the same table first.
  const sparkDates = $derived.by(() => {
    if (!m) return [] as { table: string; column: string }[];
    const out: { table: string; column: string }[] = [];
    const ids = [m.table, ...app.tables.map((t) => t.id).filter((id) => id !== m.table)];
    for (const id of ids) {
      const t = app.table(id);
      if (!t || !reach?.has(id)) continue;
      for (const c of t.columns) if (c.dtype === 'date' || c.dtype === 'datetime') out.push({ table: id, column: c.name });
    }
    return out;
  });
  const SPARK_GRAINS: Grain[] = ['day', 'week', 'month', 'quarter', 'year'];
  function setSpark(on: boolean) {
    if (!on) return set({ spark: undefined });
    const first = sparkDates[0];
    if (first) set({ spark: { ...first, grain: 'month' } });
  }

  // ---- Advanced (build 12) ----

  /** The groups on screen, in the order they have now: the choices for a manual order. */
  const shownGroups = $derived.by(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const r of current?.res.rows ?? []) {
      const v = r[0];
      if (v === null || v === undefined || v === OTHER_KEY) continue;
      const k = String(v);
      if (!seen.has(k)) {
        seen.add(k);
        out.push(k);
      }
    }
    return out;
  });
  /** The manual order as the list shows it: the saved order first, then groups that are new. */
  const manualList = $derived.by(() => {
    const saved = tile.sort?.order ?? [];
    const here = new Set(shownGroups);
    return [...saved.filter((k) => here.has(k) || !shownGroups.length), ...shownGroups.filter((k) => !saved.includes(k))];
  });
  let dragFrom = $state<number | null>(null);

  function setOrder(order: string[]) {
    set({ sort: { by: 'manual', order } });
  }

  function moveGroup(from: number, to: number) {
    if (from === to || to < 0 || to >= manualList.length) return;
    const next = [...manualList];
    const [one] = next.splice(from, 1);
    next.splice(to, 0, one);
    setOrder(next);
  }

  function pickSort(id: string) {
    // Manual starts from the order on screen.
    if (id === 'manual') return setOrder(manualList.length ? manualList : shownGroups);
    set({ sort: sortChoices.find((x) => x.id === id)?.sort });
  }

  const LAYOUTS: { id: NonNullable<Tile['layout']>; label: string; hint: string }[] = [
    { id: 'together', label: 'Together', hint: '' },
    { id: 'panels', label: 'Separate panels', hint: 'Each value gets a plot and an axis of its own. One hover line runs through all of them.' },
    { id: 'indexed', label: 'Indexed', hint: 'Every line starts at 100, so you compare how fast each one grows.' },
  ];
  const layoutNow = $derived(tile.layout ?? 'together');
  /** Panels need two values; an index also works for the groups of a split. */
  const layoutChoices = $derived(LAYOUTS.filter((l) => l.id === 'together' || (l.id === 'panels' ? tile.measures.length > 1 : tile.measures.length > 1 || !!tile.dims[1])));
  const sharesUnit = $derived(sameUnit(tile));

  /** Sets how value `i` is drawn in a bar and line chart; "bar" and "line" fall back to the tile's own kind. */
  function setMix(i: number, how: 'bar' | 'line') {
    const base: ('bar' | 'line')[] = tile.measures.map((_, j) => tile.mix?.[j] ?? (tile.type === 'bar' ? 'bar' : 'line'));
    base[i] = how;
    // All values drawn as the tile's own kind is no combo at all.
    const own = tile.type === 'bar' ? 'bar' : 'line';
    set({ mix: base.every((b) => b === own) ? undefined : base });
  }

  // Reference bands: a muted tint between two values.
  let askBand = $state(false);
  let bandFrom = $state('');
  let bandTo = $state('');
  let bandLabel = $state('');
  const bands = $derived((tile.lines ?? []).filter((l) => l.kind === 'band'));
  // A tile has up to four lines; a bullet chart has up to three bands.
  const bandLimit = $derived(isBullet ? MAX_BANDS : MAX_LINES);

  function addBand() {
    const [a, b] = [Number(bandFrom.replace(/\s/g, '')), Number(bandTo.replace(/\s/g, ''))];
    if (bandFrom.trim() === '' || bandTo.trim() === '' || !Number.isFinite(a) || !Number.isFinite(b) || a === b || bands.length >= bandLimit) return;
    const label = bandLabel.trim();
    set({ lines: [...(tile.lines ?? []), { kind: 'band', value: Math.min(a, b), to: Math.max(a, b), ...(label ? { label } : {}) }] });
    askBand = false;
    bandFrom = bandTo = bandLabel = '';
  }

  const dimTypes = $derived(tile.dims.map((d) => dtypeOf(app.tables, d.table, d.column)));
  const dim0 = $derived(dim0Kind(dimTypes[0], tile.dims[0]?.grain));
  const showAsFor = (i: number) => showAsOptions(tile, dim0, i, current?.res.parts);
  /** A trend needs an order: a date or part of a date, or the two values of a scatter. */
  const trendFits = $derived(isScatter || dim0 === 'date' || dim0 === 'part');
  const menu = $derived(LINE_MENU.filter((o) => (o.id === 'target' ? true : o.id === 'trend' ? trendFits && !hasLine('trend') : !hasLine(o.id))));
  // The same period last year needs a line, area or bar over a date with a grain, one value, no split.
  const compareType = $derived(tile.type === 'bar' || tile.type === 'line' || tile.type === 'area');
  const canCompare = $derived(compareType && dim0 === 'date' && !!tile.dims[0]?.grain && !isPart(tile.dims[0].grain) && tile.measures.length <= 1 && !tile.dims[1]);

  /** A target of 0 or less, or an empty box, clears the target. */
  function setTarget(raw: string) {
    // A value written as a percent is typed as 95 and stored as 0.95 (V4-D110).
    const v = Number(raw.replace(/\s/g, '')) / unit0.scale;
    set({ target: raw.trim() !== '' && Number.isFinite(v) && v > 0 ? { value: v } : undefined });
  }
  /** What the target of this tile is typed in, and the number as it is typed. */
  const unit0 = $derived(targetUnit(tile, 0));
  const typed = (v: number | undefined, scale: number) => (v === undefined ? '' : Math.round(v * scale * 1e8) / 1e8);

  // The picture of an image tile. The desktop app asks the system; a browser asks the page.
  const MAX_IMAGE = 1_048_576;
  let imageError = $state('');
  let fileInput: HTMLInputElement | undefined = $state();

  function readBlob(blob: Blob): Promise<string> {
    return new Promise((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(String(r.result));
      r.onerror = () => reject(new Error('That file could not be read.'));
      r.readAsDataURL(blob);
    });
  }

  /** Checks the size and the kind in words, then keeps the picture in the tile. */
  async function takePicture(blob: Blob) {
    imageError = '';
    if (blob.size > MAX_IMAGE) {
      imageError = 'That picture is too big. Pick one under 1 MB.';
      return;
    }
    // The first bytes decide, not the file name: a name that ends in .png can hold anything.
    const data = normalisePicture(await readBlob(blob));
    if (!data) {
      imageError = 'Pick a PNG, JPEG or WebP picture.';
      return;
    }
    set({ image: { fit: 'contain', alt: '', ...tile.image, data } });
  }

  async function choosePicture() {
    imageError = '';
    try {
      if (app.health?.native) {
        const { paths } = await api.pick({ kind: 'image' });
        if (paths[0]) await takePicture(await api.readImage(paths[0]));
      } else {
        fileInput?.click();
      }
    } catch (e) {
      imageError = e instanceof Error ? e.message : String(e);
    }
  }

  async function onFile(e: Event & { currentTarget: HTMLInputElement }) {
    const f = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (f) await takePicture(f).catch((err) => (imageError = String(err.message ?? err)));
  }
</script>

{#snippet formatRow(i: number)}
  {@const f = formatOf(tile, i)}
  <div class="fmt" role="group" aria-label="Format of this value">
    <span class="label">Format</span>
    <div class="pair">
      <select
        class="field"
        aria-label="Currency"
        value={f?.currency ?? ''}
        onchange={(e) => setFormat(i, { currency: e.currentTarget.value || undefined })}>
        <option value="">No currency</option>
        {#each CURRENCIES as c}<option value={c}>{c}</option>{/each}
      </select>
      <select
        class="field"
        aria-label="Decimals"
        value={f?.decimals === undefined ? 'auto' : String(f.decimals)}
        onchange={(e) => setFormat(i, { decimals: e.currentTarget.value === 'auto' ? undefined : Number(e.currentTarget.value) })}>
        <option value="auto">Auto decimals</option>
        {#each DECIMALS as d}<option value={String(d)}>{d} {d === 1 ? 'decimal' : 'decimals'}</option>{/each}
      </select>
    </div>
    <label class="check">
      <input type="checkbox" checked={f?.compact ?? !tableLike} onchange={(e) => setFormat(i, { compact: e.currentTarget.checked })} />
      Short ({app.locale === 'id' ? '1,2 jt' : '1.2M'})
    </label>
  </div>
{/snippet}

{#snippet valueRows(i: number)}
  {@const x = tile.measures[i]}
  {#if x && !numberOnly}
    {#if x.named}
      <!-- A named measure divides inside its formula. -->
    {:else if x.per}
      <div class="per">
        <span class="label">Divide by</span>
        <FieldPicker
          mode="measure"
          table={x.per.table}
          column={x.per.column} via={x.per.via}
          reachable={reach} base={baseTable}
          onchange={(t, c, v) => setPer(i, defaultMeasure(app.tables, t, c, v))}
          onclear={() => setPer(i, null)} />
        <select
          class="field"
          aria-label="How to add up the value you divide by"
          value={x.per.agg}
          onchange={(e) => setPer(i, { ...x.per!, agg: e.currentTarget.value as Agg })}>
          {#each aggsOf({ ...x.per, per: undefined }) as a}<option value={a}>{aggNames[a]}</option>{/each}
        </select>
      </div>
    {:else if dividing === i}
      <div class="per">
        <span class="label">Divide by</span>
        <FieldPicker mode="measure" table={null} column={null} placeholder="What to divide by" reachable={reach} base={baseTable} onchange={(t, c, v) => setPer(i, defaultMeasure(app.tables, t, c, v))} />
        <button class="link" onclick={() => (dividing = null)}>Cancel</button>
      </div>
    {:else}
      <button class="link" onclick={() => (dividing = i)}>Divide by…</button>
    {/if}
    {#if showAsFor(i).length}
      {@const saved = tile.showAs?.[i] ?? 'value'}
      {@const choices = showAsFor(i)}
      {@const why = whyNotAdded(tile, i, parts)}
      <div class="showas">
        <span class="label">Show as</span>
        <select class="field" aria-label="Show this value as" value={saved} onchange={(e) => setShowAs(i, e.currentTarget.value)}>
          {#each choices as o}<option value={o}>{SHOW_AS_LABELS[o]}</option>{/each}
          {#if !choices.includes(saved)}<option value={saved} disabled>{SHOW_AS_LABELS[saved]} (not available here)</option>{/if}
        </select>
      </div>
      {#if why}<p class="hint tight">{why}, so there is no % of total, running total or year to date.</p>{/if}
    {/if}
  {/if}
  {#if tableLike && x}
    {@const cf = tile.cond?.[i] ?? null}
    {@const how = effectiveShowAs(tile, i, dim0, parts)}
    <div class="cells">
      <span class="label">Cells</span>
      <div class="grains stack" role="radiogroup" aria-label="How the cells of this value look">
        {#each [[null, 'Plain'], ['bars', 'Bars'], ['goodbad', 'Good-bad']] as [id, label]}
          <button
            role="radio"
            aria-checked={(cf?.kind ?? null) === id}
            class:on={(cf?.kind ?? null) === id}
            onclick={() => setCond(i, id ? { ...(cf?.kind === id ? cf : {}), kind: id as CondFormat['kind'] } : null)}>
            {label}
          </button>
        {/each}
      </div>
      {#if cf?.kind === 'goodbad'}
        {#if isChange(how)}
          <p class="hint tight">A change is good above 0 and bad below it.</p>
        {:else}
          <input
            class="field"
            type="number"
            step="any"
            placeholder="Good above… (0)"
            aria-label="The value that splits good from bad"
            value={cf.threshold ?? ''}
            onchange={(e) => setThreshold(i, e.currentTarget.value)} />
        {/if}
        <div class="better" role="radiogroup" aria-label="Which direction is good">
          <span class="label">A higher number is</span>
          <button role="radio" aria-checked={(cf.better ?? 'up') === 'up'} class:on={(cf.better ?? 'up') === 'up'} onclick={() => setCond(i, { ...cf, better: undefined })}>good</button>
          <button role="radio" aria-checked={cf.better === 'down'} class:on={cf.better === 'down'} onclick={() => setCond(i, { ...cf, better: 'down' })}>bad</button>
        </div>
      {/if}
    </div>
  {/if}
  {@render formatRow(i)}
{/snippet}

{#snippet bandsUi()}
      <span class="label sub">Bands</span>
      {#each bands as b}
        <div class="linerow">
          <span>{b.label?.trim() || 'Band'} {cell(b.value ?? 0, 'number', null, false, formatOf(tile, 0))} to {cell(b.to ?? 0, 'number', null, false, formatOf(tile, 0))}</span>
          <button class="btn plain icon" aria-label="Remove band" onclick={() => set({ lines: (tile.lines ?? []).filter((l) => l !== b) })}><Icon name="x" size={12} /></button>
        </div>
      {/each}
      {#if askBand}
        <div class="targetform">
          <input class="field" type="number" step="any" placeholder="From" aria-label="Band from" bind:value={bandFrom} />
          <input class="field" type="number" step="any" placeholder="To" aria-label="Band to" bind:value={bandTo} />
          <input class="field" placeholder="Name (optional)" aria-label="Name of the band" bind:value={bandLabel} />
          <div class="row">
            <button class="btn small" disabled={bandFrom.trim() === '' || bandTo.trim() === ''} onclick={addBand}>Add band</button>
            <button class="link" onclick={() => (askBand = false)}>Cancel</button>
          </div>
        </div>
      {:else if bands.length < bandLimit}
        <button class="link" onclick={() => (askBand = true)}>Add band…</button>
      {/if}
{/snippet}

<aside class="insp">
  <header>
    <h3>Tile</h3>
    <button class="btn plain icon" aria-label="Close panel" onclick={onclose}><Icon name="x" /></button>
  </header>

  {#if advancedLocked(tile)}
    <!-- V3-D2: the tile still draws; only editing its Advanced parts waits for the switch. -->
    <div class="notice" role="note">
      <Icon name="info" size={14} />
      <p>This tile uses Advanced features. Turn them on in Settings to edit them.</p>
      {#if SHOW_ADVANCED_SWITCH}
        <button class="btn small" onclick={() => (app.settingsOpen = true)}>Open Settings</button>
      {/if}
    </div>
  {/if}

  {#if !isParam}
  <div class="sec">
    <div class="types" role="radiogroup" aria-label="Tile type">
      {#each types as t}
        <button
          role="radio"
          aria-checked={tile.type === t.id}
          class:on={tile.type === t.id}
          class:soft={!!chipWhy(t.id)}
          title={chipWhy(t.id) ?? t.label}
          onclick={() => pickType(t.id)}>
          <Icon name={t.icon} size={18} />
          <span>{t.label}</span>
        </button>
      {/each}
    </div>
    {#if refusedType}<p class="hint tight refusal" role="note">{refusedType}</p>{/if}
    {#if advanced && moreTypes.length}
      <span class="label up">More charts</span>
      <div class="types" role="radiogroup" aria-label="More chart types">
        {#each moreTypes as t}
          <button role="radio" aria-checked={tile.type === t.id} class:on={tile.type === t.id} class:soft={!!chipWhy(t.id)} title={chipWhy(t.id) ?? t.label} onclick={() => pickType(t.id)}>
            <Icon name={t.icon} size={18} />
            <span>{t.label}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#if tile.type === 'kpi' || tile.type === 'waterfall' || periodsMode}
      <div class="better" role="radiogroup" aria-label="Which change is good">
        <span class="label">A rise is</span>
        <button role="radio" aria-checked={(tile.better ?? 'up') === 'up'} class:on={(tile.better ?? 'up') === 'up'} onclick={() => set({ better: 'up' })}>good</button>
        <button role="radio" aria-checked={tile.better === 'down'} class:on={tile.better === 'down'} onclick={() => set({ better: 'down' })}>bad</button>
      </div>
    {/if}
    {#if tile.type === 'kpi'}
      <div class="target">
        <span class="label">Target (optional)</span>
        <span class="withunit">
        <input
          class="field"
          type="number"
          min="0"
          step="any"
          placeholder="No target"
          aria-label="Target for this number{unit0.unit ? ` in ${unit0.unit}` : ''}"
          value={typed(tile.target?.value, unit0.scale)}
          disabled={!!tile.target?.param}
          onchange={(e) => setTarget(e.currentTarget.value)} />
        {#if unit0.unit}<span class="unit" aria-hidden="true">{unit0.unit}</span>{/if}
        </span>
        {#if (advanced && numberParams.length) || tile.target?.param}
          <select class="field" aria-label="Where the target comes from" value={tile.target?.param ?? ''} onchange={(e) => setTargetParam(e.currentTarget.value)}>
            <option value="">A number typed here</option>
            {#each numberParams as p (p.id)}<option value={p.id}>The parameter {p.name}</option>{/each}
            {#if tile.target?.param && !numberParams.some((p) => p.id === tile.target?.param)}<option value={tile.target.param}>A deleted parameter</option>{/if}
          </select>
        {/if}
      </div>
    {/if}
    {#if tile.type === 'bar'}
      <label class="check">
        <input type="checkbox" checked={!!tile.horizontal} onchange={(e) => set({ horizontal: e.currentTarget.checked })} />
        Horizontal bars
      </label>
    {/if}
  </div>
  {/if}

  {#if isText}
    <TextEditor {tile} {set} />
  {/if}

  {#if isParam}
    <!-- A parameter's control (v5 build 05): which parameter it moves, and how it looks. -->
    <div class="sec">
      <span class="label">Parameter</span>
      {#if allParams.length}
        <select class="field" aria-label="The parameter this tile moves" value={tile.param?.id ?? ''} onchange={(e) => set({ param: { ...tile.param, id: e.currentTarget.value } })}>
          {#if !paramDef}<option value="">Pick a parameter</option>{/if}
          {#each allParams as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
        {#if paramDef}
          <div class="grains stack" role="radiogroup" aria-label="Control">
            {#each controlChoices as [id, label] (id)}
              <button role="radio" aria-checked={paramControl === id} class:on={paramControl === id} onclick={() => set({ param: { ...tile.param!, style: id } })}>{label}</button>
            {/each}
          </div>
          <p class="hint tight">
            {paramDef.description?.trim() || `A measure reads it as PARAM("${paramDef.name}").`} Change its range or its list in Data, under Parameters.
          </p>
        {/if}
      {:else}
        <p class="hint tight">No parameters yet. Make one in Data, under Parameters: a number on a slider, or a choice from a list.</p>
        <button class="btn small" onclick={() => go({ view: 'data' })}>Open Data</button>
      {/if}
    </div>
  {/if}

  {#if isImage}
    <div class="sec">
      <span class="label">Picture</span>
      <button class="btn" onclick={choosePicture}><Icon name="image" /> {tile.image?.data ? 'Change picture' : 'Choose a picture'}</button>
      <input bind:this={fileInput} class="hidden" type="file" accept="image/png,image/jpeg,image/webp" onchange={onFile} />
      <p class="hint tight">PNG, JPEG or WebP, up to 1 MB.</p>
      {#if imageError}<p class="error" role="alert"><Icon name="warning" size={13} /> {imageError}</p>{/if}
      {#if tile.image?.data}
        <span class="label up">Description</span>
        <input
          class="field"
          placeholder="What the picture shows"
          aria-label="Description of the picture, for screen readers"
          value={tile.image.alt}
          oninput={(e) => set({ image: { ...tile.image!, alt: e.currentTarget.value } })} />
        <div class="grains stack" role="radiogroup" aria-label="How the picture fits">
          {#each [['contain', 'Show all'], ['cover', 'Fill the tile']] as [id, label]}
            <button
              role="radio"
              aria-checked={(tile.image.fit ?? 'contain') === id}
              class:on={(tile.image.fit ?? 'contain') === id}
              onclick={() => set({ image: { ...tile.image!, fit: id as 'contain' | 'cover' } })}>
              {label}
            </button>
          {/each}
        </div>
        <button class="btn plain flush" onclick={() => set({ image: undefined })}><Icon name="trash" /> Remove picture</button>
      {/if}
    </div>
  {/if}

  {#if isFilter}
    <div class="sec">
      <span class="label">Field to filter by</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[0]?.table ?? null}
        column={tile.dims[0]?.column ?? null} via={tile.dims[0]?.via}
        placeholder="Pick a field"
        roles
        onchange={(t, c, v) => setFilterField(t, c, v)}
        onclear={() => set({ dims: [], filter: { style: tile.filter?.style ?? 'list' } })} />
      {#if tile.dims[0]}
        <div class="grains stack" role="radiogroup" aria-label="Filter style">
          {#each styleChoices as o}
            <button role="radio" aria-checked={(tile.filter?.style ?? 'list') === o.id} class:on={(tile.filter?.style ?? 'list') === o.id} onclick={() => setFilterStyle(o.id)}>{o.label}</button>
          {/each}
        </div>
        {#if (tile.filter?.style ?? 'list') === 'list' || tile.filter?.style === 'dropdown'}
          <label class="check">
            <input
              type="checkbox"
              checked={!!tile.filter?.single}
              onchange={(e) => set({ filter: { ...tile.filter!, style: tile.filter?.style ?? 'list', single: e.currentTarget.checked || undefined, values: e.currentTarget.checked ? tile.filter?.values?.slice(0, 1) : tile.filter?.values } })} />
            Pick one value at most
          </label>
        {/if}
        {#if !isCleared(tile.filter)}
          <button
            class="btn plain flush"
            onclick={() =>
              set({
                filter: {
                  style: tile.filter?.style ?? 'list',
                  ...(tile.filter?.single ? { single: true } : {}),
                  ...(tile.filter?.everyPage ? { everyPage: true } : {}),
                },
              })}>Clear the selection</button>
        {/if}
        {#if advanced || tile.filter?.everyPage}
          <label class="check">
            <input
              type="checkbox"
              checked={!!tile.filter?.everyPage}
              disabled={!advanced}
              onchange={(e) => set({ filter: { ...tile.filter!, style: tile.filter?.style ?? 'list', everyPage: e.currentTarget.checked || undefined } })} />
            Apply to every page
          </label>
        {/if}
        <p class="hint tight">
          The selection is saved with the dashboard. It filters every other tile{tile.filter?.everyPage ? ', on every page of the workspace' : ''}, in exported and published files too.
        </p>
      {/if}
    </div>
  {/if}

  {#if !noFields && !lockedType}
  <div class="sec">
    <span class="label">{isScatter ? 'Across (X)' : isGantt ? 'Start date' : isCohort ? 'Who to follow' : isCompare ? 'First value' : isBullet ? 'Value' : numberOnly ? 'Number column' : 'Value'}</span>
    <FieldPicker
      mode="measure"
      table={m?.table ?? null}
      column={m?.column ?? null} via={m?.via}
      placeholder={isGantt ? 'The column the work starts in' : isCohort ? 'A column like customer_id' : numberOnly ? 'A column of numbers' : 'What to measure'}
      numbersOnly={numberOnly}
      datesOnly={isGantt}
      {...namedProps(0, !numberOnly && !plainValue)}
      onchange={(t, c, v) => (isGantt ? setDateValue(0, t, c, v) : isCohort ? setEntity(t, c, v) : swapField(0, t, c, v))} />
    {#if m && isCohort}
      <p class="hint tight">Each different value in this column is one customer, user or item. It joins the cohort of the first period it appears in.</p>
    {/if}
    {#if m && !plainValue}
      <!-- A histogram and a box plot read every row of the column, so there is nothing to add up. -->
      {#if !numberOnly && !m.named}
        <select class="field" value={m.agg} onchange={(e) => set({ measures: [{ ...m, agg: e.currentTarget.value as Agg }, ...tile.measures.slice(1)] })}>
          {#each aggs as a}<option value={a}>{aggNames[a]}</option>{/each}
        </select>
      {/if}
      {@render valueRows(0)}
    {/if}
    {#if isScatter && m}
      <span class="label up">Up (Y)</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder="What to measure"
        reachable={reach} base={baseTable}
        {...namedProps(1)}
        onchange={(t, c, v) => swapField(1, t, c, v)}
        onclear={() => removeMeasure(1)} />
      {#if tile.measures[1]}
        {#if !tile.measures[1].named}
        <select class="field" value={tile.measures[1].agg} onchange={(e) => set({ measures: [m, { ...tile.measures[1], agg: e.currentTarget.value as Agg }, ...tile.measures.slice(2)] })}>
          {#each aggsOf(tile.measures[1]) as a}<option value={a}>{aggNames[a]}</option>{/each}
        </select>
        {/if}
        {@render valueRows(1)}
      {/if}
      <!-- A third value sets the area of each dot (Advanced). -->
      {#if tile.measures[1] && (advanced || tile.measures[2])}
        <span class="label up">Size (optional)</span>
        <FieldPicker
          mode="measure"
          table={tile.measures[2]?.table ?? null}
          column={tile.measures[2]?.column ?? null} via={tile.measures[2]?.via}
          placeholder="Make the dots bigger for more"
          reachable={reach} base={baseTable}
          {...namedProps(2)}
          onchange={(t, c, v) => set({ measures: [m, tile.measures[1], defaultMeasure(app.tables, t, c, v)] })}
          onclear={() => removeMeasure(2)} />
        {#if tile.measures[2]}
          {#if !tile.measures[2].named}
          <select class="field" aria-label="How to add up the size" value={tile.measures[2].agg} onchange={(e) => set({ measures: [m, tile.measures[1], { ...tile.measures[2], agg: e.currentTarget.value as Agg }] })}>
            {#each aggsOf(tile.measures[2]) as a}<option value={a}>{aggNames[a]}</option>{/each}
          </select>
          {/if}
          {@render valueRows(2)}
        {/if}
      {/if}
    {/if}
    {#if isBullet && m}
      <span class="label up">Target (optional)</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder="A value that holds the target"
        reachable={reach} base={baseTable}
        {...namedProps(1)}
        onchange={(t, c, v) => set({ measures: [m, defaultMeasure(app.tables, t, c, v)], target: undefined })}
        onclear={() => removeMeasure(1)} />
      {#if tile.measures[1]}
        {#if !tile.measures[1].named}
        <select class="field" aria-label="How to add up the target" value={tile.measures[1].agg} onchange={(e) => set({ measures: [m, { ...tile.measures[1], agg: e.currentTarget.value as Agg }] })}>
          {#each aggsOf(tile.measures[1]) as a}<option value={a}>{aggNames[a]}</option>{/each}
        </select>
        {/if}
      {:else}
        <span class="withunit">
        <input
          class="field"
          type="number"
          min="0"
          step="any"
          placeholder="Or type one target for every bar"
          aria-label="One target for every bar{unit0.unit ? ` in ${unit0.unit}` : ''}"
          value={typed(tile.target?.value, unit0.scale)}
          onchange={(e) => setTarget(e.currentTarget.value)} />
        {#if unit0.unit}<span class="unit" aria-hidden="true">{unit0.unit}</span>{/if}
        </span>
      {/if}
      <p class="hint tight">The thin bar is the value and the tick is the target. Add bands below for poor, fair and good.</p>
    {/if}
    {#if isCompare && m}
      <span class="label up">Second value (optional)</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder="A second value, or compare two periods"
        reachable={reach} base={baseTable}
        {...namedProps(1)}
        onchange={(t, c, v) => set({ measures: [m, defaultMeasure(app.tables, t, c, v)] })}
        onclear={() => removeMeasure(1)} />
      {#if tile.measures[1]}
        {#if !tile.measures[1].named}
        <select class="field" aria-label="How to add up the second value" value={tile.measures[1].agg} onchange={(e) => set({ measures: [m, { ...tile.measures[1], agg: e.currentTarget.value as Agg }] })}>
          {#each aggsOf(tile.measures[1]) as a}<option value={a}>{aggNames[a]}</option>{/each}
        </select>
        {/if}
        <p class="hint tight">Two dots for each row, joined by a line. The biggest gap comes first.</p>
      {:else}
        <p class="hint tight">With one value, pick a date below and two of its periods. Each row then draws as a line from the first to the second.</p>
      {/if}
    {/if}
    {#if isGantt && m}
      <span class="label up">End date</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder="The column the work ends in"
        datesOnly
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => setDateValue(1, t, c, v)}
        onclear={() => removeMeasure(1)} />
      <p class="hint tight">Each row is a bar from its earliest start to its latest end. The first 100 rows are drawn.</p>
    {/if}
    {#if isCohort && m}
      <span class="label up">Add up a value instead (optional)</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder="Show a total, not a share"
        numbersOnly
        reachable={reach} base={baseTable}
        onchange={setCohortSum}
        onclear={() => removeMeasure(1)} />
    {/if}
    {#if manyValues}
      {#each tile.measures.slice(1) as extra, k}
        <FieldPicker
          mode="measure"
          table={extra.table}
          column={extra.column} via={extra.via}
          reachable={reach} base={baseTable}
          {...namedProps(k + 1)}
          onchange={(t, c, v) => swapField(k + 1, t, c, v)}
          onclear={() => removeMeasure(k + 1)} />
        {@render valueRows(k + 1)}
      {/each}
      {#if m && tile.measures.length < maxValues}
        <FieldPicker
          mode="measure"
          table={null}
          column={null}
          placeholder="Add another value"
          reachable={reach} base={baseTable}
          measures={namedList}
          onnamed={(id) => set({ measures: [...tile.measures, namedMeasure(id)] })}
          onchange={(t, c, v) => set({ measures: [...tile.measures, defaultMeasure(app.tables, t, c, v)] })} />
      {/if}
    {/if}
  </div>

  {#if !isHistogram}
  <div class="sec">
      <span class="label">{isMap ? (mapPoints ? 'Colour (optional)' : 'Place (names or codes)') : tile.type === 'kpi' ? 'Period (a date, for the comparison)' : tile.type === 'share' || tile.type === 'donut' || tile.type === 'pie' ? 'Split into' : isScatter ? 'Dot for each' : pivotLike ? 'Rows' : isFunnel ? 'Stages' : isBox || isBullet ? 'Group by (optional)' : isGantt ? 'Name of each row' : isCalendar ? 'Date (one square for each day)' : isCohort ? 'Date of each event' : isSankey ? 'First stage' : isCompare ? 'Row for each' : 'Group by'}</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[0]?.table ?? null}
        column={tile.dims[0]?.column ?? null} via={tile.dims[0]?.via}
        placeholder={isMap ? (mapPoints ? 'Colour points by, like a kind' : 'A column of places, like province') : tile.type === 'kpi' ? 'A date, for the trend' : isScatter ? 'One dot for each of these' : isFunnel ? 'The steps, like visit and sale' : isBox ? 'One box for each of these' : isBullet ? 'One bar for each of these' : isGantt ? 'A task, a project or an id' : isCalendar || isCohort ? 'A date column' : isSankey ? 'Where it starts, like a channel' : 'Category or date'}
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && setDim(0, t, c, v)}
        onclear={() => removeDim(0)} />
      {#if tile.dims[0] && (dimTypes[0] === 'date' || dimTypes[0] === 'datetime') && !isCalendar && !isGantt}
        <div class="grains" role="radiogroup" aria-label="Date grouping">
          {#each grains as g}
            <button role="radio" aria-checked={tile.dims[0].grain === g} class:on={tile.dims[0].grain === g} onclick={() => set({ dims: [{ ...tile.dims[0], grain: g }, ...tile.dims.slice(1)] })}>
              {g}
            </button>
          {/each}
        </div>
        {#if dashGrain && isPeriod(tile.dims[0].grain)}
          <p class="hint tight">The dashboard shows every chart over time by {dashGrain}. Choose Auto in the toolbar to use this grouping.</p>
        {/if}
        <!-- Parts of the date repeat every year, week or day. A number tile compares periods, so it has none. -->
        {#if tile.type !== 'kpi' && !isCohort}
          <div class="grains parts" role="radiogroup" aria-label="Part of the date">
            {#each DATE_PARTS.filter((g) => g !== 'hour' || dimTypes[0] === 'datetime') as g}
              <button role="radio" aria-checked={tile.dims[0].grain === g} class:on={tile.dims[0].grain === g} onclick={() => set({ dims: [{ ...tile.dims[0], grain: g }, ...tile.dims.slice(1)] })}>
                {partNames[g]}
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {/if}

  {#if tile.type === 'kpi' && m}
    <CardEditor {tile} kpi={current?.res.kpi ?? null} title={tileTitle(tile, app.tables)} colors={readColors()} {reach} base={baseTable} {set} />
  {/if}

  {#if canCompare}
    <div class="sec">
      <span class="label">Compare with</span>
      <select class="field" aria-label="Compare with an earlier period" value={tile.compare ?? ''} onchange={(e) => set({ compare: (e.currentTarget.value || undefined) as Tile['compare'] })}>
        <option value="">Nothing</option>
        <option value="prev_year">Same period last year</option>
        <option value="prev_period">Previous period</option>
      </select>
    </div>
  {:else if tile.compare && compareType}
    <p class="hint">Comparing needs one value, a date with a grain and no split.</p>
  {/if}

  {#if isMap}
    <div class="sec">
      <span class="label">Map</span>
      <select class="field" aria-label="Places on the map" value={mapSpec.level} onchange={(e) => setMap({ level: e.currentTarget.value as MapLevel })}>
        <option value="country">Countries of the world</option>
        <option value="province">Provinces</option>
        <option value="regency">Kabupaten and kota</option>
      </select>
      {#if mapSpec.level !== 'country' || mapPoints}
        <select class="field" aria-label="Country" value={mapSpec.country ?? 'ID'} onchange={(e) => setMap({ country: e.currentTarget.value })}>
          {#if !countries.length}<option value={mapSpec.country ?? 'ID'}>{mapSpec.country ?? 'ID'}</option>{/if}
          {#each countries as c (c.code)}<option value={c.code}>{c.name}</option>{/each}
        </select>
        {#if mapSpec.level !== 'country' && (mapSpec.country ?? 'ID').toUpperCase() !== 'ID'}
          <p class="hint tight">Tenun downloads this country's map once, from geoBoundaries, the first time the tile draws it.</p>
        {/if}
      {/if}
      <div class="grains" role="radiogroup" aria-label="How the map shows the value">
        <button role="radio" aria-checked={!mapPoints} class:on={!mapPoints} onclick={() => setMap({ style: 'filled' })}>Filled</button>
        <button role="radio" aria-checked={mapPoints} class:on={mapPoints} onclick={() => setMap({ style: 'points' })}>Points</button>
      </div>
      {#if mapPoints}
        <span class="label up">Latitude</span>
        <FieldPicker mode="dim" table={mapSpec.lat?.table ?? null} column={mapSpec.lat?.column ?? null} via={mapSpec.lat?.via} placeholder="A column of latitudes" reachable={reach} base={baseTable} onchange={(t, c, v) => c && setMap({ lat: { table: t, column: c, ...(v ? { via: v } : {}) } })} onclear={() => setMap({ lat: undefined })} />
        <span class="label up">Longitude</span>
        <FieldPicker mode="dim" table={mapSpec.lon?.table ?? null} column={mapSpec.lon?.column ?? null} via={mapSpec.lon?.via} placeholder="A column of longitudes" reachable={reach} base={baseTable} onchange={(t, c, v) => c && setMap({ lon: { table: t, column: c, ...(v ? { via: v } : {}) } })} onclear={() => setMap({ lon: undefined })} />
        <p class="hint tight">The value sizes each point. Without one, each point is sized by its number of rows.</p>
      {:else}
        <p class="hint tight">Write places the standard way: names or codes. <button class="linkbtn" onclick={() => (placesOpen = true)}>See the place names</button></p>
      {/if}
    </div>
  {/if}

  {#if isSankey && tile.dims[0]}
    <div class="sec">
      <span class="label">Next stages</span>
      {#each tile.dims.slice(1, 3) as d, k}
        <FieldPicker mode="dim" table={d.table} column={d.column} via={d.via} reachable={reach} base={baseTable} onchange={(t, c, v) => c && setDim(k + 1, t, c, v)} onclear={() => removeDim(k + 1)} />
      {/each}
      {#if tile.dims.length < 3}
        <FieldPicker
          mode="dim"
          table={null}
          column={null}
          placeholder={tile.dims.length < 2 ? 'Where it goes next, like a category' : 'A third stage (optional)'}
          reachable={reach} base={baseTable}
          onchange={(t, c, v) => c && setDim(tile.dims.length, t, c, v)} />
      {/if}
      <p class="hint tight">A band is as wide as its value. A stage shows 25 nodes at most; the smallest are added up as Other.</p>
    </div>
  {/if}

  {#if periodsMode && tile.dims[0]}
    <div class="sec">
      <span class="label">Periods from</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[1]?.table ?? null}
        column={tile.dims[1]?.column ?? null} via={tile.dims[1]?.via}
        placeholder="A date, like the year"
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && set({ dims: [tile.dims[0], defaultDim(app.tables, t, c, v)], periods: undefined })}
        onclear={() => set({ dims: tile.dims.slice(0, 1), periods: undefined })} />
      {#if tile.dims[1] && (dim1Type === 'date' || dim1Type === 'datetime')}
        <div class="grains" role="radiogroup" aria-label="Date grouping of the periods">
          {#each grains as g}
            <button role="radio" aria-checked={tile.dims[1].grain === g} class:on={tile.dims[1].grain === g} onclick={() => set({ dims: [tile.dims[0], { ...tile.dims[1], grain: g }], periods: undefined })}>{g}</button>
          {/each}
        </div>
      {/if}
      {#if tile.dims[1] && periodChoices.length}
        <div class="pair">
          {#each [0, 1] as i}
            <select class="field" aria-label={i === 0 ? 'First period' : 'Second period'} value={String(periodsNow[i] ?? '')} onchange={(e) => setPeriod(i as 0 | 1, e.currentTarget.value)}>
              {#each periodChoices as v}<option value={String(v)}>{periodText(v)}</option>{/each}
            </select>
          {/each}
        </div>
        <p class="hint tight">Not picked: the latest two. A row that rose is good news, unless you set a rise as bad.</p>
      {/if}
    </div>
  {/if}

  {#if showSplit && tile.dims[0] && !repeated && (tile.type === 'table' || isScatter || tile.measures.length < 2)}
    <div class="sec">
      <span class="label">{tile.type === 'table' ? 'Then by' : isScatter ? 'Colour by (optional)' : 'Split by (optional)'}</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[1]?.table ?? null}
        column={tile.dims[1]?.column ?? null} via={tile.dims[1]?.via}
        placeholder={tile.type === 'table' ? 'Another column' : 'Colour by a category'}
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && setDim(1, t, c, v)}
        onclear={() => removeDim(1)} />
    </div>
  {/if}

  {#if showSplit && tile.dims[0] && tile.type !== 'table' && !isScatter && tile.measures.length > 1 && !repeated}
    <p class="hint">Split by is off while a tile shows several values.</p>
  {/if}
  {#if repeated}
    <p class="hint">Split by is off while a tile has small multiples.</p>
  {/if}

  {#if pivotLike}
    {#if isPivot && tile.dims[0]}
      <div class="sec">
        <span class="label">Then by (optional)</span>
        {#each tile.dims.slice(1, PIVOT_MAX_ROWS) as d, k}
          <FieldPicker mode="dim" table={d.table} column={d.column} via={d.via} reachable={reach} base={baseTable} onchange={(t, c, v) => c && setDim(k + 1, t, c, v)} onclear={() => removeDim(k + 1)} />
        {/each}
        {#if tile.dims.length < PIVOT_MAX_ROWS}
          <FieldPicker mode="dim" table={null} column={null} placeholder="Another row field" reachable={reach} base={baseTable} onchange={(t, c, v) => c && setDim(tile.dims.length, t, c, v)} />
        {/if}
      </div>
    {/if}
    <div class="sec">
      <span class="label">Columns by</span>
      <FieldPicker
        mode="dim"
        table={tile.cols?.table ?? null}
        column={tile.cols?.column ?? null} via={tile.cols?.via}
        placeholder="The field across the top"
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && set({ cols: defaultDim(app.tables, t, c, v) })}
        onclear={() => set({ cols: undefined })} />
      {#if tile.cols && ['date', 'datetime'].includes(dtypeOf(app.tables, tile.cols.table, tile.cols.column) ?? '')}
        <div class="grains" role="radiogroup" aria-label="Date grouping of the columns">
          {#each grains as g}
            <button role="radio" aria-checked={tile.cols.grain === g} class:on={tile.cols.grain === g} onclick={() => set({ cols: { ...tile.cols!, grain: g } })}>{g}</button>
          {/each}
        </div>
      {/if}
      {#if isPivot}
        <label class="check">
          <input type="checkbox" checked={tile.totals !== false} onchange={(e) => set({ totals: e.currentTarget.checked ? undefined : false })} />
          Totals
        </label>
        <!-- Groups of the first row fields open and close, each with a subtotal (Advanced). -->
        {#if tile.dims.length >= 2 && (advanced || tile.expand)}
          <label class="check">
            <input type="checkbox" checked={!!tile.expand} disabled={tile.totals === false} onchange={(e) => set({ expand: e.currentTarget.checked ? true : undefined })} />
            Open and close groups
          </label>
          <p class="hint tight">{tile.totals === false ? 'Turn Totals on to open and close groups.' : 'Each group of the first fields gets a subtotal row with an arrow.'}</p>
        {/if}
        <p class="hint tight">The rows are the fields above. Up to 30 columns are shown.</p>
      {:else}
        <p class="hint tight">Each cell is shaded by its number, from light to dark. Up to 30 columns are shown.</p>
      {/if}
    </div>
  {/if}

  {#if tile.type === 'table' && tile.measures.length}
    <div class="sec">
      <label class="check first">
        <input type="checkbox" checked={!!tile.totals} onchange={(e) => set({ totals: e.currentTarget.checked ? true : undefined })} />
        Total row
      </label>
      <!-- One small line for each row, over the last 12 periods (Advanced). -->
      {#if tile.dims.length && (advanced || tile.spark) && (sparkDates.length || tile.spark)}
        <label class="check">
          <input type="checkbox" checked={!!tile.spark} onchange={(e) => setSpark(e.currentTarget.checked)} />
          Trend column
        </label>
        {#if tile.spark}
          <FieldPicker
            mode="dim"
            table={tile.spark.table}
            column={tile.spark.column} via={tile.spark.via}
            datesOnly
            reachable={reach} base={baseTable}
            onchange={(t, c, v) => c && set({ spark: { table: t, column: c, ...(v ? { via: v } : {}), grain: tile.spark?.grain ?? 'month' } })} />
          <div class="grains" role="radiogroup" aria-label="Period of the trend">
            {#each SPARK_GRAINS as g}
              <button role="radio" aria-checked={(tile.spark.grain ?? 'month') === g} class:on={(tile.spark.grain ?? 'month') === g} onclick={() => set({ spark: { ...tile.spark!, grain: g } })}>{g}</button>
            {/each}
          </div>
          <p class="hint tight">Each row gets a line of its first value over the last 12 periods that are over.</p>
        {/if}
      {/if}
    </div>
  {/if}

  {#if hasSeries}
    <div class="sec">
      <span class="label">Several series</span>
      {#if stackAble}
        <div class="grains stack" role="radiogroup" aria-label="How the series are drawn">
          {#each stacks as o}
            <button
              role="radio"
              aria-checked={stackNow === o.id}
              class:on={stackNow === o.id}
              onclick={() => set({ stack: o.id })}>
              {o.label}
            </button>
          {/each}
        </div>
        {#if !tile.stack && stackDefault === 'side'}
          <p class="hint tight">These values are not in one unit, so they sit side by side. Pick Stacked to pile them up.</p>
        {/if}
      {:else}
        <p class="hint tight">{whyNotAdded(tile, 0, parts) ?? 'These values are not added up'}, so they cannot be stacked and sit side by side.</p>
      {/if}
    </div>
  {/if}

  {#if lineChart && m}
    <div class="sec">
      <span class="label">Lines</span>
      {#each lines as l}
        <div class="linerow">
          <span>{lineWords(l)}</span>
          <button class="btn plain icon" aria-label="Remove {lineWords(l)}" onclick={() => setLines(lines.filter((x) => x !== l))}><Icon name="x" size={12} /></button>
        </div>
      {/each}
      {#if lines.length < MAX_LINES}
        {#if isScatter}
          <select class="field" aria-label="Which value the new line is for" bind:value={lineAxis}>
            <option value="y">For Y (up)</option>
            <option value="x">For X (across)</option>
          </select>
        {/if}
        <select class="field" aria-label="Add line" value="" onchange={(e) => { pickLine(e.currentTarget.value); e.currentTarget.value = ''; }}>
          <option value="">Add line…</option>
          {#each menu as o}<option value={o.id}>{o.label}</option>{/each}
        </select>
        {#if askTarget}
          <div class="targetform">
            {#if advanced && numberParams.length}
              <select class="field" aria-label="Where the target comes from" bind:value={targetParam}>
                <option value="">A number typed here</option>
                {#each numberParams as p (p.id)}<option value={p.id}>The parameter {p.name}</option>{/each}
              </select>
            {/if}
            {#if !targetParam}
              <span class="withunit"><input class="field" type="number" step="any" placeholder="Target value" aria-label="Target value{lineUnit.unit ? ` in ${lineUnit.unit}` : ''}" bind:value={targetValue} />{#if lineUnit.unit}<span class="unit" aria-hidden="true">{lineUnit.unit}</span>{/if}</span>
            {/if}
            <input class="field" placeholder="Name (optional)" aria-label="Name of the target" bind:value={targetLabel} />
            <div class="row">
              <button class="btn small" disabled={!targetParam && targetValue.trim() === ''} onclick={addTarget}>Add target</button>
              <button class="link" onclick={() => (askTarget = false)}>Cancel</button>
            </div>
          </div>
        {/if}
      {:else}
        <p class="hint tight">A tile can have up to {MAX_LINES} lines.</p>
      {/if}
      {#if !isScatter && tile.dims[1] && tile.measures.length < 2 && !addsUpHere(tile, 0, parts)}
        <p class="hint tight">{whyNotAdded(tile, 0, parts)}, so on a split chart each line is read from every point drawn, and there is no trend.</p>
      {/if}
    </div>
  {/if}

  {#if valueChart}
    <div class="sec">
      <span class="label">Values on chart</span>
      <select class="field" aria-label="Values written on the chart" value={tile.labels ?? 'auto'} onchange={(e) => set({ labels: e.currentTarget.value === 'auto' ? undefined : (e.currentTarget.value as Tile['labels']) })}>
        <option value="auto">Auto</option>
        <option value="all">All</option>
        <option value="none">None</option>
      </select>
    </div>
  {/if}

  {#if isHistogram}
    <div class="sec">
      <span class="label">Bin width</span>
      <input
        class="field"
        type="number"
        min="0"
        step="any"
        placeholder="auto"
        aria-label="Bin width, or empty for automatic"
        value={tile.bins?.width ?? ''}
        onchange={(e) => setBinWidth(e.currentTarget.value)} />
      <p class="hint tight">Leave it empty and Tenun picks a width that suits the data.</p>
    </div>
  {/if}

  {#if (tile.type === 'pie' || tile.type === 'donut') && tile.dims[0]}
    <!-- A pie or a donut has at most six slices and Other (V4-D7). Not set: five. -->
    <div class="sec">
      <span class="label">Slices</span>
      <select class="field" aria-label="How many slices before Other" value={String(Math.min(6, Math.max(2, tile.top?.n ?? 5)))} onchange={(e) => set({ top: { n: Number(e.currentTarget.value) } })}>
        {#each [2, 3, 4, 5, 6] as n}<option value={String(n)}>{n} and Other</option>{/each}
      </select>
    </div>
  {:else if topKind && tile.dims[0] && dimTypes[0] !== 'date' && dimTypes[0] !== 'datetime'}
    <div class="sec">
      <span class="label">Top</span>
      <select class="field" value={topChoice} onchange={(e) => setTop(e.currentTarget.value)}>
        <option value="all">All</option>
        {#each TOP_PRESETS as n}<option value={String(n)}>Top {n}</option>{/each}
        <option value="custom">Custom…</option>
        {#if (advanced && numberParams.length) || tile.top?.param}
          <optgroup label="From a parameter">
            {#each numberParams as p (p.id)}<option value="param:{p.id}">Top {p.name}</option>{/each}
            {#if tile.top?.param && !numberParams.some((p) => p.id === tile.top?.param)}<option value="param:{tile.top.param}">A deleted parameter (Top {tile.top.n})</option>{/if}
          </optgroup>
        {/if}
      </select>
      {#if topChoice === 'custom'}
        <input
          class="field"
          type="number"
          min="1"
          max="1000"
          aria-label="How many to keep"
          value={tile.top?.n ?? 15}
          onchange={(e) => setTopN(e.currentTarget.value)} />
      {/if}
      {#if tile.top}
        <label class="check">
          <input type="checkbox" checked={tile.top.other !== false} onchange={(e) => set({ top: { ...tile.top!, other: e.currentTarget.checked ? undefined : false } })} />
          Show the rest as Other
        </label>
      {/if}
    </div>
  {/if}

  {#if (tile.type === 'bar' || tile.type === 'table' || tile.type === 'waterfall' || isFunnel || isBox || isBullet || isGantt || isCompare) && tile.dims[0]}
    <div class="sec">
      <span class="label">Sort</span>
      <select class="field" aria-label="Sort" value={sortId} onchange={(e) => pickSort(e.currentTarget.value)}>
        {#each sortChoices as o}<option value={o.id}>{o.label}</option>{/each}
      </select>
      {#if sortId === 'manual'}
        <!-- Drag a group to a new place, or use the arrows. The order is saved with the tile. -->
        <ul class="order" aria-label="Order of the groups">
          {#each manualList as g, i (g)}
            <li
              class:over={dragFrom !== null && dragFrom !== i}
              draggable="true"
              ondragstart={() => (dragFrom = i)}
              ondragover={(e) => e.preventDefault()}
              ondrop={(e) => {
                e.preventDefault();
                if (dragFrom !== null) moveGroup(dragFrom, i);
                dragFrom = null;
              }}
              ondragend={() => (dragFrom = null)}>
              <Icon name="grip" size={14} />
              <span class="gname" title={g}>{g}</span>
              <button class="btn plain icon" aria-label="Move {g} up" disabled={i === 0} onclick={() => moveGroup(i, i - 1)}><Icon name="chevronDown" size={12} class="flip" /></button>
              <button class="btn plain icon" aria-label="Move {g} down" disabled={i === manualList.length - 1} onclick={() => moveGroup(i, i + 1)}><Icon name="chevronDown" size={12} /></button>
            </li>
          {/each}
        </ul>
        {#if !manualList.length}<p class="hint tight">The groups show here once the tile has data.</p>{/if}
      {/if}
    </div>
  {/if}

  {#if advanced && isBullet}
    <div class="sec adv">
      <span class="label">Advanced</span>
      {@render bandsUi()}
    </div>
  {/if}

  {#if advanced && chartKind && tile.dims[0]}
    <div class="sec adv">
      <span class="label">Advanced</span>

      {#if !repeated && (tile.measures.length > 1 || tile.dims[1])}
        <span class="label sub">Several values</span>
        <div class="grains stack" role="radiogroup" aria-label="How several values share the chart">
          {#each layoutChoices as o}
            <button
              role="radio"
              aria-checked={layoutNow === o.id}
              class:on={layoutNow === o.id}
              onclick={() => set({ layout: o.id === 'together' ? undefined : o.id, ...(o.id === 'together' ? {} : { mix: undefined }) })}>
              {o.label}
            </button>
          {/each}
        </div>
        {#if LAYOUTS.find((l) => l.id === layoutNow)?.hint}
          <p class="hint tight">{LAYOUTS.find((l) => l.id === layoutNow)?.hint}</p>
        {/if}
      {/if}

      {#if !repeated && tile.measures.length > 1 && layoutNow === 'together'}
        <span class="label sub">Bar or line</span>
        {#each tile.measures as mm, i}
          <div class="mixrow">
            <span class="gname" title={measureLabel(mm, app.tables)}>{measureLabel(mm, app.tables)}</span>
            <select
              class="field"
              aria-label="How {measureLabel(mm, app.tables)} is drawn"
              disabled={!sharesUnit}
              value={sharesUnit ? (tile.mix?.[i] ?? (tile.type === 'bar' ? 'bar' : 'line')) : (tile.type === 'bar' ? 'bar' : 'line')}
              onchange={(e) => setMix(i, e.currentTarget.value as 'bar' | 'line')}>
              <option value="bar">Bar</option>
              <option value="line">Line</option>
            </select>
          </div>
        {/each}
        {#if !sharesUnit}
          <p class="hint tight">Different units: use panels. Bars and a line share one axis, so they need values in the same unit.</p>
        {/if}
      {/if}

      <span class="label sub">Small multiples</span>
      <FieldPicker
        mode="dim"
        table={tile.repeat?.table ?? null}
        column={tile.repeat?.column ?? null} via={tile.repeat?.via}
        placeholder="One chart for each…"
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && set({ repeat: defaultDim(app.tables, t, c, v), layout: undefined, mix: undefined })}
        onclear={() => set({ repeat: undefined, ownScale: undefined })} />
      {#if tile.repeat}
        <label class="check">
          <input type="checkbox" checked={!!tile.ownScale} onchange={(e) => set({ ownScale: e.currentTarget.checked ? true : undefined })} />
          Own scale per chart
        </label>
        <p class="hint tight">Up to 12 charts, the biggest first. They share one scale, so the sizes can be compared.{tile.measures.length > 1 ? ' Only the first value is shown.' : ''}</p>
      {/if}

      {@render bandsUi()}
    </div>
  {/if}

  {#if advanced && drillChoices.length}
    <div class="sec adv">
      <span class="label">Drill down</span>
      <select class="field" aria-label="Drill down" value={drillSettingValue(tile.drill)} onchange={(e) => set({ drill: drillSetting(e.currentTarget.value) })}>
        {#each drillChoices as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
      <p class="hint tight">
        A reader switches on drill-down in the tile's corner; a click then opens the level below. Make a hierarchy, such as Region › Store, in the Model view.
      </p>
    </div>
  {/if}

  <Suggestions {tile} {current} {filters} onapply={onchange} />
  {/if}

  <div class="sec">
    <span class="label">Title</span>
    <input
      class="field"
      placeholder={noFields ? 'Optional' : tileTitle({ ...tile, title: '' }, app.tables)}
      value={tile.title}
      oninput={(e) => set({ title: e.currentTarget.value })} />
  </div>

  <div class="foot">
    <button class="btn danger" onclick={onremove}><Icon name="trash" /> Remove tile</button>
  </div>
</aside>

{#if placesOpen}
  <PlaceNames level={mapSpec.level} onclose={() => (placesOpen = false)} />
{/if}

<style>
  .linkbtn { border: 0; background: none; padding: 0; color: var(--accent-text); font: inherit; cursor: pointer; }
  .linkbtn:hover { text-decoration: underline; }
  .insp {
    width: 332px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 12px 16px 16px;
    border-left: 1px solid var(--sep);
    background: var(--surface);
    overflow: auto;
  }
  header { display: flex; align-items: center; justify-content: space-between; }
  .notice {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 8px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--selected);
    color: var(--text);
    font-size: 12px;
  }
  .notice :global(svg) { color: var(--accent-text); margin-top: 1px; }
  .notice p { margin: 0; line-height: 1.4; }
  .notice .btn { grid-column: 2; justify-self: start; height: 24px; font-size: 12px; }
  .sec { display: flex; flex-direction: column; gap: 6px; }
  .btn.flush { align-self: flex-start; padding-left: 0; }
  .types { display: grid; grid-template-columns: repeat(3, 1fr); gap: 6px; }
  .types button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 4px 6px;
    border: 1px solid var(--sep);
    border-radius: 8px;
    background: var(--surface);
    font-size: 11px;
    color: var(--text-2);
  }
  .types button :global(svg) { color: var(--text-2); }
  .types button:hover { background: var(--hover); }
  .types button.on { border-color: var(--accent); background: var(--selected); color: var(--accent-text); }
  .types button.soft:not(.on) { opacity: 0.55; }
  .types button.on :global(svg) { color: var(--accent-text); }
  .better { display: flex; align-items: center; gap: 4px; margin-top: 6px; }
  .better .label { margin-right: 4px; }
  .better button {
    height: 24px;
    padding: 0 10px;
    border: 1px solid var(--sep);
    border-radius: 12px;
    background: none;
    font-size: 12px;
    color: var(--text-2);
  }
  .better button.on { border-color: var(--accent); background: var(--selected); color: var(--accent-text); }
  .check { display: flex; align-items: center; gap: 6px; font-size: 12px; margin-top: 4px; }
  select.field { height: 30px; }
  .grains { display: flex; padding: 2px; border-radius: 8px; background: var(--inset); }
  .grains button {
    flex: 1;
    height: 24px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
    font-size: 11px;
    text-transform: capitalize;
  }
  .grains.parts button { text-transform: none; }
  .grains button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .fmt { display: flex; flex-direction: column; gap: 4px; margin: 6px 0 10px; }
  .fmt .label { font-size: 11px; }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .pair select.field { min-width: 0; }
  .hint { margin: -8px 0 0; font-size: 11px; color: var(--text-2); }
  .per, .showas { display: flex; flex-direction: column; gap: 6px; margin-top: 2px; }
  .per .label, .showas .label { font-size: 11px; }
  .link { align-self: flex-start; border: 0; background: none; padding: 0; font-size: 12px; color: var(--accent-text); cursor: pointer; }
  .link:hover { text-decoration: underline; }
  .hint.tight { margin: 0; line-height: 1.4; }
  .hint.refusal { margin-top: 6px; }
  .label.up { margin-top: 6px; }
  .check.first { margin-top: 0; }
  .grains.stack button { text-transform: none; padding: 0 4px; }
  .foot { margin-top: auto; padding-top: 8px; }
  .cells { display: flex; flex-direction: column; gap: 6px; margin-top: 6px; }
  .cells .label { font-size: 11px; }
  .linerow { display: flex; align-items: center; justify-content: space-between; gap: 6px; font-size: 12px; }
  .linerow .btn.icon { width: 22px; height: 22px; }
  .targetform { display: flex; flex-direction: column; gap: 6px; }
  .targetform .row { display: flex; align-items: center; gap: 12px; }
  .target { display: flex; flex-direction: column; gap: 6px; margin-top: 8px; }
  .withunit { display: flex; align-items: center; gap: 6px; }
  .withunit .field { flex: 1; min-width: 0; }
  .unit { color: var(--text-2); font-size: 12px; }
  .hidden { display: none; }
  .label.sub { margin-top: 8px; font-size: 11px; }
  .sec.adv { padding-top: 12px; border-top: 1px solid var(--sep); }
  .order { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .order li { display: flex; align-items: center; gap: 6px; padding: 2px 4px; border-radius: 6px; background: var(--inset); cursor: grab; font-size: 12px; }
  .order li.over { outline: 1px dashed var(--accent); }
  .order .btn.icon { width: 22px; height: 22px; }
  .order :global(.flip) { transform: rotate(180deg); }
  .gname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mixrow { display: grid; grid-template-columns: 1fr 90px; align-items: center; gap: 8px; font-size: 12px; }
  .error { display: flex; gap: 6px; margin: 0; color: var(--danger); font-size: 12px; }
  .error :global(svg) { flex: none; margin-top: 1px; }
</style>

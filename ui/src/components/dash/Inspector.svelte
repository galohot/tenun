<script lang="ts">
  import { drillOptions, drillSetting, drillSettingValue } from '../../lib/drilldown';
  import { api, DATE_PARTS, isPart, type Agg, type Cell, type CondFormat, type Dim, type DType, type Grain, type MapLevel, type MapSpec, type Measure, type NumFormat, type QueryResult, type ShowAs, type Tile, type TileLine, type TileType } from '../../lib/api';
  import { app, go } from '../../lib/state.svelte';
  import { controlOf, MAX_BUTTONS } from '../../lib/params';
  import { MAX_TIP_VALUES } from '../../lib/tips';
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
  import FormatEditor from '../FormatEditor.svelte';
  import PlaceNames from '../PlaceNames.svelte';
  import Suggestions from './Suggestions.svelte';
  import CardEditor from './CardEditor.svelte';
  import TextEditor from './TextEditor.svelte';
  import { readColors } from '../../lib/chart';
  import { normalisePicture } from '../../lib/image';
  import { MAX_BANDS } from '../../lib/charts/bullet';
  import { t, tn } from '../../lib/i18n';

  let {
    tile,
    current = null,
    filters = [],
    dashGrain = null,
    onchange,
    onremove,
    onclose,
    onduplicate,
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
    /** Duplicates the tile. The button shows only when the dashboard passes it. */
    onduplicate?: () => void;
  } = $props();

  const types: { id: TileType; icon: IconName; label: string }[] = [
    { id: 'bar', icon: 'bar', label: t('Bar') },
    { id: 'line', icon: 'line', label: t('Line') },
    { id: 'area', icon: 'area', label: t('Area') },
    { id: 'share', icon: 'share', label: t('Share') },
    { id: 'pie', icon: 'pie', label: t('Pie') },
    { id: 'donut', icon: 'donut', label: t('Donut') },
    { id: 'kpi', icon: 'kpi', label: t('Number') },
    { id: 'table', icon: 'table', label: t('Table') },
    { id: 'scatter', icon: 'scatter', label: t('Scatter') },
    { id: 'histogram', icon: 'histogram', label: t('Histogram') },
    { id: 'waterfall', icon: 'waterfall', label: t('Waterfall') },
    { id: 'pivot', icon: 'pivot', label: t('Pivot') },
    { id: 'map', icon: 'map', label: t('Map') },
    { id: 'filter', icon: 'filter', label: t('Filter') },
    { id: 'text', icon: 'text', label: t('Text') },
    { id: 'image', icon: 'image', label: t('Image') },
  ];
  // The second group of the picker, shown while Advanced is on (build 12).
  const moreAll = ([
    { id: 'heatmap', icon: 'heatmap', label: t('Heat map') },
    { id: 'funnel', icon: 'funnel', label: t('Funnel') },
    { id: 'box', icon: 'box', label: t('Box plot') },
    { id: 'bullet', icon: 'bullet', label: t('Bullet') },
    { id: 'compare', icon: 'compare', label: t('Compare') },
    { id: 'sankey', icon: 'sankey', label: t('Sankey') },
    { id: 'calendar', icon: 'calendarheat', label: t('Calendar') },
    { id: 'cohort', icon: 'cohort', label: t('Cohort') },
    { id: 'gantt', icon: 'gantt', label: t('Gantt') },
  ] as { id: TileType; icon: IconName; label: string }[]).filter((t) => MORE_CHARTS.includes(t.id));
  // A Gantt chart needs a start and an end, so it is offered when some table has two date columns.
  const ganttOffered = $derived(hasTwoDates(app.tables));
  const moreTypes = $derived(moreAll.filter((t) => t.id !== 'gantt' || ganttOffered || tile.type === 'gantt'));
  const stacks: { id: NonNullable<Tile['stack']>; label: string }[] = [
    { id: 'stacked', label: t('Stacked') },
    { id: 'side', label: t('Side by side') },
    { id: 'percent', label: t('100 %') },
  ];
  const grains: Grain[] = ['year', 'quarter', 'month', 'week', 'day'];
  const partNames: Record<string, string> = { month_of_year: t('Month of year'), weekday: t('Weekday'), hour: t('Hour') };
  const grainNames: Record<string, string> = { year: t('year'), quarter: t('quarter'), month: t('month'), week: t('week'), day: t('day') };

  const TOP_PRESETS = [5, 10, 20];
  const sorts: { id: string; label: string; sort?: Tile['sort'] }[] = [
    { id: 'auto', label: t('Automatic') },
    { id: 'value-desc', label: t('Biggest first'), sort: { by: 'value', desc: true } },
    { id: 'value-asc', label: t('Smallest first'), sort: { by: 'value', desc: false } },
    { id: 'label-asc', label: t('A → Z'), sort: { by: 'label', desc: false } },
    { id: 'label-desc', label: t('Z → A'), sort: { by: 'label', desc: true } },
  ];
  // A funnel starts with the biggest stage, and a box plot with A → Z: that is its "Automatic".
  const funnelSorts: typeof sorts = [
    { id: 'auto', label: t('Biggest first') },
    { id: 'value-asc', label: t('Smallest first'), sort: { by: 'value', desc: false } },
    { id: 'label-asc', label: t('A → Z'), sort: { by: 'label', desc: false } },
    { id: 'label-desc', label: t('Z → A'), sort: { by: 'label', desc: true } },
  ];
  const ganttSorts: typeof sorts = [
    { id: 'auto', label: t('Earliest start first') },
    { id: 'value-desc', label: t('Latest start first'), sort: { by: 'value', desc: true } },
    { id: 'label-asc', label: t('A → Z'), sort: { by: 'label', desc: false } },
    { id: 'label-desc', label: t('Z → A'), sort: { by: 'label', desc: true } },
  ];
  const compareSorts: typeof sorts = [
    { id: 'auto', label: t('Biggest gap first') },
    { id: 'value-desc', label: t('Highest first value'), sort: { by: 'value', desc: true } },
    { id: 'value-asc', label: t('Lowest first value'), sort: { by: 'value', desc: false } },
    { id: 'label-asc', label: t('A → Z'), sort: { by: 'label', desc: false } },
    { id: 'label-desc', label: t('Z → A'), sort: { by: 'label', desc: true } },
  ];
  const boxSorts: typeof sorts = [
    { id: 'auto', label: t('A → Z') },
    { id: 'label-desc', label: t('Z → A'), sort: { by: 'label', desc: true } },
    { id: 'value-desc', label: t('Highest median first'), sort: { by: 'value', desc: true } },
    { id: 'value-asc', label: t('Lowest median first'), sort: { by: 'value', desc: false } },
  ];

  // How a value is written. Each value has its own (V3-D9). Unset fields keep the defaults.
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
  const hasDate = (table: string) => !!app.table(table)?.columns.some((c) => c.dtype === 'date' || c.dtype === 'datetime');
  /** The ways a value can be added up: a number column can be summed, any other column only counted. */
  const aggsOf = (x: Measure | undefined): Agg[] =>
    !x || x.column === null
      ? ['count']
      : dtypeOf(app.tables, x.table, x.column) === 'number'
        ? [
            'sum', 'avg', 'min', 'max', 'median', 'p90',
            // Spread is Advanced; a value that already has it keeps it in the list.
            ...(advanced || x.agg === 'stdev' || x.agg === 'variance' ? (['stdev', 'variance'] as Agg[]) : []),
            // A stock (v5 build 07, Advanced): the table needs a date to know each period's last day.
            ...((advanced && hasDate(x.table)) || x.agg === 'closing' || x.agg === 'opening' ? (['closing', 'opening'] as Agg[]) : []),
            'count', 'count_distinct',
          ]
        : ['count', 'count_distinct', 'min', 'max'];
  // A Sankey chart adds links together, so it takes a sum or a count.
  const aggs = $derived(isSankey ? aggsOf(m).filter((a) => a === 'sum' || a === 'count') : aggsOf(m));

  function set(patch: Partial<Tile>) {
    apply({ ...tile, ...patch });
  }

  // Undo in the panel's footer: the tile as it was before each change made here. Changes that come
  // quickly one after another (typing a title) are one step. The tile keeps its place on the page.
  let history = $state<Tile[]>([]);
  let lastStep = 0;
  function apply(next: Tile) {
    const now = Date.now();
    if (now - lastStep > 700 || !history.length) history = [...history.slice(-49), $state.snapshot(tile) as Tile];
    lastStep = now;
    onchange(next);
  }
  function undoLast() {
    const prev = history.at(-1);
    if (!prev) return;
    history = history.slice(0, -1);
    lastStep = 0;
    onchange({ ...prev, x: tile.x, y: tile.y, w: tile.w, h: tile.h });
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
      history = [];
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
    ...(advanced || tile.sort?.by === 'manual' ? [{ id: 'manual', label: t('Manual order') }] : []),
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
        ? [['slider', t('Slider')], ['input', t('Number box')]]
        : (paramDef.choices ?? []).length <= MAX_BUTTONS
          ? [['buttons', t('Buttons')], ['dropdown', t('Dropdown')]]
          : [['dropdown', t('Dropdown')]],
  );
  const paramName = (id: string | undefined) => allParams.find((p) => p.id === id)?.name ?? t('a deleted parameter');
  /** A target that takes a parameter: its value is the parameter's now (the tile keeps it too, for a file without it). */
  function setTargetParam(id: string) {
    const p = numberParams.find((x) => x.id === id);
    if (!p) return set({ target: tile.target && tile.target.value > 0 ? { value: tile.target.value } : undefined });
    set({ target: { value: typeof p.value === 'number' ? p.value : (tile.target?.value ?? 0), param: p.id } });
  }
  const filterStyles: { id: NonNullable<Tile['filter']>['style']; label: string; types: DType[] | null }[] = [
    { id: 'list', label: t('List'), types: null },
    { id: 'dropdown', label: t('Dropdown'), types: null },
    { id: 'range', label: t('Range'), types: ['number'] },
    { id: 'dates', label: t('Dates'), types: ['date', 'datetime'] },
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
    { id: 'target', label: t('Target…') },
    { id: 'average', label: t('Average') },
    { id: 'median', label: t('Median') },
    { id: 'min', label: t('Minimum') },
    { id: 'max', label: t('Maximum') },
    { id: 'trend', label: t('Trend') },
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
    const name = l.label?.trim() || LINE_MENU.find((o) => o.id === l.kind)?.label.replace('…', '') || l.kind;
    const line =
      l.kind !== 'target' ? name
      : l.param ? t('{name} from {param}', { name, param: paramName(l.param) })
      : l.value === undefined ? name
      : `${name} ${cell(l.value, 'number', null, false, formatOf(tile, 0))}`;
    if (!isScatter || l.kind === 'trend') return line;
    return l.axis === 'x' ? t('{line} of X (across)', { line }) : l.axis === 'y' || l.kind !== 'median' ? t('{line} of Y (up)', { line }) : t('{line} of X and Y', { line });
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
  // The panel's tabs. "On click" shows only when a click on this tile has something to set.
  type Tab = 'data' | 'style' | 'click';
  let tab = $state<Tab>('data');
  const otherPages = $derived((app.snap?.workspace.dashboards ?? []).filter((d) => !d.tiles.some((t) => t.id === tile.id)));
  /** The tile's marks can be clicked: the same test as the tile's menu (a group, or a bin of a histogram). */
  const clickable = $derived(tile.type === 'histogram' ? tile.measures.length > 0 : tile.type !== 'kpi' && tile.type !== 'cohort' && tile.type !== 'filter' && tile.dims.length > 0);
  const actionOffered = $derived(!noFields && !lockedType && clickable && (otherPages.length > 0 || !!tile.action));
  const reachShown = $derived((advanced || tile.reach !== undefined) && (clickable || isFilter));
  const clickTab = $derived(actionOffered || reachShown || (advanced && !noFields && !lockedType && drillChoices.length > 0));
  const tabList = $derived<{ id: Tab; label: string }[]>([
    { id: 'data', label: t('Data') },
    { id: 'style', label: t('Style') },
    ...(clickTab ? [{ id: 'click' as Tab, label: t('On click') }] : []),
  ]);
  const tabNow = $derived<Tab>(tab === 'click' && !clickTab ? 'data' : tab);
  const markWord = $derived(tile.type === 'line' || tile.type === 'area' ? t('a point') : tile.type === 'pie' || tile.type === 'donut' ? t('a slice') : tile.type === 'table' || tile.type === 'pivot' ? t('a row') : tile.type === 'map' ? t('a place') : tile.type === 'bar' || tile.type === 'histogram' || tile.type === 'waterfall' ? t('a bar') : t('a mark'));
  /** What a click on a mark does: filter this page, or open another page with only the clicked value (build 12). */
  function setAction(page: string | null) {
    const { action: _old, ...rest } = tile;
    apply(page ? { ...rest, action: { page } } : rest);
  }
  const headTitle = $derived(tileTitle(tile, app.tables) || t('Untitled tile'));
  const typeLabel = $derived([...types, ...moreAll].find((t) => t.id === tile.type)?.label ?? '');
  // "More charts" opens by itself when the tile is one of them.
  let moreShown = $state<boolean | null>(null);
  const moreOpen = $derived(moreShown ?? moreTypes.some((t) => t.id === tile.type));

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
    { id: 'together', label: t('Together'), hint: '' },
    { id: 'panels', label: t('Separate panels'), hint: t('Each value gets a plot and an axis of its own. One hover line runs through all of them.') },
    { id: 'indexed', label: t('Indexed'), hint: t('Every line starts at 100, so you compare how fast each one grows.') },
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
  // A forecast (v5 build 08): a line or an area over a date with a grain, one value.
  const canForecast = $derived((tile.type === 'line' || tile.type === 'area') && dim0 === 'date' && !!tile.dims[0]?.grain && !isPart(tile.dims[0].grain) && tile.measures.length === 1);
  const FORECAST_AHEAD: Record<string, number> = { month: 3, quarter: 2, week: 8, day: 14, year: 2 };
  // More in the tooltip (v5 build 10): charts whose marks are groups.
  const canTip = $derived(['bar', 'line', 'area', 'pie', 'donut', 'scatter'].includes(tile.type) && !!tile.dims[0] && !!m);
  const tipExtra = $derived(tile.tooltip?.extra ?? []);
  function setTip(i: number, x: Measure | null) {
    const list = [...tipExtra];
    if (x) list[i] = x;
    else list.splice(i, 1);
    const next = { ...tile.tooltip, extra: list.slice(0, MAX_TIP_VALUES) };
    if (!next.extra.length) delete (next as { extra?: Measure[] }).extra;
    set({ tooltip: next.extra || next.trend ? next : undefined });
  }
  /** The date columns a trend can run over: those of the tables the tile's value reaches. */
  const tipDates = $derived(
    app.tables
      .filter((t) => !reach || reach.has(t.id) || t.id === baseTable)
      .flatMap((t) =>
        t.columns
          .filter((c) => !c.hidden && (c.dtype === 'date' || c.dtype === 'datetime'))
          .map((c) => ({ value: `${t.id}\u0000${c.name}`, label: `${t.label} · ${c.label ?? c.name}` })),
      ),
  );
  function setTipTrend(value: string) {
    const [table, column] = value.split('\u0000');
    const trend = value ? { table, column, grain: tile.tooltip?.trend?.grain ?? ('month' as Grain) } : undefined;
    const next = { ...tile.tooltip, trend };
    if (!trend) delete (next as { trend?: Dim }).trend;
    set({ tooltip: next.extra?.length || next.trend ? next : undefined });
  }
  // Unusual values (v5 build 09): a line, an area or bars over a date with a grain.
  const canMarkUnusual = $derived((tile.type === 'line' || tile.type === 'area' || tile.type === 'bar') && dim0 === 'date' && !!tile.dims[0]?.grain && !isPart(tile.dims[0].grain));

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
      r.onerror = () => reject(new Error(t('That file could not be read.')));
      r.readAsDataURL(blob);
    });
  }

  /** Checks the size and the kind in words, then keeps the picture in the tile. */
  async function takePicture(blob: Blob) {
    imageError = '';
    if (blob.size > MAX_IMAGE) {
      imageError = t('That picture is too big. Pick one under 1 MB.');
      return;
    }
    // The first bytes decide, not the file name: a name that ends in .png can hold anything.
    const data = normalisePicture(await readBlob(blob));
    if (!data) {
      imageError = t('Pick a PNG, JPEG or WebP picture.');
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
  <div class="fmt">
  <FormatEditor
    value={formatOf(tile, i)}
    onchange={(patch) => setFormat(i, patch)}
    shortDefault={!tableLike}
    blank={tableLike}
    advanced={!!app.settings?.advanced}
    label={t('Format of this value')} />
  </div>
{/snippet}

{#snippet valueRows(i: number)}
  {@const x = tile.measures[i]}
  {#if x && !numberOnly}
    {#if x.named}
      <!-- A named measure divides inside its formula. -->
    {:else if x.per}
      <div class="per">
        <span class="label">{t('Divide by')}</span>
        <FieldPicker
          mode="measure"
          table={x.per.table}
          column={x.per.column} via={x.per.via}
          reachable={reach} base={baseTable}
          onchange={(t, c, v) => setPer(i, defaultMeasure(app.tables, t, c, v))}
          onclear={() => setPer(i, null)} />
        <select
          class="field"
          aria-label={t('How to add up the value you divide by')}
          value={x.per.agg}
          onchange={(e) => setPer(i, { ...x.per!, agg: e.currentTarget.value as Agg })}>
          {#each aggsOf({ ...x.per, per: undefined }) as a}<option value={a}>{aggNames[a]}</option>{/each}
        </select>
      </div>
    {:else if dividing === i}
      <div class="per">
        <span class="label">{t('Divide by')}</span>
        <FieldPicker mode="measure" table={null} column={null} placeholder={t('What to divide by')} reachable={reach} base={baseTable} onchange={(t, c, v) => setPer(i, defaultMeasure(app.tables, t, c, v))} />
        <button class="link" onclick={() => (dividing = null)}>{t('Cancel')}</button>
      </div>
    {:else}
      <button class="link" onclick={() => (dividing = i)}>{t('Divide by…')}</button>
    {/if}
    {#if showAsFor(i).length}
      {@const saved = tile.showAs?.[i] ?? 'value'}
      {@const choices = showAsFor(i)}
      {@const why = whyNotAdded(tile, i, parts)}
      <div class="showas">
        <span class="label">{t('Show as')}</span>
        <select class="field" aria-label={t('Show this value as')} value={saved} onchange={(e) => setShowAs(i, e.currentTarget.value)}>
          {#each choices as o}<option value={o}>{SHOW_AS_LABELS[o]}</option>{/each}
          {#if !choices.includes(saved)}<option value={saved} disabled>{t('{name} (not available here)', { name: SHOW_AS_LABELS[saved] })}</option>{/if}
        </select>
      </div>
      {#if why}<p class="hint tight">{t('{why}, so there is no % of total, running total or year to date.', { why })}</p>{/if}
    {/if}
  {/if}
  {#if tableLike && x}
    {@const cf = tile.cond?.[i] ?? null}
    {@const how = effectiveShowAs(tile, i, dim0, parts)}
    <div class="cells">
      <span class="label">{t('Cells')}</span>
      <div class="grains stack" role="radiogroup" aria-label={t('How the cells of this value look')}>
        {#each [[null, t('Plain')], ['bars', t('Bars')], ['goodbad', t('Good-bad')]] as [id, label]}
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
          <p class="hint tight">{t('A change is good above 0 and bad below it.')}</p>
        {:else}
          <input
            class="field"
            type="number"
            step="any"
            placeholder={t('Good above… (0)')}
            aria-label={t('The value that splits good from bad')}
            value={cf.threshold ?? ''}
            onchange={(e) => setThreshold(i, e.currentTarget.value)} />
        {/if}
        <div class="better" role="radiogroup" aria-label={t('Which direction is good')}>
          <span class="label">{t('A higher number is')}</span>
          <button role="radio" aria-checked={(cf.better ?? 'up') === 'up'} class:on={(cf.better ?? 'up') === 'up'} onclick={() => setCond(i, { ...cf, better: undefined })}>{t('good')}</button>
          <button role="radio" aria-checked={cf.better === 'down'} class:on={cf.better === 'down'} onclick={() => setCond(i, { ...cf, better: 'down' })}>{t('bad')}</button>
        </div>
      {/if}
    </div>
  {/if}
  {@render formatRow(i)}
{/snippet}

{#snippet bandsUi()}
      <span class="label sub">{t('Bands')}</span>
      {#each bands as b}
        <div class="linerow">
          <span>{t('{name} {from} to {to}', { name: b.label?.trim() || t('Band'), from: cell(b.value ?? 0, 'number', null, false, formatOf(tile, 0)), to: cell(b.to ?? 0, 'number', null, false, formatOf(tile, 0)) })}</span>
          <button class="btn plain icon" aria-label={t('Remove band')} onclick={() => set({ lines: (tile.lines ?? []).filter((l) => l !== b) })}><Icon name="x" size={12} /></button>
        </div>
      {/each}
      {#if askBand}
        <div class="targetform">
          <input class="field" type="number" step="any" placeholder={t('From')} aria-label={t('Band from')} bind:value={bandFrom} />
          <input class="field" type="number" step="any" placeholder={t('To')} aria-label={t('Band to')} bind:value={bandTo} />
          <input class="field" placeholder={t('Name (optional)')} aria-label={t('Name of the band')} bind:value={bandLabel} />
          <div class="row">
            <button class="btn small" disabled={bandFrom.trim() === '' || bandTo.trim() === ''} onclick={addBand}>{t('Add band')}</button>
            <button class="link" onclick={() => (askBand = false)}>{t('Cancel')}</button>
          </div>
        </div>
      {:else if bands.length < bandLimit}
        <button class="link" onclick={() => (askBand = true)}>{t('Add band…')}</button>
      {/if}
{/snippet}

<aside class="insp" aria-label={t('Edit tile')}>
  <header class="ihead">
    <div class="who">
      <span class="eyebrow">{t('Editing tile')}</span>
      <span class="name" title={headTitle}>{headTitle}</span>
    </div>
    {#if onduplicate}
      <button class="btn plain icon" title={t('Duplicate tile')} aria-label={t('Duplicate tile')} onclick={onduplicate}>
        <svg width="16" height="16" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6.5 6.5h10v10h-10zM3.5 13.5v-10h10" /></svg>
      </button>
    {/if}
    <button class="btn plain icon" title={t('Remove tile')} aria-label={t('Remove tile')} onclick={onremove}><Icon name="trash" size={15} /></button>
    <button class="btn plain icon" title={t('Close panel')} aria-label={t('Close panel')} onclick={onclose}><Icon name="x" /></button>
  </header>
  <div class="tabs" role="tablist" aria-label={t('Tile settings')}>
    {#each tabList as t (t.id)}
      <button
        role="tab"
        id="insp-tab-{t.id}"
        aria-selected={tabNow === t.id}
        aria-controls="insp-panel"
        class:on={tabNow === t.id}
        onclick={() => (tab = t.id)}>{t.label}</button>
    {/each}
  </div>

  <div class="body" id="insp-panel" role="tabpanel" aria-labelledby="insp-tab-{tabNow}">
  {#if advancedLocked(tile)}
    <!-- V3-D2: the tile still draws; only editing its Advanced parts waits for the switch. -->
    <div class="notice" role="note">
      <Icon name="info" size={14} />
      <p>{t('This tile uses Advanced features. Turn them on in Settings to edit them.')}</p>
      {#if SHOW_ADVANCED_SWITCH}
        <button class="btn small" onclick={() => (app.settingsOpen = true)}>{t('Open Settings')}</button>
      {/if}
    </div>
  {/if}

  {#if tabNow === 'data'}
  {#if !isParam}
  <div class="sec">
    <div class="sh"><span>{t('Chart')}</span><span class="now">{typeLabel}</span></div>
    <div class="types" role="radiogroup" aria-label={t('Tile type')}>
      {#each types as t}
        <button
          role="radio"
          aria-checked={tile.type === t.id}
          aria-label={t.label}
          class:on={tile.type === t.id}
          class:soft={!!chipWhy(t.id)}
          title={chipWhy(t.id) ?? t.label}
          onclick={() => pickType(t.id)}>
          <Icon name={t.icon} size={18} />
        </button>
      {/each}
    </div>
    {#if refusedType}<p class="hint tight refusal" role="note">{refusedType}</p>{/if}
    {#if advanced && moreTypes.length}
      <button class="disclose" aria-expanded={moreOpen} onclick={() => (moreShown = !moreOpen)}>
        <span class="chev" class:open={moreOpen}><Icon name="chevronDown" size={12} /></span>
        {t('More charts (Advanced)')}
      </button>
      {#if moreOpen}
        <div class="types" role="radiogroup" aria-label={t('More chart types')}>
          {#each moreTypes as t}
            <button role="radio" aria-checked={tile.type === t.id} aria-label={t.label} class:on={tile.type === t.id} class:soft={!!chipWhy(t.id)} title={chipWhy(t.id) ?? t.label} onclick={() => pickType(t.id)}>
              <Icon name={t.icon} size={18} />
            </button>
          {/each}
        </div>
      {/if}
    {/if}
    {#if tile.type === 'bar'}
      <label class="check">
        <input type="checkbox" checked={!!tile.horizontal} onchange={(e) => set({ horizontal: e.currentTarget.checked })} />
        {t('Horizontal bars')}
      </label>
    {/if}
  </div>
  <div class="rule" role="presentation"></div>
  {/if}

  {#if isText}
    <TextEditor {tile} {set} />
  {/if}

  {#if isParam}
    <!-- A parameter's control (v5 build 05): which parameter it moves, and how it looks. -->
    <div class="sec">
      <span class="label">{t('Parameter')}</span>
      {#if allParams.length}
        <select class="field" aria-label={t('The parameter this tile moves')} value={tile.param?.id ?? ''} onchange={(e) => set({ param: { ...tile.param, id: e.currentTarget.value } })}>
          {#if !paramDef}<option value="">{t('Pick a parameter')}</option>{/if}
          {#each allParams as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
        {#if paramDef}
          <div class="grains stack" role="radiogroup" aria-label={t('Control')}>
            {#each controlChoices as [id, label] (id)}
              <button role="radio" aria-checked={paramControl === id} class:on={paramControl === id} onclick={() => set({ param: { ...tile.param!, style: id } })}>{label}</button>
            {/each}
          </div>
          <p class="hint tight">
            {paramDef.description?.trim() || t('A measure reads it as PARAM("{name}").', { name: paramDef.name })} {t('Change its range or its list in Data, under Parameters.')}
          </p>
        {/if}
      {:else}
        <p class="hint tight">{t('No parameters yet. Make one in Data, under Parameters: a number on a slider, or a choice from a list.')}</p>
        <button class="btn small" onclick={() => go({ view: 'data' })}>{t('Open Data')}</button>
      {/if}
    </div>
  {/if}

  {#if isImage}
    <div class="sec">
      <span class="label">{t('Picture')}</span>
      <button class="btn" onclick={choosePicture}><Icon name="image" /> {tile.image?.data ? t('Change picture') : t('Choose a picture')}</button>
      <input bind:this={fileInput} class="hidden" type="file" accept="image/png,image/jpeg,image/webp" onchange={onFile} />
      <p class="hint tight">{t('PNG, JPEG or WebP, up to 1 MB.')}</p>
      {#if imageError}<p class="error" role="alert"><Icon name="warning" size={13} /> {imageError}</p>{/if}
      {#if tile.image?.data}
        <span class="label up">{t('Description')}</span>
        <input
          class="field"
          placeholder={t('What the picture shows')}
          aria-label={t('Description of the picture, for screen readers')}
          value={tile.image.alt}
          oninput={(e) => set({ image: { ...tile.image!, alt: e.currentTarget.value } })} />
        <div class="grains stack" role="radiogroup" aria-label={t('How the picture fits')}>
          {#each [['contain', t('Show all')], ['cover', t('Fill the tile')]] as [id, label]}
            <button
              role="radio"
              aria-checked={(tile.image.fit ?? 'contain') === id}
              class:on={(tile.image.fit ?? 'contain') === id}
              onclick={() => set({ image: { ...tile.image!, fit: id as 'contain' | 'cover' } })}>
              {label}
            </button>
          {/each}
        </div>
        <button class="btn plain flush" onclick={() => set({ image: undefined })}><Icon name="trash" /> {t('Remove picture')}</button>
      {/if}
    </div>
  {/if}

  {#if isFilter}
    <div class="sec">
      <span class="label">{t('Field to filter by')}</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[0]?.table ?? null}
        column={tile.dims[0]?.column ?? null} via={tile.dims[0]?.via}
        placeholder={t('Pick a field')}
        roles
        onchange={(t, c, v) => setFilterField(t, c, v)}
        onclear={() => set({ dims: [], filter: { style: tile.filter?.style ?? 'list' } })} />
      {#if tile.dims[0]}
        <div class="grains stack" role="radiogroup" aria-label={t('Filter style')}>
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
            {t('Pick one value at most')}
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
              })}>{t('Clear the selection')}</button>
        {/if}
        {#if advanced || tile.filter?.everyPage}
          <label class="check">
            <input
              type="checkbox"
              checked={!!tile.filter?.everyPage}
              disabled={!advanced}
              onchange={(e) => set({ filter: { ...tile.filter!, style: tile.filter?.style ?? 'list', everyPage: e.currentTarget.checked || undefined } })} />
            {t('Apply to every page')}
          </label>
        {/if}
        <p class="hint tight">
          {tile.filter?.everyPage
            ? t('The selection is saved with the dashboard. It filters every other tile, on every page of the workspace, in exported and published files too.')
            : t('The selection is saved with the dashboard. It filters every other tile, in exported and published files too.')}
        </p>
      {/if}
    </div>
  {/if}

  {#if !noFields && !lockedType}
  <div class="sec">
    <span class="label">{isScatter ? t('Across (X)') : isGantt ? t('Start date') : isCohort ? t('Who to follow') : isCompare ? t('First value') : isBullet ? t('Value') : numberOnly ? t('Number column') : t('Value')}</span>
    <FieldPicker
      mode="measure"
      table={m?.table ?? null}
      column={m?.column ?? null} via={m?.via}
      placeholder={isGantt ? t('The column the work starts in') : isCohort ? t('A column like customer_id') : numberOnly ? t('A column of numbers') : t('What to measure')}
      numbersOnly={numberOnly}
      datesOnly={isGantt}
      {...namedProps(0, !numberOnly && !plainValue)}
      onchange={(t, c, v) => (isGantt ? setDateValue(0, t, c, v) : isCohort ? setEntity(t, c, v) : swapField(0, t, c, v))} />
    {#if m && isCohort}
      <p class="hint tight">{t('Each different value in this column is one customer, user or item. It joins the cohort of the first period it appears in.')}</p>
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
      <span class="label up">{t('Up (Y)')}</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder={t('What to measure')}
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
        <span class="label up">{t('Size (optional)')}</span>
        <FieldPicker
          mode="measure"
          table={tile.measures[2]?.table ?? null}
          column={tile.measures[2]?.column ?? null} via={tile.measures[2]?.via}
          placeholder={t('Make the dots bigger for more')}
          reachable={reach} base={baseTable}
          {...namedProps(2)}
          onchange={(t, c, v) => set({ measures: [m, tile.measures[1], defaultMeasure(app.tables, t, c, v)] })}
          onclear={() => removeMeasure(2)} />
        {#if tile.measures[2]}
          {#if !tile.measures[2].named}
          <select class="field" aria-label={t('How to add up the size')} value={tile.measures[2].agg} onchange={(e) => set({ measures: [m, tile.measures[1], { ...tile.measures[2], agg: e.currentTarget.value as Agg }] })}>
            {#each aggsOf(tile.measures[2]) as a}<option value={a}>{aggNames[a]}</option>{/each}
          </select>
          {/if}
          {@render valueRows(2)}
        {/if}
      {/if}
    {/if}
    {#if isBullet && m}
      <span class="label up">{t('Target (optional)')}</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder={t('A value that holds the target')}
        reachable={reach} base={baseTable}
        {...namedProps(1)}
        onchange={(t, c, v) => set({ measures: [m, defaultMeasure(app.tables, t, c, v)], target: undefined })}
        onclear={() => removeMeasure(1)} />
      {#if tile.measures[1]}
        {#if !tile.measures[1].named}
        <select class="field" aria-label={t('How to add up the target')} value={tile.measures[1].agg} onchange={(e) => set({ measures: [m, { ...tile.measures[1], agg: e.currentTarget.value as Agg }] })}>
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
          placeholder={t('Or type one target for every bar')}
          aria-label={unit0.unit ? t('One target for every bar in {unit}', { unit: unit0.unit }) : t('One target for every bar')}
          value={typed(tile.target?.value, unit0.scale)}
          onchange={(e) => setTarget(e.currentTarget.value)} />
        {#if unit0.unit}<span class="unit" aria-hidden="true">{unit0.unit}</span>{/if}
        </span>
      {/if}
      <p class="hint tight">{t('The thin bar is the value and the tick is the target. Add bands below for poor, fair and good.')}</p>
    {/if}
    {#if isCompare && m}
      <span class="label up">{t('Second value (optional)')}</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder={t('A second value, or compare two periods')}
        reachable={reach} base={baseTable}
        {...namedProps(1)}
        onchange={(t, c, v) => set({ measures: [m, defaultMeasure(app.tables, t, c, v)] })}
        onclear={() => removeMeasure(1)} />
      {#if tile.measures[1]}
        {#if !tile.measures[1].named}
        <select class="field" aria-label={t('How to add up the second value')} value={tile.measures[1].agg} onchange={(e) => set({ measures: [m, { ...tile.measures[1], agg: e.currentTarget.value as Agg }] })}>
          {#each aggsOf(tile.measures[1]) as a}<option value={a}>{aggNames[a]}</option>{/each}
        </select>
        {/if}
        <p class="hint tight">{t('Two dots for each row, joined by a line. The biggest gap comes first.')}</p>
      {:else}
        <p class="hint tight">{t('With one value, pick a date below and two of its periods. Each row then draws as a line from the first to the second.')}</p>
      {/if}
    {/if}
    {#if isGantt && m}
      <span class="label up">{t('End date')}</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder={t('The column the work ends in')}
        datesOnly
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => setDateValue(1, t, c, v)}
        onclear={() => removeMeasure(1)} />
      <p class="hint tight">{t('Each row is a bar from its earliest start to its latest end. The first 100 rows are drawn.')}</p>
    {/if}
    {#if isCohort && m}
      <span class="label up">{t('Add up a value instead (optional)')}</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null} via={tile.measures[1]?.via}
        placeholder={t('Show a total, not a share')}
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
          placeholder={t('Add another value')}
          reachable={reach} base={baseTable}
          measures={namedList}
          onnamed={(id) => set({ measures: [...tile.measures, namedMeasure(id)] })}
          onchange={(t, c, v) => set({ measures: [...tile.measures, defaultMeasure(app.tables, t, c, v)] })} />
      {/if}
    {/if}
  </div>

  {#if !isHistogram}
  <div class="sec">
      <span class="label">{isMap ? (mapPoints ? t('Colour (optional)') : t('Place (names or codes)')) : tile.type === 'kpi' ? t('Period (a date, for the comparison)') : tile.type === 'share' || tile.type === 'donut' || tile.type === 'pie' ? t('Split into') : isScatter ? t('Dot for each') : pivotLike ? t('Rows') : isFunnel ? t('Stages') : isBox || isBullet ? t('Group by (optional)') : isGantt ? t('Name of each row') : isCalendar ? t('Date (one square for each day)') : isCohort ? t('Date of each event') : isSankey ? t('First stage') : isCompare ? t('Row for each') : t('Group by')}</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[0]?.table ?? null}
        column={tile.dims[0]?.column ?? null} via={tile.dims[0]?.via}
        placeholder={isMap ? (mapPoints ? t('Colour points by, like a kind') : t('A column of places, like province')) : tile.type === 'kpi' ? t('A date, for the trend') : isScatter ? t('One dot for each of these') : isFunnel ? t('The steps, like visit and sale') : isBox ? t('One box for each of these') : isBullet ? t('One bar for each of these') : isGantt ? t('A task, a project or an id') : isCalendar || isCohort ? t('A date column') : isSankey ? t('Where it starts, like a channel') : t('Category or date')}
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && setDim(0, t, c, v)}
        onclear={() => removeDim(0)} />
      {#if tile.dims[0] && (dimTypes[0] === 'date' || dimTypes[0] === 'datetime') && !isCalendar && !isGantt}
        <div class="grains" role="radiogroup" aria-label={t('Date grouping')}>
          {#each grains as g}
            <button role="radio" aria-checked={tile.dims[0].grain === g} class:on={tile.dims[0].grain === g} onclick={() => set({ dims: [{ ...tile.dims[0], grain: g }, ...tile.dims.slice(1)] })}>
              {grainNames[g]}
            </button>
          {/each}
        </div>
        {#if dashGrain && isPeriod(tile.dims[0].grain)}
          <p class="hint tight">{t('The dashboard shows every chart over time by {grain}. Choose Auto in the toolbar to use this grouping.', { grain: grainNames[dashGrain] ?? dashGrain })}</p>
        {/if}
        <!-- Parts of the date repeat every year, week or day. A number tile compares periods, so it has none. -->
        {#if tile.type !== 'kpi' && !isCohort}
          <div class="grains parts" role="radiogroup" aria-label={t('Part of the date')}>
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

  {#if isMap}
    <div class="sec">
      <span class="label">{t('Map')}</span>
      <select class="field" aria-label={t('Places on the map')} value={mapSpec.level} onchange={(e) => setMap({ level: e.currentTarget.value as MapLevel })}>
        <option value="country">{t('Countries of the world')}</option>
        <option value="province">{t('Provinces')}</option>
        <option value="regency">{t('Kabupaten and kota')}</option>
      </select>
      {#if mapSpec.level !== 'country' || mapPoints}
        <select class="field" aria-label={t('Country')} value={mapSpec.country ?? 'ID'} onchange={(e) => setMap({ country: e.currentTarget.value })}>
          {#if !countries.length}<option value={mapSpec.country ?? 'ID'}>{mapSpec.country ?? 'ID'}</option>{/if}
          {#each countries as c (c.code)}<option value={c.code}>{c.name}</option>{/each}
        </select>
        {#if mapSpec.level !== 'country' && (mapSpec.country ?? 'ID').toUpperCase() !== 'ID'}
          <p class="hint tight">{t('Tenun downloads this country\'s map once, from geoBoundaries, the first time the tile draws it.')}</p>
        {/if}
      {/if}
      <div class="grains" role="radiogroup" aria-label={t('How the map shows the value')}>
        <button role="radio" aria-checked={!mapPoints} class:on={!mapPoints} onclick={() => setMap({ style: 'filled' })}>{t('Filled')}</button>
        <button role="radio" aria-checked={mapPoints} class:on={mapPoints} onclick={() => setMap({ style: 'points' })}>{t('Points')}</button>
      </div>
      {#if mapPoints}
        <span class="label up">{t('Latitude')}</span>
        <FieldPicker mode="dim" table={mapSpec.lat?.table ?? null} column={mapSpec.lat?.column ?? null} via={mapSpec.lat?.via} placeholder={t('A column of latitudes')} reachable={reach} base={baseTable} onchange={(t, c, v) => c && setMap({ lat: { table: t, column: c, ...(v ? { via: v } : {}) } })} onclear={() => setMap({ lat: undefined })} />
        <span class="label up">{t('Longitude')}</span>
        <FieldPicker mode="dim" table={mapSpec.lon?.table ?? null} column={mapSpec.lon?.column ?? null} via={mapSpec.lon?.via} placeholder={t('A column of longitudes')} reachable={reach} base={baseTable} onchange={(t, c, v) => c && setMap({ lon: { table: t, column: c, ...(v ? { via: v } : {}) } })} onclear={() => setMap({ lon: undefined })} />
        <p class="hint tight">{t('The value sizes each point. Without one, each point is sized by its number of rows.')}</p>
      {:else}
        <p class="hint tight">{t('Write places the standard way: names or codes.')} <button class="linkbtn" onclick={() => (placesOpen = true)}>{t('See the place names')}</button></p>
      {/if}
    </div>
  {/if}

  {#if isSankey && tile.dims[0]}
    <div class="sec">
      <span class="label">{t('Next stages')}</span>
      {#each tile.dims.slice(1, 3) as d, k}
        <FieldPicker mode="dim" table={d.table} column={d.column} via={d.via} reachable={reach} base={baseTable} onchange={(t, c, v) => c && setDim(k + 1, t, c, v)} onclear={() => removeDim(k + 1)} />
      {/each}
      {#if tile.dims.length < 3}
        <FieldPicker
          mode="dim"
          table={null}
          column={null}
          placeholder={tile.dims.length < 2 ? t('Where it goes next, like a category') : t('A third stage (optional)')}
          reachable={reach} base={baseTable}
          onchange={(t, c, v) => c && setDim(tile.dims.length, t, c, v)} />
      {/if}
      <p class="hint tight">{t('A band is as wide as its value. A stage shows 25 nodes at most; the smallest are added up as Other.')}</p>
    </div>
  {/if}

  {#if periodsMode && tile.dims[0]}
    <div class="sec">
      <span class="label">{t('Periods from')}</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[1]?.table ?? null}
        column={tile.dims[1]?.column ?? null} via={tile.dims[1]?.via}
        placeholder={t('A date, like the year')}
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && set({ dims: [tile.dims[0], defaultDim(app.tables, t, c, v)], periods: undefined })}
        onclear={() => set({ dims: tile.dims.slice(0, 1), periods: undefined })} />
      {#if tile.dims[1] && (dim1Type === 'date' || dim1Type === 'datetime')}
        <div class="grains" role="radiogroup" aria-label={t('Date grouping of the periods')}>
          {#each grains as g}
            <button role="radio" aria-checked={tile.dims[1].grain === g} class:on={tile.dims[1].grain === g} onclick={() => set({ dims: [tile.dims[0], { ...tile.dims[1], grain: g }], periods: undefined })}>{grainNames[g]}</button>
          {/each}
        </div>
      {/if}
      {#if tile.dims[1] && periodChoices.length}
        <div class="pair">
          {#each [0, 1] as i}
            <select class="field" aria-label={i === 0 ? t('First period') : t('Second period')} value={String(periodsNow[i] ?? '')} onchange={(e) => setPeriod(i as 0 | 1, e.currentTarget.value)}>
              {#each periodChoices as v}<option value={String(v)}>{periodText(v)}</option>{/each}
            </select>
          {/each}
        </div>
        <p class="hint tight">{t('Not picked: the latest two. A row that rose is good news, unless you set a rise as bad.')}</p>
      {/if}
    </div>
  {/if}

  {#if showSplit && tile.dims[0] && !repeated && (tile.type === 'table' || isScatter || tile.measures.length < 2)}
    <div class="sec">
      <span class="label">{tile.type === 'table' ? t('Then by') : isScatter ? t('Colour by (optional)') : t('Split by (optional)')}</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[1]?.table ?? null}
        column={tile.dims[1]?.column ?? null} via={tile.dims[1]?.via}
        placeholder={tile.type === 'table' ? t('Another column') : t('Colour by a category')}
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && setDim(1, t, c, v)}
        onclear={() => removeDim(1)} />
    </div>
  {/if}

  {#if showSplit && tile.dims[0] && tile.type !== 'table' && !isScatter && tile.measures.length > 1 && !repeated}
    <p class="hint">{t('Split by is off while a tile shows several values.')}</p>
  {/if}
  {#if repeated}
    <p class="hint">{t('Split by is off while a tile has small multiples.')}</p>
  {/if}

  {#if pivotLike}
    {#if isPivot && tile.dims[0]}
      <div class="sec">
        <span class="label">{t('Then by (optional)')}</span>
        {#each tile.dims.slice(1, PIVOT_MAX_ROWS) as d, k}
          <FieldPicker mode="dim" table={d.table} column={d.column} via={d.via} reachable={reach} base={baseTable} onchange={(t, c, v) => c && setDim(k + 1, t, c, v)} onclear={() => removeDim(k + 1)} />
        {/each}
        {#if tile.dims.length < PIVOT_MAX_ROWS}
          <FieldPicker mode="dim" table={null} column={null} placeholder={t('Another row field')} reachable={reach} base={baseTable} onchange={(t, c, v) => c && setDim(tile.dims.length, t, c, v)} />
        {/if}
      </div>
    {/if}
    <div class="sec">
      <span class="label">{t('Columns by')}</span>
      <FieldPicker
        mode="dim"
        table={tile.cols?.table ?? null}
        column={tile.cols?.column ?? null} via={tile.cols?.via}
        placeholder={t('The field across the top')}
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && set({ cols: defaultDim(app.tables, t, c, v) })}
        onclear={() => set({ cols: undefined })} />
      {#if tile.cols && ['date', 'datetime'].includes(dtypeOf(app.tables, tile.cols.table, tile.cols.column) ?? '')}
        <div class="grains" role="radiogroup" aria-label={t('Date grouping of the columns')}>
          {#each grains as g}
            <button role="radio" aria-checked={tile.cols.grain === g} class:on={tile.cols.grain === g} onclick={() => set({ cols: { ...tile.cols!, grain: g } })}>{grainNames[g]}</button>
          {/each}
        </div>
      {/if}
      {#if isPivot}
        <label class="check">
          <input type="checkbox" checked={tile.totals !== false} onchange={(e) => set({ totals: e.currentTarget.checked ? undefined : false })} />
          {t('Totals')}
        </label>
        <!-- Groups of the first row fields open and close, each with a subtotal (Advanced). -->
        {#if tile.dims.length >= 2 && (advanced || tile.expand)}
          <label class="check">
            <input type="checkbox" checked={!!tile.expand} disabled={tile.totals === false} onchange={(e) => set({ expand: e.currentTarget.checked ? true : undefined })} />
            {t('Open and close groups')}
          </label>
          <p class="hint tight">{tile.totals === false ? t('Turn Totals on to open and close groups.') : t('Each group of the first fields gets a subtotal row with an arrow.')}</p>
        {/if}
        <p class="hint tight">{t('The rows are the fields above. Up to 30 columns are shown.')}</p>
      {:else}
        <p class="hint tight">{t('Each cell is shaded by its number, from light to dark. Up to 30 columns are shown.')}</p>
      {/if}
    </div>
  {/if}

  {#if tile.type === 'table' && tile.measures.length}
    <div class="sec">
      <label class="check first">
        <input type="checkbox" checked={!!tile.totals} onchange={(e) => set({ totals: e.currentTarget.checked ? true : undefined })} />
        {t('Total row')}
      </label>
      <!-- One small line for each row, over the last 12 periods (Advanced). -->
      {#if tile.dims.length && (advanced || tile.spark) && (sparkDates.length || tile.spark)}
        <label class="check">
          <input type="checkbox" checked={!!tile.spark} onchange={(e) => setSpark(e.currentTarget.checked)} />
          {t('Trend column')}
        </label>
        {#if tile.spark}
          <FieldPicker
            mode="dim"
            table={tile.spark.table}
            column={tile.spark.column} via={tile.spark.via}
            datesOnly
            reachable={reach} base={baseTable}
            onchange={(t, c, v) => c && set({ spark: { table: t, column: c, ...(v ? { via: v } : {}), grain: tile.spark?.grain ?? 'month' } })} />
          <div class="grains" role="radiogroup" aria-label={t('Period of the trend')}>
            {#each SPARK_GRAINS as g}
              <button role="radio" aria-checked={(tile.spark.grain ?? 'month') === g} class:on={(tile.spark.grain ?? 'month') === g} onclick={() => set({ spark: { ...tile.spark!, grain: g } })}>{grainNames[g]}</button>
            {/each}
          </div>
          <p class="hint tight">{t('Each row gets a line of its first value over the last 12 periods that are over.')}</p>
        {/if}
      {/if}
    </div>
  {/if}

  {#if isHistogram}
    <div class="sec">
      <span class="label">{t('Bin width')}</span>
      <input
        class="field"
        type="number"
        min="0"
        step="any"
        placeholder={t('auto')}
        aria-label={t('Bin width, or empty for automatic')}
        value={tile.bins?.width ?? ''}
        onchange={(e) => setBinWidth(e.currentTarget.value)} />
      <p class="hint tight">{t('Leave it empty and Tenun picks a width that suits the data.')}</p>
    </div>
  {/if}

  {#if (tile.type === 'pie' || tile.type === 'donut') && tile.dims[0]}
    <!-- A pie or a donut has at most six slices and Other (V4-D7). Not set: five. -->
    <div class="sec">
      <span class="label">{t('Slices')}</span>
      <select class="field" aria-label={t('How many slices before Other')} value={String(Math.min(6, Math.max(2, tile.top?.n ?? 5)))} onchange={(e) => set({ top: { n: Number(e.currentTarget.value) } })}>
        {#each [2, 3, 4, 5, 6] as n}<option value={String(n)}>{t('{n} and Other', { n })}</option>{/each}
      </select>
    </div>
  {:else if topKind && tile.dims[0] && dimTypes[0] !== 'date' && dimTypes[0] !== 'datetime'}
    <div class="sec">
      <span class="label">{t('Top')}</span>
      <select class="field" value={topChoice} onchange={(e) => setTop(e.currentTarget.value)}>
        <option value="all">{t('All')}</option>
        {#each TOP_PRESETS as n}<option value={String(n)}>{t('Top {n}', { n })}</option>{/each}
        <option value="custom">{t('Custom…')}</option>
        {#if (advanced && numberParams.length) || tile.top?.param}
          <optgroup label={t('From a parameter')}>
            {#each numberParams as p (p.id)}<option value="param:{p.id}">{t('Top {n}', { n: p.name })}</option>{/each}
            {#if tile.top?.param && !numberParams.some((p) => p.id === tile.top?.param)}<option value="param:{tile.top.param}">{t('A deleted parameter (Top {n})', { n: tile.top.n })}</option>{/if}
          </optgroup>
        {/if}
      </select>
      {#if topChoice === 'custom'}
        <input
          class="field"
          type="number"
          min="1"
          max="1000"
          aria-label={t('How many to keep')}
          value={tile.top?.n ?? 15}
          onchange={(e) => setTopN(e.currentTarget.value)} />
      {/if}
      {#if tile.top}
        <label class="check">
          <input type="checkbox" checked={tile.top.other !== false} onchange={(e) => set({ top: { ...tile.top!, other: e.currentTarget.checked ? undefined : false } })} />
          {t('Show the rest as Other')}
        </label>
      {/if}
    </div>
  {/if}

  {#if (tile.type === 'bar' || tile.type === 'table' || tile.type === 'waterfall' || isFunnel || isBox || isBullet || isGantt || isCompare) && tile.dims[0]}
    <div class="sec">
      <span class="label">{t('Sort')}</span>
      <select class="field" aria-label={t('Sort')} value={sortId} onchange={(e) => pickSort(e.currentTarget.value)}>
        {#each sortChoices as o}<option value={o.id}>{o.label}</option>{/each}
      </select>
      {#if sortId === 'manual'}
        <!-- Drag a group to a new place, or use the arrows. The order is saved with the tile. -->
        <ul class="order" aria-label={t('Order of the groups')}>
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
              <button class="btn plain icon" aria-label={t('Move {name} up', { name: g })} disabled={i === 0} onclick={() => moveGroup(i, i - 1)}><Icon name="chevronDown" size={12} class="flip" /></button>
              <button class="btn plain icon" aria-label={t('Move {name} down', { name: g })} disabled={i === manualList.length - 1} onclick={() => moveGroup(i, i + 1)}><Icon name="chevronDown" size={12} /></button>
            </li>
          {/each}
        </ul>
        {#if !manualList.length}<p class="hint tight">{t('The groups show here once the tile has data.')}</p>{/if}
      {/if}
    </div>
  {/if}

  {#if canCompare}
    <div class="sec">
      <span class="label">{t('Compare with')}</span>
      <select class="field" aria-label={t('Compare with an earlier period')} value={tile.compare ?? ''} onchange={(e) => set({ compare: (e.currentTarget.value || undefined) as Tile['compare'] })}>
        <option value="">{t('Nothing')}</option>
        <option value="prev_year">{t('Same period last year')}</option>
        <option value="prev_period">{t('Previous period')}</option>
      </select>
    </div>
  {:else if tile.compare && compareType}
    <p class="hint">{t('Comparing needs one value, a date with a grain and no split.')}</p>
  {/if}

  <Suggestions {tile} {current} {filters} onapply={apply} />
  {/if}
  {/if}

  {#if tabNow === 'style'}
  <div class="sec">
    <span class="label">{t('Title')}</span>
    <input
      class="field"
      placeholder={noFields ? t('Optional') : tileTitle({ ...tile, title: '' }, app.tables)}
      value={tile.title}
      oninput={(e) => set({ title: e.currentTarget.value })} />
  </div>

  {#if !isParam && (tile.type === 'kpi' || tile.type === 'waterfall' || periodsMode)}
  <div class="sec">
    <span class="label">{t('Good and bad')}</span>
    {#if tile.type === 'kpi' || tile.type === 'waterfall' || periodsMode}
      <div class="better" role="radiogroup" aria-label={t('Which change is good')}>
        <span class="label">{t('A rise is')}</span>
        <button role="radio" aria-checked={(tile.better ?? 'up') === 'up'} class:on={(tile.better ?? 'up') === 'up'} onclick={() => set({ better: 'up' })}>{t('good')}</button>
        <button role="radio" aria-checked={tile.better === 'down'} class:on={tile.better === 'down'} onclick={() => set({ better: 'down' })}>{t('bad')}</button>
      </div>
    {/if}
    {#if tile.type === 'kpi'}
      <div class="target">
        <span class="label">{t('Target (optional)')}</span>
        <span class="withunit">
        <input
          class="field"
          type="number"
          min="0"
          step="any"
          placeholder={t('No target')}
          aria-label={unit0.unit ? t('Target for this number in {unit}', { unit: unit0.unit }) : t('Target for this number')}
          value={typed(tile.target?.value, unit0.scale)}
          disabled={!!tile.target?.param}
          onchange={(e) => setTarget(e.currentTarget.value)} />
        {#if unit0.unit}<span class="unit" aria-hidden="true">{unit0.unit}</span>{/if}
        </span>
        {#if (advanced && numberParams.length) || tile.target?.param}
          <select class="field" aria-label={t('Where the target comes from')} value={tile.target?.param ?? ''} onchange={(e) => setTargetParam(e.currentTarget.value)}>
            <option value="">{t('A number typed here')}</option>
            {#each numberParams as p (p.id)}<option value={p.id}>{t('The parameter {name}', { name: p.name })}</option>{/each}
            {#if tile.target?.param && !numberParams.some((p) => p.id === tile.target?.param)}<option value={tile.target.param}>{t('A deleted parameter')}</option>{/if}
          </select>
        {/if}
      </div>
    {/if}
  </div>
  {/if}

  {#if !noFields && !lockedType}
  {#if tile.type === 'kpi' && m}
    <CardEditor {tile} kpi={current?.res.kpi ?? null} title={tileTitle(tile, app.tables)} colors={readColors()} {reach} base={baseTable} {set} />
  {/if}

  {#if hasSeries}
    <div class="sec">
      <span class="label">{t('Several series')}</span>
      {#if stackAble}
        <div class="grains stack" role="radiogroup" aria-label={t('How the series are drawn')}>
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
          <p class="hint tight">{t('These values are not in one unit, so they sit side by side. Pick Stacked to pile them up.')}</p>
        {/if}
      {:else}
        <p class="hint tight">{t('{why}, so they cannot be stacked and sit side by side.', { why: whyNotAdded(tile, 0, parts) ?? t('These values are not added up') })}</p>
      {/if}
    </div>
  {/if}

  {#if lineChart && m}
    <div class="sec">
      <span class="label">{t('Lines')}</span>
      {#each lines as l}
        <div class="linerow">
          <span>{lineWords(l)}</span>
          <button class="btn plain icon" aria-label={t('Remove {name}', { name: lineWords(l) })} onclick={() => setLines(lines.filter((x) => x !== l))}><Icon name="x" size={12} /></button>
        </div>
      {/each}
      {#if lines.length < MAX_LINES}
        {#if isScatter}
          <select class="field" aria-label={t('Which value the new line is for')} bind:value={lineAxis}>
            <option value="y">{t('For Y (up)')}</option>
            <option value="x">{t('For X (across)')}</option>
          </select>
        {/if}
        <select class="field" aria-label={t('Add line')} value="" onchange={(e) => { pickLine(e.currentTarget.value); e.currentTarget.value = ''; }}>
          <option value="">{t('Add line…')}</option>
          {#each menu as o}<option value={o.id}>{o.label}</option>{/each}
        </select>
        {#if askTarget}
          <div class="targetform">
            {#if advanced && numberParams.length}
              <select class="field" aria-label={t('Where the target comes from')} bind:value={targetParam}>
                <option value="">{t('A number typed here')}</option>
                {#each numberParams as p (p.id)}<option value={p.id}>{t('The parameter {name}', { name: p.name })}</option>{/each}
              </select>
            {/if}
            {#if !targetParam}
              <span class="withunit"><input class="field" type="number" step="any" placeholder={t('Target value')} aria-label={lineUnit.unit ? t('Target value in {unit}', { unit: lineUnit.unit }) : t('Target value')} bind:value={targetValue} />{#if lineUnit.unit}<span class="unit" aria-hidden="true">{lineUnit.unit}</span>{/if}</span>
            {/if}
            <input class="field" placeholder={t('Name (optional)')} aria-label={t('Name of the target')} bind:value={targetLabel} />
            <div class="row">
              <button class="btn small" disabled={!targetParam && targetValue.trim() === ''} onclick={addTarget}>{t('Add target')}</button>
              <button class="link" onclick={() => (askTarget = false)}>{t('Cancel')}</button>
            </div>
          </div>
        {/if}
      {:else}
        <p class="hint tight">{t('A tile can have up to {n} lines.', { n: MAX_LINES })}</p>
      {/if}
      {#if !isScatter && tile.dims[1] && tile.measures.length < 2 && !addsUpHere(tile, 0, parts)}
        <p class="hint tight">{t('{why}, so on a split chart each line is read from every point drawn, and there is no trend.', { why: whyNotAdded(tile, 0, parts) ?? '' })}</p>
      {/if}
    </div>
  {/if}

  {#if valueChart}
    <div class="sec">
      <span class="label">{t('Values on chart')}</span>
      <select class="field" aria-label={t('Values written on the chart')} value={tile.labels ?? 'auto'} onchange={(e) => set({ labels: e.currentTarget.value === 'auto' ? undefined : (e.currentTarget.value as Tile['labels']) })}>
        <option value="auto">{t('Auto')}</option>
        <option value="all">{t('All')}</option>
        <option value="none">{t('None')}</option>
      </select>
    </div>
  {/if}

  {#if advanced && isBullet}
    <div class="sec adv">
      <span class="label">{t('Advanced')}</span>
      {@render bandsUi()}
    </div>
  {/if}

  {#if advanced && chartKind && tile.dims[0]}
    <div class="sec adv">
      <span class="label">{t('Advanced')}</span>

      {#if !repeated && (tile.measures.length > 1 || tile.dims[1])}
        <span class="label sub">{t('Several values')}</span>
        <div class="grains stack" role="radiogroup" aria-label={t('How several values share the chart')}>
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
        <span class="label sub">{t('Bar or line')}</span>
        {#each tile.measures as mm, i}
          <div class="mixrow">
            <span class="gname" title={measureLabel(mm, app.tables)}>{measureLabel(mm, app.tables)}</span>
            <select
              class="field"
              aria-label={t('How {name} is drawn', { name: measureLabel(mm, app.tables) })}
              disabled={!sharesUnit}
              value={sharesUnit ? (tile.mix?.[i] ?? (tile.type === 'bar' ? 'bar' : 'line')) : (tile.type === 'bar' ? 'bar' : 'line')}
              onchange={(e) => setMix(i, e.currentTarget.value as 'bar' | 'line')}>
              <option value="bar">{t('Bar')}</option>
              <option value="line">{t('Line')}</option>
            </select>
          </div>
        {/each}
        {#if !sharesUnit}
          <p class="hint tight">{t('Different units: use panels. Bars and a line share one axis, so they need values in the same unit.')}</p>
        {/if}
      {/if}

      <span class="label sub">{t('Small multiples')}</span>
      <FieldPicker
        mode="dim"
        table={tile.repeat?.table ?? null}
        column={tile.repeat?.column ?? null} via={tile.repeat?.via}
        placeholder={t('One chart for each…')}
        reachable={reach} base={baseTable}
        onchange={(t, c, v) => c && set({ repeat: defaultDim(app.tables, t, c, v), layout: undefined, mix: undefined })}
        onclear={() => set({ repeat: undefined, ownScale: undefined })} />
      {#if tile.repeat}
        <label class="check">
          <input type="checkbox" checked={!!tile.ownScale} onchange={(e) => set({ ownScale: e.currentTarget.checked ? true : undefined })} />
          {t('Own scale per chart')}
        </label>
        <p class="hint tight">{t('Up to 12 charts, the biggest first. They share one scale, so the sizes can be compared.')}{tile.measures.length > 1 ? ` ${t('Only the first value is shown.')}` : ''}</p>
      {/if}

      {@render bandsUi()}
    </div>
  {/if}

  {#if (advanced || tile.tooltip) && canTip}
    <!-- More in the tooltip (v5 build 10, Advanced): up to 3 more values, and a trend of the first value. -->
    <div class="sec">
      <span class="label">{t('Tooltip')}</span>
      {#each tipExtra as x, i (i)}
        <div class="tiprow">
          <FieldPicker
            mode="measure"
            table={x.table || null}
            column={x.column} via={x.via}
            placeholder={t('A value')}
            reachable={reach} base={baseTable}
            measures={namedList}
            kinds={['number']}
            named={x.named ?? null}
            onnamed={(id) => setTip(i, namedMeasure(id))}
            onchange={(t, c, v) => setTip(i, defaultMeasure(app.tables, t, c, v))}
            onclear={() => setTip(i, null)} />
          {#if !x.named && x.column}
            <select class="field" aria-label={t('How the tooltip value adds up')} value={x.agg} onchange={(e) => setTip(i, { ...x, agg: e.currentTarget.value as Agg })}>
              {#each aggsOf(x).filter((a) => a !== 'closing' && a !== 'opening') as a (a)}<option value={a}>{aggNames[a]}</option>{/each}
            </select>
          {/if}
        </div>
      {/each}
      {#if tipExtra.length < MAX_TIP_VALUES}
        <FieldPicker
          mode="measure"
          table={null}
          column={null}
          placeholder={tipExtra.length ? t('Add another value') : t('Add a value to the tooltip')}
          reachable={reach} base={baseTable}
          measures={namedList}
          kinds={['number']}
          named={null}
          onnamed={(id) => setTip(tipExtra.length, namedMeasure(id))}
          onchange={(t, c, v) => setTip(tipExtra.length, defaultMeasure(app.tables, t, c, v))} />
      {/if}
      <span class="label up">{t('Trend in the tooltip')}</span>
      <div class="tiprow">
        <select class="field" aria-label={t('The date the tooltip\'s trend runs over')} value={tile.tooltip?.trend ? `${tile.tooltip.trend.table}\u0000${tile.tooltip.trend.column}` : ''} onchange={(e) => setTipTrend(e.currentTarget.value)}>
          <option value="">{t('No trend')}</option>
          {#each tipDates as d (d.value)}<option value={d.value}>{d.label}</option>{/each}
        </select>
        {#if tile.tooltip?.trend}
          <select class="field" aria-label={t('The periods of the trend')} value={tile.tooltip.trend.grain ?? 'month'} onchange={(e) => set({ tooltip: { ...tile.tooltip, trend: { ...tile.tooltip!.trend!, grain: e.currentTarget.value as Grain } } })}>
            <option value="day">{t('Days')}</option>
            <option value="week">{t('Weeks')}</option>
            <option value="month">{t('Months')}</option>
            <option value="quarter">{t('Quarters')}</option>
          </select>
        {/if}
      </div>
      <p class="hint tight">{tile.type === 'pie' || tile.type === 'donut' ? t('The tooltip of each slice shows these values too, and the last 12 periods of the first value.') : t('The tooltip of each group shows these values too, and the last 12 periods of the first value.')}</p>
    </div>
  {/if}

  {#if (advanced || tile.unusual) && canMarkUnusual}
    <!-- Unusual values (v5 build 09, Advanced): a ring on a value far from the periods around it. -->
    <div class="sec">
      <label class="check">
        <input type="checkbox" checked={!!tile.unusual} onchange={(e) => set({ unusual: e.currentTarget.checked ? {} : undefined })} />
        {t('Mark unusual values')}
      </label>
      {#if tile.unusual}
        <div class="grains stack" role="radiogroup" aria-label={t('How many to mark')}>
          {#each [['low', t('Fewer')], ['medium', t('Some')], ['high', t('More@@amount')]] as [id, label] (id)}
            <button role="radio" aria-checked={(tile.unusual.level ?? 'medium') === id} class:on={(tile.unusual.level ?? 'medium') === id} onclick={() => set({ unusual: id === 'medium' ? {} : { level: id as 'low' | 'high' } })}>{label}</button>
          {/each}
        </div>
        <p class="hint tight">{t('A value far from the 6 periods around it, measured in their usual spread. Unusual is not wrong: it is worth a look.')}</p>
      {/if}
    </div>
  {/if}

  {#if (advanced || tile.forecast) && canForecast}
    <!-- A forecast (v5 build 08, Advanced): the periods ahead, and the 90 % range. -->
    <div class="sec">
      <label class="check">
        <input type="checkbox" checked={!!tile.forecast} onchange={(e) => set({ forecast: e.currentTarget.checked ? {} : undefined })} />
        {t('Forecast')}
      </label>
      {#if tile.forecast}
        <label class="ahead">
          <span>{t('Periods ahead')}</span>
          <input
            class="field"
            type="number"
            min="1"
            max="36"
            aria-label={t('Periods to forecast')}
            placeholder={String(FORECAST_AHEAD[tile.dims[0]?.grain ?? 'month'] ?? 3)}
            value={tile.forecast.periods ?? ''}
            onchange={(e) => {
              const n = Math.round(Number(e.currentTarget.value));
              set({ forecast: { ...tile.forecast, periods: e.currentTarget.value.trim() && n >= 1 ? Math.min(n, 36) : undefined } });
            }} />
        </label>
        <label class="check">
          <input type="checkbox" checked={tile.forecast.band !== false} onchange={(e) => set({ forecast: { ...tile.forecast, band: e.currentTarget.checked ? undefined : false } })} />
          {t('Show the likely range')}
        </label>
        <p class="hint tight">{t('An estimate from the periods before: their level, trend and season. It needs two full years of months (or of quarters, weeks or days); the unfinished period is not used.')}</p>
      {/if}
    </div>
  {/if}
  {/if}
  {/if}

  {#if tabNow === 'click'}
  {#if actionOffered}
    <div class="sec">
      <span class="label">{t('A click on {mark}', { mark: markWord })}</span>
      <div class="choices" role="radiogroup" aria-label={t('What a click does')}>
        <button role="radio" aria-checked={!tile.action} class:on={!tile.action} onclick={() => setAction(null)}>
          <span class="dot" aria-hidden="true"></span>
          <span><b>{t('Filters this page')}</b><small>{t('The other tiles show only the value clicked.')}</small></span>
        </button>
        {#each otherPages as d (d.id)}
          <button role="radio" aria-checked={tile.action?.page === d.id} class:on={tile.action?.page === d.id} onclick={() => setAction(d.id)}>
            <span class="dot" aria-hidden="true"></span>
            <span><b>{t('Opens {page}', { page: d.title || t('Untitled') })}</b><small>{t('That page keeps only the value clicked.')}</small></span>
          </button>
        {/each}
        {#if tile.action && !otherPages.some((d) => d.id === tile.action?.page)}
          <button role="radio" aria-checked="true" class="on" disabled>
            <span class="dot" aria-hidden="true"></span>
            <span><b>{t('Opens a page that is gone')}</b><small>{t('Pick another choice above.')}</small></span>
          </button>
        {/if}
      </div>
    </div>
  {/if}
  {#if reachShown}
    <div class="sec">
      <span class="label">{t('Filters other tiles')}</span>
      <p class="fact num">{tile.reach === undefined ? t('Every tile on this page') : tile.reach === 'none' ? t('No other tile') : tn('{n} tile', '{n} tiles', tile.reach.length)}</p>
      <p class="hint tight">{t('Pick the tiles in the tile\'s ⋯ menu, under "Filters other tiles".')}</p>
    </div>
  {/if}
  {#if advanced && !noFields && !lockedType && drillChoices.length}
    <div class="sec adv">
      <span class="label">{t('Drill down')}</span>
      <select class="field" aria-label={t('Drill down')} value={drillSettingValue(tile.drill)} onchange={(e) => set({ drill: drillSetting(e.currentTarget.value) })}>
        {#each drillChoices as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
      <p class="hint tight">
        {t('A reader switches on drill-down in the tile\'s corner; a click then opens the level below. Make a hierarchy, such as Region › Store, in the Model view.')}
      </p>
    </div>
  {/if}
  {/if}
  </div>

  <footer class="ifoot">
    <span class="live">{t('Changes apply as you edit')}</span>
    <button class="btn" title={t('Undo the last change in this panel')} aria-label={t('Undo the last change to this tile')} disabled={!history.length} onclick={undoLast}>{t('Undo')}</button>
    <button class="btn primary" onclick={onclose}>{t('Done')}</button>
  </footer>
</aside>

{#if placesOpen}
  <PlaceNames level={mapSpec.level} onclose={() => (placesOpen = false)} />
{/if}

<style>
  .linkbtn { border: 0; background: none; padding: 0; color: var(--accent-text); font: inherit; cursor: pointer; }
  .linkbtn:hover { text-decoration: underline; }
  .insp {
    width: 340px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-left: 1px solid var(--sep);
    background: var(--surface);
  }
  .ihead { display: flex; align-items: center; gap: 2px; padding: 10px 10px 10px 16px; border-bottom: 1px solid var(--sep); flex: none; }
  .ihead .btn.icon { color: var(--text-2); }
  .ihead .btn.icon:hover { color: var(--text); }
  .who { display: flex; flex-direction: column; flex: 1; min-width: 0; margin-right: 6px; }
  .eyebrow { font-size: 11px; color: var(--text-2); }
  .name { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tabs { display: flex; gap: 18px; padding: 0 16px; border-bottom: 1px solid var(--sep); flex: none; }
  .tabs button {
    position: relative;
    height: 36px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-2);
    font-weight: 500;
    transition: color 140ms var(--spring);
  }
  .tabs button:hover { color: var(--text); }
  .tabs button.on { color: var(--text); font-weight: 600; }
  .tabs button.on::after { content: ''; position: absolute; left: 0; right: 0; bottom: -1px; height: 2px; border-radius: 1px; background: var(--accent); }
  .body { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 20px; padding: 16px; }
  .body :global(.sec > .label:first-child) { font-family: var(--mono); font-size: 10.5px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); }
  .sh { display: flex; align-items: baseline; justify-content: space-between; font-family: var(--mono); font-size: 10.5px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); }
  .sh .now { font-family: var(--font); font-weight: 500; letter-spacing: 0; text-transform: none; font-size: 12px; }
  .rule { height: 1px; background: var(--sep); flex: none; }
  .disclose { display: flex; align-items: center; gap: 6px; align-self: flex-start; height: 24px; padding: 0; border: 0; background: none; font-size: 12px; color: var(--accent-text); }
  .disclose:hover { text-decoration: underline; }
  .chev { display: inline-grid; transform: rotate(-90deg); transition: transform 160ms var(--spring); }
  .chev.open { transform: none; }
  .ifoot { display: flex; align-items: center; gap: 8px; padding: 10px 16px; border-top: 1px solid var(--sep); flex: none; }
  .ifoot .live { flex: 1; font-size: 11px; color: var(--text-2); }
  .ifoot .btn { height: 30px; }
  .choices { display: flex; flex-direction: column; gap: 6px; }
  .choices button {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 9px 10px;
    border: 1px solid var(--sep);
    border-radius: 8px;
    background: var(--surface);
    text-align: left;
    transition: background 120ms var(--spring), border-color 120ms var(--spring);
  }
  .choices button:hover { background: var(--hover); }
  .choices button.on { border-color: var(--accent); background: var(--selected); }
  .choices b { display: block; font-weight: 600; font-size: 12px; }
  .choices small { display: block; font-size: 11px; color: var(--text-2); margin-top: 1px; }
  .choices .dot { flex: none; width: 14px; height: 14px; margin-top: 1px; border-radius: 50%; border: 1.5px solid var(--sep-strong); background: var(--surface); }
  .choices button.on .dot { border: 4px solid var(--accent); }
  .fact { margin: 0; font-weight: 500; }
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
  .types { display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 4px; }
  .types button {
    display: grid;
    place-items: center;
    height: 38px;
    padding: 0;
    border: 1px solid var(--sep);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text-2);
    transition: background 120ms var(--spring), border-color 120ms var(--spring), transform 120ms var(--spring);
  }
  .types button:hover { background: var(--hover); color: var(--text); }
  .types button:active { transform: scale(0.96); }
  .types button.on { border-color: var(--accent); background: var(--selected); color: var(--accent-text); }
  .types button.soft:not(.on) { opacity: 0.55; }
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
  .check { display: flex; align-items: center; gap: 8px; font-size: 12px; margin-top: 2px; }
  select.field {
    height: 32px;
    appearance: none;
    padding: 0 28px 0 10px;
    border: 1px solid var(--sep-strong);
    background: var(--surface) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 20 20' fill='none' stroke='%238a8a8f' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M5 8l5 5 5-5'/%3E%3C/svg%3E") no-repeat right 9px center / 12px 12px;
    font-size: 12px;
    text-overflow: ellipsis;
    transition: border-color 120ms var(--spring);
  }
  select.field:hover:not(:disabled) { border-color: var(--text-3); }
  select.field:disabled { opacity: 0.55; }
  input.field { height: 32px; font-size: 12px; }
  .check input { margin: 0; accent-color: var(--accent); }
  .num, input[type='number'] { font-variant-numeric: tabular-nums; }
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
  .fmt { margin: 6px 0 10px; }
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
  .ahead { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .ahead .field { width: 80px; }
  .tiprow { display: flex; gap: 6px; align-items: center; margin-bottom: 4px; }
  .tiprow > :global(*) { flex: 1; min-width: 0; }
</style>

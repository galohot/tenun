// Saved views (v5 build 03): a dashboard keeps named views of how it was read: the clicks, the filter tiles'
// selections, the grain for every chart, the drill paths, and (from build 05) the parameter values. Opening a view
// sets exactly those. Pure, with type imports and t(), so the node tests load it.

import type { Cell, Dashboard, DashFilter, DrillState, Grain, SavedView, Tile, TileFilter } from './api';
import { t } from './i18n.ts';

/** A dashboard keeps at most this many views (V5-D5: an export holds each one in full). */
export const MAX_VIEWS = 10;

/** What a reader set on a dashboard, as a view holds it. */
export interface ViewState {
  filters: DashFilter[];
  selections: Record<string, Pick<TileFilter, 'values' | 'range' | 'preset'>>;
  grain: Grain | null;
  drill: Record<string, DrillState['path']>;
  params?: Record<string, Cell>;
}

const selectionOf = (f: TileFilter | undefined): Pick<TileFilter, 'values' | 'range' | 'preset'> => ({
  ...(f?.values?.length ? { values: f.values } : {}),
  ...(f?.range && (f.range[0] !== null || f.range[1] !== null) ? { range: f.range } : {}),
  ...(f?.preset ? { preset: f.preset } : {}),
});

/**
 * The state of a dashboard now: its clicks, the selection of each filter tile, its grain, the drill paths, and the
 * value of each parameter that has a control on it (build 05; `values` are the workspace's).
 */
export function currentState(
  dash: Pick<Dashboard, 'tiles' | 'grain'>,
  clicks: readonly DashFilter[],
  drills: Readonly<Record<string, DrillState>>,
  values: Readonly<Record<string, Cell>> = {},
): ViewState {
  const selections: ViewState['selections'] = {};
  for (const t of dash.tiles) if (t.type === 'filter') selections[t.id] = selectionOf(t.filter);
  const drill: ViewState['drill'] = {};
  for (const [id, s] of Object.entries(drills)) if (s.path.length && dash.tiles.some((t) => t.id === id)) drill[id] = s.path;
  const params: Record<string, Cell> = {};
  for (const t of dash.tiles) if (t.type === 'param' && t.param?.id && t.param.id in values) params[t.param.id] = values[t.param.id];
  return { filters: [...clicks], selections, grain: dash.grain ?? null, drill, ...(Object.keys(params).length ? { params } : {}) };
}

/** A view of the state now, with a name. A new view has an empty id; `newViewId` gives it one. */
export const viewOf = (id: string, name: string, state: ViewState): SavedView => ({ id, name: name.trim() || t('View'), ...state });

/** The next `v<n>` id of a dashboard's views. */
export function newViewId(views: readonly SavedView[]): string {
  const n = Math.max(0, ...views.map((v) => Number(/^v(\d+)$/.exec(v.id)?.[1] ?? 0)));
  return `v${n + 1}`;
}

const canon = (s: ViewState) =>
  JSON.stringify({
    filters: [...s.filters].map((f) => ({ ...f, label: undefined })).sort((a, b) => a.fromTile.localeCompare(b.fromTile)),
    selections: Object.fromEntries(Object.entries(s.selections).filter(([, v]) => Object.keys(v).length).sort(([a], [b]) => a.localeCompare(b))),
    grain: s.grain ?? null,
    drill: Object.fromEntries(Object.entries(s.drill).filter(([, p]) => p.length).sort(([a], [b]) => a.localeCompare(b))),
    params: s.params ?? {},
  });

/** Whether the dashboard reads as the view: same clicks, selections, grain, drill paths and parameters. */
export const sameState = (a: ViewState, b: ViewState): boolean => canon(a) === canon(b);

/**
 * Whether the dashboard now reads as a saved view. Only the parameters the view holds count: a view saved before a
 * parameter control was added reads the same.
 */
export function sameAsView(now: ViewState, v: SavedView): boolean {
  const keys = Object.keys(v.params ?? {});
  const params = Object.fromEntries(keys.filter((k) => now.params && k in now.params).map((k) => [k, now.params![k]]));
  return sameState(
    { ...now, params: keys.length ? params : undefined },
    { filters: v.filters, selections: v.selections ?? {}, grain: v.grain ?? null, drill: v.drill ?? {}, params: v.params },
  );
}

/**
 * What opening a view does to the tiles: each filter tile takes the view's selection (none when the view has none for
 * it; a filter tile the view does not know keeps nothing either). Other tiles are as they are.
 */
export function tilesWithView(tiles: readonly Tile[], view: SavedView): Tile[] {
  return tiles.map((t) => {
    if (t.type !== 'filter' || !t.filter) return t;
    const sel = view.selections?.[t.id] ?? {};
    const { values: _v, range: _r, preset: _p, ...rest } = t.filter;
    return { ...t, filter: { ...rest, ...sel } };
  });
}

/** The clicks of a view that still name a tile of the dashboard. */
export const viewClicks = (tiles: readonly Tile[], view: SavedView): DashFilter[] => view.filters.filter((f) => tiles.some((t) => t.id === f.fromTile));

/** The drill states of a view, for the tiles that still drill. */
export function viewDrills(tiles: readonly Tile[], view: SavedView): Record<string, DrillState> {
  const out: Record<string, DrillState> = {};
  for (const [id, path] of Object.entries(view.drill ?? {})) if (tiles.some((t) => t.id === id && t.drill)) out[id] = { path };
  return out;
}

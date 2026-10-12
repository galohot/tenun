// Copy and paste of tiles and dashboards (build 09, V4-D11). A copy carries the tiles and only what they need: the
// named measures they use, the card styles they use, and the tables they read, with the columns they and the measures'
// formulas read. A paste into the
// same workspace keeps every field; a paste into another workspace matches each table and column first (MatchFields).
// Pure, with type-only imports, so the node tests load it.

import type { CardStyle, Dashboard, DType, Grain, NamedMeasure, Table, Tile } from './api';

/** The first line of a copy, so a paste knows it is one. */
export const MARKER = 'tenun/tiles';

export interface ClipTable {
  id: string;
  label: string;
  columns: { name: string; dtype: DType }[];
}

export interface Clip {
  tenun: 'tiles';
  version: 1;
  tiles: Tile[];
  measures: NamedMeasure[];
  cardStyles: CardStyle[];
  tables: ClipTable[];
  /** A whole dashboard: its title and grain. The tiles keep their places. */
  dashboard?: { title: string; grain?: Grain | null };
}

/** A field a tile reads: a value, a group, a "divide by", the columns of a pivot, the latitude of a map… */
export interface FieldRef {
  table: string;
  column: string | null;
}

/** Every object in a tile that names a table: the fields it reads. A named measure's table means nothing, so it is left out. */
function refsIn(x: unknown, out: { obj: Record<string, unknown> }[]): void {
  if (Array.isArray(x)) {
    for (const v of x) refsIn(v, out);
  } else if (x && typeof x === 'object') {
    const o = x as Record<string, unknown>;
    if (typeof o.table === 'string' && !o.named) out.push({ obj: o });
    for (const [k, v] of Object.entries(o)) if (k !== 'image' && k !== 'text') refsIn(v, out);
  }
}

/** The fields a tile reads. */
export function fieldsOf(tile: Tile): FieldRef[] {
  const found: { obj: Record<string, unknown> }[] = [];
  refsIn(tile, found);
  return found.map(({ obj }) => ({ table: obj.table as string, column: typeof obj.column === 'string' ? obj.column : null }));
}

/** The named measures a tile uses, by id. */
export const namedOf = (tile: Tile): string[] => tile.measures.flatMap((m) => (m.named ? [m.named] : []));

/** Everything a copy of these tiles needs from this workspace, and nothing else. */
export function buildClip(
  tiles: readonly Tile[],
  ws: { measures?: NamedMeasure[]; cardStyles?: CardStyle[] },
  tables: readonly Table[],
  /** The tables each named measure reads (`Snapshot.measure_info[id].tables`). */
  measureTables: Record<string, string[]>,
  dashboard?: Clip['dashboard'],
): Clip {
  const copy = <T>(x: T): T => JSON.parse(JSON.stringify(x));
  const named = new Set(tiles.flatMap(namedOf));
  const styles = new Set(tiles.flatMap((t) => (t.card?.style ? [t.card.style] : [])));
  const used = new Map<string, Set<string>>();
  const use = (table: string, column: string | null) => {
    if (!used.has(table)) used.set(table, new Set());
    if (column) used.get(table)!.add(column);
  };
  for (const t of tiles) for (const f of fieldsOf(t)) use(f.table, f.column);
  // A named measure reads the columns its formula names, in the tables the engine says it reads.
  for (const m of (ws.measures ?? []).filter((x) => named.has(x.id))) {
    const homes = (measureTables[m.id] ?? []).map((id) => tables.find((t) => t.id === id)).filter((t): t is Table => !!t);
    for (const t of homes) use(t.id, null);
    for (const r of formulaRefs(m.formula)) {
      const t = homes.find((x) => same(x.label, r.table)) ?? tables.find((x) => same(x.label, r.table));
      if (t) use(t.id, t.columns.find((c) => same(c.name, r.column))?.name ?? null);
    }
  }
  return {
    tenun: 'tiles',
    version: 1,
    tiles: copy([...tiles]),
    measures: copy((ws.measures ?? []).filter((m) => named.has(m.id))),
    cardStyles: copy((ws.cardStyles ?? []).filter((s) => styles.has(s.id))),
    // Each table with only the columns the tiles and the measures' formulas read.
    tables: [...used.keys()].flatMap((id) => {
      const t = tables.find((x) => x.id === id);
      if (!t) return [];
      const cols = t.columns.filter((c) => used.get(id)!.has(c.name));
      return [{ id, label: t.label, columns: cols.map((c) => ({ name: c.name, dtype: c.dtype })) }];
    }),
    ...(dashboard ? { dashboard } : {}),
  };
}

/** The text a copy puts on the clipboard: the marker, then the JSON. Another app sees readable JSON. */
export const clipText = (clip: Clip): string => `${MARKER}\n${JSON.stringify(clip)}`;

/** A copy read back from the clipboard, or null when the text is not one (a table pasted from Excel, a sentence). */
export function readClip(text: string): Clip | null {
  if (!text.startsWith(MARKER + '\n') && !text.startsWith(MARKER + '\r\n')) return null;
  let c: unknown;
  try {
    c = JSON.parse(text.slice(text.indexOf('\n') + 1));
  } catch {
    return null;
  }
  const o = c as Partial<Clip>;
  if (!o || o.tenun !== 'tiles' || o.version !== 1 || !Array.isArray(o.tiles)) return null;
  if (!o.tiles.every((t) => t && typeof t === 'object' && typeof t.type === 'string' && Array.isArray(t.measures) && Array.isArray(t.dims))) return null;
  return {
    tenun: 'tiles',
    version: 1,
    tiles: o.tiles,
    measures: Array.isArray(o.measures) ? o.measures : [],
    cardStyles: Array.isArray(o.cardStyles) ? o.cardStyles : [],
    tables: Array.isArray(o.tables) ? o.tables : [],
    ...(o.dashboard && typeof o.dashboard.title === 'string' ? { dashboard: { title: o.dashboard.title, grain: o.dashboard.grain ?? null } } : {}),
  };
}

// ---- matching the fields of another workspace ----------------------------------------------------------------------

/** Where each table of a copy goes here, and each of its columns. null: not matched. */
export interface TableMatch {
  to: string | null;
  columns: Record<string, string | null>;
}
export type Matches = Record<string, TableMatch>;

const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

/** True when every table and column the copy reads is here, under the same ids: the copy came from this workspace. */
export function sameWorkspace(clip: Clip, here: readonly Table[]): boolean {
  return clip.tables.every((ct) => {
    const t = here.find((x) => x.id === ct.id);
    return !!t && ct.columns.every((c) => t.columns.some((x) => x.name === c.name));
  });
}

/**
 * A first guess for each table: the table here with the same label, else the one that shares the most column names
 * (at least one). For each column: the same name, else the first column of the same type not already taken.
 */
export function guessMatches(clip: Clip, here: readonly Table[]): Matches {
  const out: Matches = {};
  for (const ct of clip.tables) {
    const byLabel = here.find((t) => same(t.label, ct.label));
    let to = byLabel;
    if (!to) {
      let best = 0;
      for (const t of here) {
        const n = ct.columns.filter((c) => t.columns.some((x) => same(x.name, c.name))).length;
        if (n > best) [best, to] = [n, t];
      }
    }
    out[ct.id] = { to: to?.id ?? null, columns: to ? guessColumns(ct, to) : Object.fromEntries(ct.columns.map((c) => [c.name, null])) };
  }
  return out;
}

/** The columns of a copied table, matched in a table here: the same name, else the first free column of the same type. */
export function guessColumns(ct: ClipTable, to: Table): Record<string, string | null> {
  const out: Record<string, string | null> = {};
  const taken = new Set<string>();
  for (const c of ct.columns) {
    const hit = to.columns.find((x) => same(x.name, c.name));
    if (hit) {
      out[c.name] = hit.name;
      taken.add(hit.name);
    }
  }
  for (const c of ct.columns) {
    if (c.name in out) continue;
    const hit = to.columns.find((x) => x.dtype === c.dtype && !taken.has(x.name));
    out[c.name] = hit?.name ?? null;
    if (hit) taken.add(hit.name);
  }
  return out;
}

/** The fields of the copy that no match places. A tile that reads one pastes unlinked: its Inspector shows what to pick. */
export function unmatched(clip: Clip, m: Matches): { table: string; column: string | null }[] {
  return clip.tables.flatMap((ct): { table: string; column: string | null }[] => {
    const t = m[ct.id];
    if (!t?.to) return [{ table: ct.label, column: null }];
    return ct.columns.filter((c) => !t.columns[c.name]).map((c) => ({ table: ct.label, column: c.name }));
  });
}

/**
 * A tile with its fields moved to the matched tables and columns. A field whose table or column has no match keeps what it
 * had: the tile then names a field this workspace does not have, and says so. A link (`via`) of the other workspace is
 * dropped, so the field takes the default path here.
 */
export function remapTile(tile: Tile, m: Matches, links: ReadonlySet<string>): Tile {
  const t: Tile = JSON.parse(JSON.stringify(tile));
  const found: { obj: Record<string, unknown> }[] = [];
  refsIn(t, found);
  for (const { obj } of found) {
    const hit = m[obj.table as string];
    if (hit?.to) {
      const col = typeof obj.column === 'string' ? hit.columns[obj.column] : undefined;
      obj.table = hit.to;
      if (typeof obj.column === 'string' && col) obj.column = col;
    }
    if (typeof obj.via === 'string' && !links.has(obj.via)) delete obj.via;
  }
  return t;
}

// ---- formulas -------------------------------------------------------------------------------------------------------

/** A name written in brackets, with `]]` for a `]` inside it. */
const BRACKET = /\[((?:[^\]]|\]\])*)\]/y;

/**
 * Walks a formula: `pair` gets each `[Table].[Column]` with its place, and returns the text to put there (or null to
 * keep it). Text in quotes is passed by.
 */
function walk(formula: string, pair: (table: string, column: string) => string | null): string {
  const unq = (s: string) => s.replace(/\]\]/g, ']');
  let out = '';
  let i = 0;
  while (i < formula.length) {
    const ch = formula[i];
    if (ch === '"') {
      // A text: up to the closing quote, with "" for a quote inside it.
      let j = i + 1;
      while (j < formula.length && !(formula[j] === '"' && formula[j + 1] !== '"')) j += formula[j] === '"' ? 2 : 1;
      out += formula.slice(i, j + 1);
      i = j + 1;
      continue;
    }
    if (ch === '[') {
      BRACKET.lastIndex = i;
      const a = BRACKET.exec(formula);
      if (a) {
        const after = i + a[0].length;
        BRACKET.lastIndex = after + 1;
        const b = formula[after] === '.' ? BRACKET.exec(formula) : null;
        if (b) {
          const to = pair(unq(a[1]), unq(b[1]));
          out += to ?? formula.slice(i, after + 1 + b[0].length);
          i = after + 1 + b[0].length;
          continue;
        }
        out += a[0];
        i = after;
        continue;
      }
    }
    out += ch;
    i++;
  }
  return out;
}

/** The `[Table].[Column]` pairs a formula reads, by table label. */
export function formulaRefs(formula: string): { table: string; column: string }[] {
  const out: { table: string; column: string }[] = [];
  walk(formula, (table, column) => {
    out.push({ table, column });
    return null;
  });
  return out;
}

/**
 * A named measure's formula with its tables and columns renamed: `[Sales].[amount]` becomes `[Orders].[total]` when
 * Sales is matched to Orders and amount to total. Text in quotes is left alone. The engine checks the formula again when
 * it is saved (build 01).
 */
export function rewriteFormula(formula: string, renames: { table: string; to: string; columns: Record<string, string> }[]): string {
  const q = (s: string) => `[${s.replace(/]/g, ']]')}]`;
  return walk(formula, (table, column) => {
    const r = renames.find((x) => same(x.table, table));
    if (!r) return null;
    const to = Object.entries(r.columns).find(([k]) => same(k, column))?.[1] ?? column;
    return `${q(r.to)}.${q(to)}`;
  });
}

// ---- landing --------------------------------------------------------------------------------------------------------

/** Tiles placed below the lowest tile already there, keeping their places relative to each other. */
export function placeBelow(tiles: readonly Tile[], existing: readonly Tile[]): Tile[] {
  if (!tiles.length) return [];
  const floor = existing.reduce((m, t) => Math.max(m, t.y + t.h), 0);
  const top = Math.min(...tiles.map((t) => t.y));
  return tiles.map((t) => ({ ...t, y: t.y - top + floor }));
}

/** A name that is free among `taken`: the name, else "Name (2)", "Name (3)"… */
export function freeName(name: string, taken: readonly string[]): string {
  const has = (n: string) => taken.some((t) => same(t, n));
  if (!has(name)) return name;
  for (let k = 2; ; k++) if (!has(`${name} (${k})`)) return `${name} (${k})`;
}

/**
 * The named measures of a copy, joined to the measures here. One that is here already, with the same id, name and formula,
 * is used as it is; any other is added, renamed "Name (2)" when its name is taken, with a new id (`''`: the engine gives
 * it one). `ids` says which id each copied measure has now, for the tiles; a new one is keyed by its place in `added`.
 */
export function joinMeasures(
  copied: readonly NamedMeasure[],
  here: readonly NamedMeasure[],
  rewrite: (formula: string) => string = (f) => f,
): { added: NamedMeasure[]; ids: Record<string, string | { added: number }> } {
  const added: NamedMeasure[] = [];
  const ids: Record<string, string | { added: number }> = {};
  const names = here.map((m) => m.name);
  for (const m of copied) {
    const formula = rewrite(m.formula);
    const twin = here.find((h) => h.id === m.id && h.name === m.name && h.formula === formula);
    if (twin) {
      ids[m.id] = twin.id;
      continue;
    }
    const name = freeName(m.name, names);
    names.push(name);
    ids[m.id] = { added: added.length };
    added.push({ ...m, id: '', name, formula });
  }
  return { added, ids };
}

/** The card styles of a copy, joined to the styles here: the same as `joinMeasures`, with new ids made here. */
export function joinStyles(copied: readonly CardStyle[], here: readonly CardStyle[], newId: () => string): { list: CardStyle[]; ids: Record<string, string> } {
  const list = [...here];
  const ids: Record<string, string> = {};
  for (const s of copied) {
    const twin = here.find((h) => h.id === s.id && h.name === s.name && JSON.stringify(h.card) === JSON.stringify(s.card));
    if (twin) {
      ids[s.id] = twin.id;
      continue;
    }
    const id = here.some((h) => h.id === s.id) ? newId() : s.id;
    const name = freeName(s.name, list.map((x) => x.name));
    list.push({ id, name, card: JSON.parse(JSON.stringify(s.card)) });
    ids[s.id] = id;
  }
  return { list, ids };
}

// ---- landing a paste ------------------------------------------------------------------------------------------------

/** The renames a measure's formula needs: each copied table matched here, by label, with the columns that were matched. */
export function renamesFor(clip: Clip, m: Matches, here: readonly Table[]): { table: string; to: string; columns: Record<string, string> }[] {
  return clip.tables.flatMap((ct) => {
    const hit = m[ct.id];
    const to = here.find((t) => t.id === hit?.to);
    if (!hit || !to) return [];
    const columns = Object.fromEntries(Object.entries(hit.columns).filter((e): e is [string, string] => !!e[1]));
    return [{ table: ct.label, to: to.label, columns }];
  });
}

/**
 * The id each copied measure has once the new ones are saved. The engine gives a new measure its id and keeps the order
 * it was sent in, so the k-th one kept is at `here + k` in the saved list. A measure the engine refused has none: its
 * tiles keep the old id and say the measure is missing.
 */
export function measureIds(
  ids: Record<string, string | { added: number }>,
  added: readonly NamedMeasure[],
  kept: readonly NamedMeasure[],
  saved: readonly NamedMeasure[],
  here: number,
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [from, to] of Object.entries(ids)) {
    if (typeof to === 'string') out[from] = to;
    else {
      const k = kept.indexOf(added[to.added]);
      if (k >= 0 && saved[here + k]) out[from] = saved[here + k].id;
    }
  }
  return out;
}

/**
 * The tiles of a copy as they land: their fields moved to the matched ones (from another workspace; `m` null when the copy
 * is from this one), named measures and card styles by their ids here, and new tile ids.
 */
export function landTiles(
  clip: Clip,
  m: Matches | null,
  ids: { measures: Record<string, string>; styles: Record<string, string> },
  links: ReadonlySet<string>,
  newId: () => string,
): Tile[] {
  return clip.tiles.map((t) => {
    const x: Tile = m ? remapTile(t, m, links) : JSON.parse(JSON.stringify(t));
    x.measures = x.measures.map((mm) => (mm.named ? { ...mm, named: ids.measures[mm.named] ?? mm.named } : mm));
    if (x.card?.style) x.card = { ...x.card, style: ids.styles[x.card.style] ?? x.card.style };
    return { ...x, id: newId() };
  });
}

/** A dashboard as a copy: its tiles and their places, new ids, and the filters it keeps pointed at the new tiles. */
export function dashboardCopy(d: Dashboard, title: string, newId: (prefix: string) => string): Dashboard {
  const c: Dashboard = JSON.parse(JSON.stringify(d));
  const ids = new Map(c.tiles.map((t) => [t.id, newId('t')]));
  return {
    ...c,
    id: newId('d'),
    title,
    tiles: c.tiles.map((t) => ({ ...t, id: ids.get(t.id)! })),
    // A published dashboard's link stays with the original.
    publish: undefined,
    ...(c.filters ? { filters: c.filters.map((f) => ({ ...f, fromTile: ids.get(f.fromTile) ?? f.fromTile })) } : {}),
  };
}

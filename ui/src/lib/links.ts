// Links between tables: what the data says about them, which tables a tile can reach, and the
// names that tell two links to one table apart (V4-D26 to V4-D28). Pure functions, so the node
// tests can load them. The engine decides what a link means; this file only follows its answers.

import type { LinkHealth, Relationship, Table } from './api';

type Health = Record<string, LinkHealth> | undefined;
type Named = Pick<Table, 'id' | 'label'> & { columns?: { name: string; label?: string }[] };

/** The longest name a link may have. Same as the engine. */
export const MAX_NAME = 60;

// ---- what the data says --------------------------------------------------------------------

/** A link with no health record is read as stored: forward only. */
const usable = (h?: LinkHealth) => !h || h.kind !== 'many-many';
/** The link may be crossed the stored way, from the many side to the one side. */
const forwardOk = (h?: LinkHealth) => !h || h.kind === 'one-many' || h.kind === 'one-one';
/** The link may be crossed from the one side to the many side: no key repeats on the many side. */
const backwardOk = (h?: LinkHealth) => !!h && h.fromUnique;

/** The words on the line between two tables, read from the one side to the many side. */
export const KIND_WORDS: Record<LinkHealth['kind'], string> = {
  'one-many': '1 – many',
  'one-one': '1 – 1',
  'many-one': 'many – 1',
  'many-many': 'many – many',
};

export const kindWords = (h: LinkHealth | undefined): string => KIND_WORDS[h?.kind ?? 'one-many'];

/** A link that has something to say: a key that repeats, or a link the engine reads another way. */
export const hasWarning = (h: LinkHealth | undefined): boolean => !!h?.note;

// ---- which tables can be reached -----------------------------------------------------------

/** The edges out of a table, in the order the links were made: forward first, then the other way. */
function edgesOf(table: string, rels: Relationship[], health: Health): { rel: string; to: string }[] {
  const out: { rel: string; to: string }[] = [];
  for (const r of rels) {
    const h = health?.[r.id];
    if (!usable(h)) continue;
    if (r.from.table === table && forwardOk(h)) out.push({ rel: r.id, to: r.to.table });
  }
  for (const r of rels) {
    const h = health?.[r.id];
    if (!usable(h)) continue;
    if (r.to.table === table && backwardOk(h)) out.push({ rel: r.id, to: r.from.table });
  }
  return out;
}

/** Tables a query based on `base` can use: base plus everything reachable. Same rules as the engine. */
export function reachableFrom(base: string, rels: Relationship[], health?: Health): Set<string> {
  const seen = new Set([base]);
  const queue = [base];
  while (queue.length) {
    const t = queue.shift()!;
    for (const e of edgesOf(t, rels, health)) {
      if (!seen.has(e.to)) {
        seen.add(e.to);
        queue.push(e.to);
      }
    }
  }
  return seen;
}

/**
 * The links a path from `base` to `table` can start with: the first link of every shortest path.
 * One entry means the way is clear. Two or more mean a field of `table` can mean either link.
 */
export function firstLinks(base: string, table: string, rels: Relationship[], health?: Health): string[] {
  if (base === table) return [];
  const dist = new Map<string, number>([[base, 0]]);
  const first = new Map<string, Set<string>>();
  const queue = [base];
  while (queue.length) {
    const t = queue.shift()!;
    const d = dist.get(t)!;
    for (const e of edgesOf(t, rels, health)) {
      const here = t === base ? new Set([e.rel]) : (first.get(t) ?? new Set<string>());
      if (!dist.has(e.to)) {
        dist.set(e.to, d + 1);
        first.set(e.to, new Set(here));
        queue.push(e.to);
      } else if (dist.get(e.to) === d + 1) {
        for (const id of here) first.get(e.to)!.add(id);
      }
    }
  }
  const found = first.get(table);
  if (!found) return [];
  const order = new Map(rels.map((r, i) => [r.id, i]));
  return [...found].sort((a, b) => (order.get(a) ?? 0) - (order.get(b) ?? 0));
}

// ---- names ---------------------------------------------------------------------------------

const tableLabel = (tables: Named[], id: string): string => tables.find((t) => t.id === id)?.label ?? id;

/** The name Tenun proposes for a link: the one-side table with the many-side column, "Stores (ship_store_id)". */
export function defaultName(rel: Relationship, tables: Named[]): string {
  return `${tableLabel(tables, rel.to.table)} (${rel.from.column})`;
}

/** What a link is called: its name, else the proposal. */
export function linkLabel(rel: Relationship, tables: Named[]): string {
  return rel.name?.trim() || defaultName(rel, tables);
}

/** The links that join the same two tables as this one, this one included, in the order they were made. */
export function siblings(rel: Relationship, rels: Relationship[]): Relationship[] {
  const same = (r: Relationship) =>
    (r.from.table === rel.from.table && r.to.table === rel.to.table) ||
    (r.from.table === rel.to.table && r.to.table === rel.from.table);
  return rels.filter(same);
}

/** Groups of two or more links that join the same two tables. */
export function pairs(rels: Relationship[]): Relationship[][] {
  const seen = new Set<string>();
  const out: Relationship[][] = [];
  for (const r of rels) {
    if (seen.has(r.id)) continue;
    const group = siblings(r, rels);
    group.forEach((g) => seen.add(g.id));
    if (group.length > 1) out.push(group);
  }
  return out;
}

/** The pairs where some link has no name yet: the Model view asks for names. */
export const unnamedPairs = (rels: Relationship[]): Relationship[][] => pairs(rels).filter((g) => g.some((r) => !r.name?.trim()));

/** A name as the engine keeps it: no spaces at the ends, one space between words. */
export const cleanName = (s: string): string => s.split(/\s+/).filter(Boolean).join(' ');

/**
 * Why a name cannot be used, in words, or null. The same rules as the engine, so the sheet can
 * say it before the request does. `pending` holds names typed for other links in the same sheet.
 */
export function nameProblem(
  name: string,
  rel: Relationship,
  rels: Relationship[],
  tables: Named[],
  pending: Record<string, string> = {},
): string | null {
  const n = cleanName(name);
  if (!n) {
    return siblings(rel, rels).length > 1 ? 'Give this link a name. Two links join these tables.' : null;
  }
  if (n.length > MAX_NAME) return `Keep the name to ${MAX_NAME} letters or fewer.`;
  const same = (a: string) => a.trim().toLowerCase() === n.toLowerCase();
  if (tables.some((t) => same(t.label))) return `A table is called ${n}. Give the link another name.`;
  const other = rels.find((r) => r.id !== rel.id && same(cleanName(pending[r.id] ?? r.name ?? '')));
  if (other) return `Another link is called ${n}. Give this one another name.`;
  return null;
}

/** A name for each of these links, ready to type over: the ones they have, else the proposal, never twice. */
export function proposeNames(group: Relationship[], tables: Named[]): Record<string, string> {
  const out: Record<string, string> = {};
  const used = new Set<string>();
  for (const r of group) {
    const base = r.name?.trim() || defaultName(r, tables);
    let name = base;
    for (let i = 2; used.has(name.toLowerCase()); i++) name = `${base} ${i}`;
    used.add(name.toLowerCase());
    out[r.id] = name;
  }
  return out;
}

// ---- the field list ------------------------------------------------------------------------

/** One table in the field list: once, or once for each link that leads to it. */
export interface FieldGroup {
  /** Unique in the list. */
  key: string;
  table: Table;
  /** The link the fields of this group cross. Not set when only one path leads to the table. */
  via?: string;
  /** The heading: the table, or the link that leads to it. */
  title: string;
  /** When the heading is a link: the table behind it. */
  sub?: string;
}

/**
 * The tables of the field list. With a `base` (the table of the tile's first value), a table that
 * two or more links lead to is listed once for each link, headed by the name of the link:
 * "Shipped from" and "Sold at" both list the stores. Without a base, or with one path, each table
 * is listed once.
 */
export function fieldGroups(tables: Table[], rels: Relationship[], health: Health, base: string | null): FieldGroup[] {
  const out: FieldGroup[] = [];
  for (const t of tables) {
    const links = base && t.id !== base ? firstLinks(base, t.id, rels, health) : [];
    if (links.length < 2) {
      out.push({ key: t.id, table: t, title: t.label });
      continue;
    }
    for (const id of links) {
      const r = rels.find((x) => x.id === id);
      if (!r) continue;
      out.push({ key: `${t.id}\u0000${id}`, table: t, via: id, title: linkLabel(r, tables), sub: t.label });
    }
  }
  return out;
}

/** The group a field belongs to: a field with no link in a table that has several uses the first. */
export function groupOf(groups: FieldGroup[], table: string | null, via: string | undefined): FieldGroup | undefined {
  if (!table) return undefined;
  const mine = groups.filter((g) => g.table.id === table);
  return mine.find((g) => g.via === via) ?? (via === undefined ? mine[0] : undefined);
}

// ---- the names the rest of the app reads -----------------------------------------------------

const labels = new Map<string, string>();

/** Remembers what every link is called, so a title and a column heading can say "Shipped from › region". */
export function setLinkLabels(rels: Relationship[], tables: Named[]): void {
  labels.clear();
  for (const r of rels) labels.set(r.id, linkLabel(r, tables));
}

/** Remembers the names a file or a page carries with it. */
export function setLinkNames(names: Record<string, string> | undefined): void {
  labels.clear();
  for (const [id, name] of Object.entries(names ?? {})) labels.set(id, name);
}

/** What a link is called, or undefined for a link that is gone. */
export const labelOfLink = (id: string | undefined): string | undefined => (id ? labels.get(id) : undefined);

/** The field as a person reads it: "region", or "Shipped from › region" when a link leads to the table. */
export function fieldWithLink(column: string, via: string | undefined): string {
  const link = labelOfLink(via);
  return link ? `${link} › ${column}` : column;
}

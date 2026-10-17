// A workspace as a site (build 12, V4-D14): a title, a navigation of its pages (a top bar or a sidebar), a home page,
// named groups of pages, pages kept out of the navigation, a footer line, and tiles whose click opens another page
// filtered by the clicked value (drill-through, V4-D16). Off by default. Pure, with only pure imports at run time, so
// the node tests load it.

import type { Bins, Cell, Dashboard, DashFilter, GeoInfo, SiteSettings, Tile } from './api';
import { markFilter } from './filters.ts';
import { t } from './i18n.ts';

/** A page as the navigation shows it. */
export interface NavPage {
  id: string;
  title: string;
}

export interface Nav {
  /** Pages in no group, first, in the order of the dashboards. */
  pages: NavPage[];
  groups: { name: string; pages: NavPage[] }[];
}

const titleOf = (d: Pick<Dashboard, 'title'>) => d.title.trim() || t('Untitled');

/** The pages the navigation lists: every dashboard not marked hidden. A hidden page is still reached by a link. */
export const visiblePages = (dashboards: readonly Dashboard[]): Dashboard[] => dashboards.filter((d) => !d.hidden);

/** The pages a site on tenun.cloud holds at most (build 14): the service's own limit. */
export const CLOUD_SITE_PAGES = 10;

/**
 * The site settings as they are used: off unless switched on; a title (the workspace's name when blank); the top bar
 * unless the sidebar is picked; groups with only pages that exist, each page in one group at most (the first); the home
 * page a visible page, else the first visible one; a footer of one line; the publish settings as they are (build 14).
 */
export function normaliseSite(site: Partial<SiteSettings> | null | undefined, dashboards: readonly Dashboard[], workspace = 'Tenun'): SiteSettings {
  const ids = new Set(dashboards.map((d) => d.id));
  const seen = new Set<string>();
  const groups = (site?.groups ?? []).map((g) => ({
    name: (g.name ?? '').trim() || t('Group'),
    pages: (g.pages ?? []).filter((id) => ids.has(id) && !seen.has(id) && (seen.add(id), true)),
  }));
  const visible = visiblePages(dashboards);
  const home = site?.home && visible.some((d) => d.id === site.home) ? site.home : visible[0]?.id;
  return {
    on: site?.on === true,
    title: (site?.title ?? '').trim() || workspace,
    nav: site?.nav === 'side' ? 'side' : 'top',
    ...(home ? { home } : {}),
    groups,
    footer: (site?.footer ?? '').replace(/\s+/g, ' ').trim(),
    ...(site?.publish ? { publish: site.publish } : {}),
  };
}

/** The navigation: ungrouped visible pages first, then each group with its visible pages, in the group's order. */
export function navOf(site: SiteSettings, dashboards: readonly Dashboard[]): Nav {
  const byId = new Map(visiblePages(dashboards).map((d) => [d.id, d]));
  const grouped = new Set((site.groups ?? []).flatMap((g) => g.pages));
  const page = (d: Dashboard): NavPage => ({ id: d.id, title: titleOf(d) });
  return {
    pages: [...byId.values()].filter((d) => !grouped.has(d.id)).map(page),
    groups: (site.groups ?? []).map((g) => ({ name: g.name, pages: g.pages.flatMap((id) => (byId.has(id) ? [page(byId.get(id)!)] : [])) })),
  };
}

/** A group moved up or down in the list. */
export function moveGroup(site: SiteSettings, from: number, to: number): SiteSettings {
  const groups = [...(site.groups ?? [])];
  if (to < 0 || to >= groups.length || from === to) return site;
  const [g] = groups.splice(from, 1);
  groups.splice(to, 0, g);
  return { ...site, groups };
}

/** A page put in a group (by its place in the list), or taken out of every group (null). */
export function setGroup(site: SiteSettings, page: string, group: number | null): SiteSettings {
  return {
    ...site,
    groups: (site.groups ?? []).map((g, i) => ({ ...g, pages: i === group ? [...g.pages.filter((p) => p !== page), page] : g.pages.filter((p) => p !== page) })),
  };
}

// ---- drill-through ----------------------------------------------------------------------------------------------------

/** The source a filter that came with a drill-through names, so no tile of the page takes it as its own. */
export const DRILL = '\u0000drill';

/** Where a drill-through came from: the chip says "From <page>: <filter>", and Back returns there. */
export interface Drill {
  /** The page it opens. */
  page: string;
  filter: DashFilter;
  /** The page it came from. */
  from: { id: string; title: string };
}

/**
 * What a click on a mark of a tile with an action opens: the target page, keeping only the clicked value (a bar, a line
 * point, a slice, a place on a map, a table row). Null when the tile has no action, the target is gone or is this page,
 * or the click names nothing to filter by.
 */
export function drillOf(
  tile: Pick<Tile, 'type' | 'dims' | 'measures' | 'action'>,
  from: Pick<Dashboard, 'id' | 'title'>,
  dashboards: readonly Pick<Dashboard, 'id'>[],
  value: Cell,
  otherOf?: Cell[],
  bins?: Bins,
  stage?: number,
  geo?: GeoInfo,
): Drill | null {
  const page = tile.action?.page;
  if (!page || page === from.id || !dashboards.some((d) => d.id === page)) return null;
  const f = markFilter(tile, value, otherOf, bins, stage, geo);
  if (!f) return null;
  return { page, filter: { ...f, fromTile: DRILL }, from: { id: from.id, title: titleOf(from) } };
}

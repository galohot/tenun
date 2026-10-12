// An exported site (build 13, V4-D15, V4-D16): which page the address names, and the address of a page or of a
// drill-through. Pure, with no imports, so the node tests load it.

/** What the address shows: a page, or a page opened by a click on a mark of a tile (a drill-through). */
export interface Route {
  page: string;
  drill?: { tile: string; key: string };
}

/**
 * The route an address names: `#/p/<page>` or `#/p/<page>/d/<tile>/<value key>`. An address that names no page of the
 * site (or nothing at all) shows the home page.
 */
export function parseHash(hash: string, pages: readonly string[], home: string): Route {
  const m = /^#\/p\/([^/]+)(?:\/d\/([^/]+)\/(.*))?$/.exec(hash);
  if (!m) return { page: home };
  const page = decodeURIComponent(m[1]);
  if (!pages.includes(page)) return { page: home };
  if (m[2] === undefined) return { page };
  return { page, drill: { tile: decodeURIComponent(m[2]), key: decodeURIComponent(m[3]) } };
}

/** The address of a route, so back and forward move between pages and drill-throughs. */
export function hashOf(r: Route): string {
  const page = `#/p/${encodeURIComponent(r.page)}`;
  return r.drill ? `${page}/d/${encodeURIComponent(r.drill.tile)}/${encodeURIComponent(r.drill.key)}` : page;
}

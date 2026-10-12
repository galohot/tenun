// TopoJSON to GeoJSON for the map tile (build 06), with no dependency. A TopoJSON file keeps each border once as
// an "arc" of whole-number steps; a shape lists the arcs around it. Decoding turns the steps back into longitude
// and latitude and joins the arcs of each ring. Only what Tenun's files use is read: polygons and multipolygons,
// quantized or not.

export interface Topology {
  type: 'Topology';
  transform?: { scale: [number, number]; translate: [number, number] };
  arcs: number[][][];
  objects: Record<string, { type: 'GeometryCollection'; geometries: TopoGeometry[] }>;
}

export type TopoGeometry =
  | { type: 'Polygon'; arcs: number[][]; properties?: Record<string, unknown> }
  | { type: 'MultiPolygon'; arcs: number[][][]; properties?: Record<string, unknown> }
  | { type: null; properties?: Record<string, unknown> };

export interface Feature {
  type: 'Feature';
  properties: Record<string, unknown>;
  geometry: { type: 'Polygon'; coordinates: number[][][] } | { type: 'MultiPolygon'; coordinates: number[][][][] } | null;
}

export interface FeatureCollection {
  type: 'FeatureCollection';
  features: Feature[];
}

/** Every arc as points of longitude and latitude. */
function decodeArcs(t: Topology): number[][][] {
  const tf = t.transform;
  return t.arcs.map((arc) => {
    if (!tf) return arc.map((p) => [p[0], p[1]]);
    let x = 0;
    let y = 0;
    return arc.map((p) => {
      x += p[0];
      y += p[1];
      return [x * tf.scale[0] + tf.translate[0], y * tf.scale[1] + tf.translate[1]];
    });
  });
}

/** A ring from its arcs. A negative index `~i` is arc `i` read backwards. Each arc after the first drops its first point, which is the last of the one before. */
function ring(ids: number[], arcs: number[][][]): number[][] {
  const out: number[][] = [];
  ids.forEach((id, n) => {
    const a = id < 0 ? [...arcs[~id]].reverse() : arcs[id];
    out.push(...(n === 0 ? a : a.slice(1)));
  });
  return out;
}

/** The features of one object of a topology (the first one when no name is given). */
export function features(t: Topology, name?: string): FeatureCollection {
  const obj = name ? t.objects[name] : Object.values(t.objects)[0];
  const arcs = decodeArcs(t);
  return {
    type: 'FeatureCollection',
    features: (obj?.geometries ?? []).map((g) => ({
      type: 'Feature',
      properties: g.properties ?? {},
      geometry:
        g.type === 'Polygon'
          ? { type: 'Polygon', coordinates: g.arcs.map((r) => ring(r, arcs)) }
          : g.type === 'MultiPolygon'
            ? { type: 'MultiPolygon', coordinates: g.arcs.map((p) => p.map((r) => ring(r, arcs))) }
            : null,
    })),
  };
}

/** The box around some features, as `[[west, north], [east, south]]` (what an ECharts map's `boundingCoords` takes). */
export function bounds(fs: readonly Feature[]): [[number, number], [number, number]] | null {
  let w = Infinity;
  let e = -Infinity;
  let s = Infinity;
  let n = -Infinity;
  const visit = (pts: number[][]) => {
    for (const [x, y] of pts) {
      if (x < w) w = x;
      if (x > e) e = x;
      if (y < s) s = y;
      if (y > n) n = y;
    }
  };
  for (const f of fs) {
    const g = f.geometry;
    if (!g) continue;
    if (g.type === 'Polygon') g.coordinates.forEach(visit);
    else g.coordinates.forEach((p) => p.forEach(visit));
  }
  return Number.isFinite(w) ? [[w, n], [e, s]] : null;
}

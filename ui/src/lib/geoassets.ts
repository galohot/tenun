// Where a map's shapes come from (build 06): the app's own files (`/geo/*.topo.json`), a file the engine
// downloaded for another country (`/api/geo/{country}/{level}`), or what an exported file carries. Each map is
// decoded and registered with ECharts once.

import type { MapLevel } from './api';
import { echarts } from './echarts';
import { features, type Feature, type FeatureCollection, type Topology } from './topo';

/** Shapes an exported file carries: the TopoJSON of the bundled maps by file name, GeoJSON of downloaded ones. */
let carried: Record<string, unknown> | null = null;

/** An exported file hands over the maps it carries, so nothing is fetched. */
export function useCarried(geo: Record<string, unknown> | null | undefined): void {
  carried = geo ?? null;
}

export interface Shapes {
  /** The name the shapes are registered under with ECharts. */
  mapName: string;
  shapes: Feature[];
  /** A kabupaten map: the provinces drawn over it. */
  borders: { mapName: string } | null;
  /** The credit line for the tile's footer. */
  credit: string;
}

/** The file a bundled level is in, or null for another country's provinces. */
export function fileOf(level: MapLevel, country: string): string | null {
  if (level === 'country') return 'world';
  if (country.toUpperCase() === 'ID') return level === 'province' ? 'ID-1' : 'ID-2';
  return null;
}

/** The key under which an exported file carries a downloaded map. */
export const downloadedKey = (country: string, level: MapLevel) => `${country.toUpperCase()}-${level}`;

const NATURAL_EARTH = 'Boundaries: Natural Earth';
const GEOBOUNDARIES_ID = 'Boundaries: geoBoundaries (CC BY 3.0 IGO), BPS; codes: Kemendagri';
const GEOBOUNDARIES = 'Boundaries: geoBoundaries';

const loaded = new Map<string, Promise<Feature[]>>();

async function loadTopo(file: string): Promise<Feature[]> {
  const fromFile = carried?.[file] as Topology | undefined;
  const topo: Topology = fromFile ?? (await (await fetch(`/geo/${file}.topo.json`)).json());
  return features(topo).features;
}

/** The code of a feature of a geoBoundaries file: its ISO code, or its name (the engine reads it the same way, geo.rs). */
export function downloadedCode(p: Record<string, unknown>): string | null {
  const iso = String(p.shapeISO ?? '').trim();
  const name = String(p.shapeName ?? '').trim();
  return iso || name || null;
}

async function loadDownloaded(country: string, level: MapLevel): Promise<Feature[]> {
  const key = downloadedKey(country, level);
  const fromFile = carried?.[key] as FeatureCollection | undefined;
  const gj: FeatureCollection =
    fromFile ?? (await (async () => {
      const r = await fetch(`/api/geo/${encodeURIComponent(country)}/${level}`, { headers: { 'X-Tenun': '1' } });
      if (!r.ok) throw new Error((await r.json().catch(() => null))?.error ?? `The map of ${country} could not be downloaded.`);
      return r.json();
    })());
  const out: Feature[] = [];
  for (const f of gj.features) {
    const code = downloadedCode(f.properties);
    if (code) out.push({ ...f, properties: { ...f.properties, code, name: String(f.properties.shapeName ?? code) } });
  }
  return out;
}

function once(key: string, load: () => Promise<Feature[]>): Promise<Feature[]> {
  let p = loaded.get(key);
  if (!p) {
    p = load().then((fs) => {
      echarts.registerMap(`tenun:${key}`, { type: 'FeatureCollection', features: fs } as never);
      return fs;
    });
    // A failed download can be tried again.
    p.catch(() => loaded.delete(key));
    loaded.set(key, p);
  }
  return p;
}

/** The shapes of a map, registered with ECharts. A kabupaten map of Indonesia also gets the provinces. */
export async function shapesFor(level: MapLevel, country = 'ID'): Promise<Shapes> {
  const file = fileOf(level, country);
  if (file) {
    const shapes = await once(file, () => loadTopo(file));
    let borders: Shapes['borders'] = null;
    if (file === 'ID-2') {
      await once('ID-1', () => loadTopo('ID-1'));
      borders = { mapName: 'tenun:ID-1' };
    }
    return { mapName: `tenun:${file}`, shapes, borders, credit: file === 'world' ? NATURAL_EARTH : GEOBOUNDARIES_ID };
  }
  const key = downloadedKey(country, level);
  const shapes = await once(key, () => loadDownloaded(country, level));
  return { mapName: `tenun:${key}`, shapes, borders: null, credit: GEOBOUNDARIES };
}

/** The outline under a points map: the provinces of Indonesia, or the world. */
export async function outlineFor(country?: string): Promise<Shapes> {
  return shapesFor(country?.toUpperCase() === 'ID' ? 'province' : 'country', 'ID');
}

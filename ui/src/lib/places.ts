// The standard place names a map reads (build 05, V4-D8): the lists of the Place names sheet and their CSV. Pure
// functions with no imports at run time, so the node tests can load them. The engine matches (geo.rs).

import type { Plain } from './tabular';

export type PlaceLevel = 'country' | 'province' | 'regency';

/** One place of the gazetteer, as `GET /api/geo/places` sends it. */
export interface PlaceRow {
  code: string;
  name: string;
  aliases: string[];
  parent?: string;
}

/** The lists the sheet offers: the world's countries, and the provinces and kabupaten/kota of Indonesia. */
export const PLACE_LISTS: { level: PlaceLevel; country: string; title: string; codeWords: string }[] = [
  { level: 'country', country: '', title: 'Countries', codeWords: 'ISO 3166-1 alpha-2; the alpha-3 code is read too' },
  { level: 'province', country: 'ID', title: 'Provinces of Indonesia', codeWords: 'ISO 3166-2, with or without ID-; the Kemendagri number is read too' },
  { level: 'regency', country: 'ID', title: 'Kabupaten and kota of Indonesia', codeWords: 'Kemendagri, with or without the dot' },
];

/** A word as the search compares it: lower case, no dots, dashes or spaces. */
const flat = (s: string) => s.toLowerCase().replace(/[.\-'’\s]+/g, '');

/** The places whose code, name or another form holds every word typed. */
export function searchPlaces(rows: readonly PlaceRow[], q: string): PlaceRow[] {
  const words = q.split(/\s+/).map(flat).filter(Boolean);
  if (!words.length) return [...rows];
  return rows.filter((r) => {
    const hay = flat([r.code, r.name, ...r.aliases, r.parent ?? ''].join(' '));
    return words.every((w) => hay.includes(w));
  });
}

/** The table of the CSV download: code, name, the other forms Tenun reads, and the place it is in. */
export function placesTable(rows: readonly PlaceRow[]): Plain {
  return {
    heads: ['code', 'name', 'also read as', 'in'],
    rows: rows.map((r) => [r.code, r.name, r.aliases.join('; '), r.parent ?? '']),
    total: null,
    numeric: [false, false, false, false],
  };
}

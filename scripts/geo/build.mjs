// Builds the map boundaries and the place names of Tenun (build 05, V4-D8, V4-D9) from public sources:
//
//   ui/public/geo/world.topo.json   countries (Natural Earth admin-0, 1:50m, public domain)
//   ui/public/geo/ID-1.topo.json    the 38 provinces of Indonesia (the kabupaten/kota below, dissolved by province)
//   ui/public/geo/ID-2.topo.json    the 514 kabupaten/kota (geoBoundaries gbOpen IDN ADM2, CC BY 3.0 IGO)
//   crates/tenun-core/src/geo/places.tsv   the gazetteer: level, country, code, name, aliases, parent
//
// Codes and official names of Indonesia come from cahyadsn/wilayah (MIT), which follows the Kepmendagri of 2025.
// Every source is pinned (a release tag or a commit), downloaded once into `.cache/`, and listed in SOURCES.md.
// Run: `npm install && npm run build` in this folder. The output is committed, so a build of Tenun needs no network.

import { mkdir, readFile, writeFile, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import mapshaper from 'mapshaper';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..', '..');
const CACHE = join(HERE, '.cache');
const OUT = join(ROOT, 'ui', 'public', 'geo');
const TSV = join(ROOT, 'crates', 'tenun-core', 'src', 'geo', 'places.tsv');

export const SOURCES = {
  world: {
    url: 'https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_50m_admin_0_countries.geojson',
    file: 'ne_50m_admin_0_countries.geojson',
  },
  idn2: {
    url: 'https://github.com/wmgeolab/geoBoundaries/raw/9469f09/releaseData/gbOpen/IDN/ADM2/geoBoundaries-IDN-ADM2.topojson',
    file: 'geoBoundaries-IDN-ADM2.topojson',
  },
  wilayah: {
    url: 'https://raw.githubusercontent.com/cahyadsn/wilayah/7586f4278ecbcbabd4b65f8d594913e9d256a29a/db/wilayah.sql',
    file: 'wilayah.sql',
  },
};

async function fetchOnce({ url, file }) {
  const path = join(CACHE, file);
  if (existsSync(path)) return path;
  await mkdir(CACHE, { recursive: true });
  console.log(`download ${url}`);
  const r = await fetch(url, { redirect: 'follow' });
  if (!r.ok) throw new Error(`${url}: ${r.status}`);
  await writeFile(path, Buffer.from(await r.arrayBuffer()));
  return path;
}

// ---- names and codes --------------------------------------------------------------------------

/**
 * A name as the matching compares it: lower case, no dots, dashes or apostrophes, one space, no "administrasi",
 * and "Kep." written out as "Kepulauan" (the official list writes both). geo.rs reads names the same way.
 */
export const norm = (s) =>
  s
    .toLowerCase()
    .replace(/[.'’\-]/g, ' ')
    .replace(/\badministrasi\b/g, ' ')
    .replace(/\bkep\b/g, 'kepulauan')
    .replace(/\s+/g, ' ')
    .trim();
/** The same with no spaces at all: "Banyu Asin" and "Banyuasin" are one name. */
export const squash = (s) => norm(s).replace(/ /g, '');

/** Shapes of geoBoundaries that are not a kabupaten or kota: lakes, reservoirs and a forest. They are left out. */
const NOT_A_PLACE = new Set(['Danau', 'Danau Toba', 'Waduk Cirata', 'Wadung Kedungombo', 'Hutan']);

/** geoBoundaries (2020) names whose kabupaten was renamed since, or that read as another kind of place. */
const RENAMED = {
  'Kota Baru': 'Kabupaten Kotabaru', // a kabupaten in Kalimantan Selatan, not a kota
  'Mahakam Hulu': 'Kabupaten Mahakam Ulu',
  'Maluku Tenggara Barat': 'Kabupaten Kepulauan Tanimbar', // renamed in 2019
  'Mamuju Utara': 'Kabupaten Pasangkayu', // renamed in 2017
  'Siau Tagulandang Biaro': 'Kabupaten Kepulauan Siau Tagulandang Biaro',
  'Toba Samosir': 'Kabupaten Toba', // renamed in 2020
};

/** Forms people type that the rules (case, dots, spaces, Kab./Kota) do not reach. */
const EXTRA_ALIASES = {
  'Kota Palangkaraya': ['Palangka Raya', 'Kota Palangka Raya'],
  'Kota Pangkal Pinang': ['Pangkalpinang', 'Kota Pangkalpinang'],
  'Kota Bau Bau': ['Baubau', 'Kota Baubau', 'Bau-Bau'],
  'Kabupaten Administrasi Kepulauan Seribu': ['Kepulauan Seribu', 'Kabupaten Kepulauan Seribu'],
  'Kota Administrasi Jakarta Pusat': ['Jakarta Pusat', 'Kota Jakarta Pusat', 'Jakpus'],
  'Kota Administrasi Jakarta Utara': ['Jakarta Utara', 'Kota Jakarta Utara', 'Jakut'],
  'Kota Administrasi Jakarta Barat': ['Jakarta Barat', 'Kota Jakarta Barat', 'Jakbar'],
  'Kota Administrasi Jakarta Selatan': ['Jakarta Selatan', 'Kota Jakarta Selatan', 'Jaksel'],
  'Kota Administrasi Jakarta Timur': ['Jakarta Timur', 'Kota Jakarta Timur', 'Jaktim'],
};

/**
 * The 38 provinces: Kemendagri code → ISO 3166-2 code, and the forms people use (short names, English names).
 * ISO 3166-2:ID added ID-PS, ID-PT, ID-PE and ID-PD for the four new Papua provinces (2022).
 */
export const PROVINCES = {
  11: ['ID-AC', ['Aceh', 'NAD', 'Nanggroe Aceh Darussalam']],
  12: ['ID-SU', ['Sumut', 'North Sumatra', 'North Sumatera']],
  13: ['ID-SB', ['Sumbar', 'West Sumatra', 'West Sumatera']],
  14: ['ID-RI', ['Riau']],
  15: ['ID-JA', ['Jambi']],
  16: ['ID-SS', ['Sumsel', 'South Sumatra', 'South Sumatera']],
  17: ['ID-BE', ['Bengkulu']],
  18: ['ID-LA', ['Lampung']],
  19: ['ID-BB', ['Bangka Belitung', 'Babel', 'Bangka Belitung Islands', 'Kep. Bangka Belitung']],
  21: ['ID-KR', ['Kepri', 'Riau Islands', 'Kep. Riau']],
  31: ['ID-JK', ['DKI Jakarta', 'Jakarta', 'DKJ', 'Daerah Khusus Jakarta']],
  32: ['ID-JB', ['Jabar', 'West Java']],
  33: ['ID-JT', ['Jateng', 'Central Java']],
  34: ['ID-YO', ['DIY', 'DI Yogyakarta', 'Yogyakarta', 'Jogja', 'Jogjakarta', 'Special Region of Yogyakarta']],
  35: ['ID-JI', ['Jatim', 'East Java']],
  36: ['ID-BT', ['Banten']],
  51: ['ID-BA', ['Bali']],
  52: ['ID-NB', ['NTB', 'West Nusa Tenggara']],
  53: ['ID-NT', ['NTT', 'East Nusa Tenggara']],
  61: ['ID-KB', ['Kalbar', 'West Kalimantan']],
  62: ['ID-KT', ['Kalteng', 'Central Kalimantan']],
  63: ['ID-KS', ['Kalsel', 'South Kalimantan']],
  64: ['ID-KI', ['Kaltim', 'East Kalimantan']],
  65: ['ID-KU', ['Kaltara', 'North Kalimantan']],
  71: ['ID-SA', ['Sulut', 'North Sulawesi']],
  72: ['ID-ST', ['Sulteng', 'Central Sulawesi']],
  73: ['ID-SN', ['Sulsel', 'South Sulawesi']],
  74: ['ID-SG', ['Sultra', 'Southeast Sulawesi']],
  75: ['ID-GO', ['Gorontalo']],
  76: ['ID-SR', ['Sulbar', 'West Sulawesi']],
  81: ['ID-MA', ['Maluku']],
  82: ['ID-MU', ['Malut', 'North Maluku']],
  91: ['ID-PA', ['Papua']],
  92: ['ID-PB', ['Papua Barat', 'West Papua', 'Pabar']],
  93: ['ID-PS', ['Papua Selatan', 'South Papua']],
  94: ['ID-PT', ['Papua Tengah', 'Central Papua']],
  95: ['ID-PE', ['Papua Pegunungan', 'Highland Papua']],
  96: ['ID-PD', ['Papua Barat Daya', 'Southwest Papua']],
};

/** The provinces and kabupaten/kota of `wilayah.sql`: `[code, name]` pairs. */
export function parseWilayah(sql) {
  const prov = [...sql.matchAll(/\('(\d{2})','([^']*)'\)/g)].map((m) => [m[1], m[2]]);
  const kab = [...sql.matchAll(/\('(\d{2}\.\d{2})','([^']*)'\)/g)].map((m) => [m[1], m[2]]);
  return { prov, kab };
}

/** The Kemendagri code of each geoBoundaries shape, by its name; `null` for a shape that is not a place. */
export function codeOfShape(name, kab) {
  if (NOT_A_PLACE.has(name)) return null;
  const by = new Map(kab.map(([c, n]) => [squash(n), c]));
  const wanted = RENAMED[name] ?? (name.startsWith('Kota ') ? name : `Kabupaten ${name}`);
  const code = by.get(squash(wanted));
  if (!code) throw new Error(`no code for the shape "${name}" (looked for "${wanted}")`);
  return code;
}

// ---- the build ------------------------------------------------------------------------------

async function run(cmd, inputs) {
  return mapshaper.applyCommands(cmd, inputs);
}

async function main() {
  const [worldPath, idnPath, wilayahPath] = await Promise.all([fetchOnce(SOURCES.world), fetchOnce(SOURCES.idn2), fetchOnce(SOURCES.wilayah)]);
  const { prov, kab } = parseWilayah(await readFile(wilayahPath, 'utf8'));
  if (prov.length !== 38 || kab.length !== 514) throw new Error(`wilayah: ${prov.length} provinces, ${kab.length} kabupaten/kota`);
  const kabName = new Map(kab);
  const provName = new Map(prov);

  // Indonesia, level 2: name each shape with its code, drop the lakes, then simplify. The source is TopoJSON, so the
  // borders two kabupaten share are one line and stay one line when simplified: no gaps, no overlaps.
  const topo = JSON.parse(await readFile(idnPath, 'utf8'));
  const obj = Object.values(topo.objects)[0];
  const seen = new Set();
  const gbName = new Map();
  obj.geometries = obj.geometries.filter((g) => {
    const name = g.properties.shapeName;
    const code = codeOfShape(name, kab);
    if (!code) return false;
    if (seen.has(code)) throw new Error(`two shapes for ${code}`);
    seen.add(code);
    gbName.set(code, name);
    g.properties = { code, name: kabName.get(code), parent: PROVINCES[code.slice(0, 2)][0] };
    return true;
  });
  if (seen.size !== 514) throw new Error(`${seen.size} shapes have a code; 514 are wanted`);
  const idnIn = { 'idn.json': JSON.stringify(topo) };
  const simplify = `-simplify visvalingam weighted ${process.env.KEEP2 ?? '3%'} keep-shapes`;
  const simplify1 = `-simplify visvalingam weighted ${process.env.KEEP1 ?? '2%'} keep-shapes`;
  const out2 = await run(`-i idn.json ${simplify} -clean -o ID-2.topo.json format=topojson quantization=60000 precision=0.0001`, idnIn);
  const out1 = await run(
    `-i idn.json ${simplify1} -clean -dissolve parent -rename-fields code=parent -o ID-1.topo.json format=topojson quantization=60000 precision=0.0001`,
    idnIn,
  );
  const id1 = JSON.parse(String(out1['ID-1.topo.json']));
  const iso = new Map(Object.entries(PROVINCES).map(([k, [code]]) => [code, k]));
  for (const g of Object.values(id1.objects)[0].geometries) g.properties = { code: g.properties.code, name: provName.get(iso.get(g.properties.code)) };
  if (Object.values(id1.objects)[0].geometries.length !== 38) throw new Error('the provinces are not 38');

  // The world: one shape per country, with its ISO codes. Natural Earth writes -99 where ISO has no code for a shape it
  // draws apart (France with its overseas parts, Norway, Kosovo); its `_EH` fields hold the code to use. Three shapes have
  // no ISO code at all (Somaliland, Northern Cyprus, Siachen Glacier); they are drawn as part of the country Natural
  // Earth's Indonesian point of view gives them (`ADM0_A3_ID`), so the map has no hole. No other shape moves.
  const world = JSON.parse(await readFile(worldPath, 'utf8'));
  const countries = [];
  const a2of = new Map();
  const codesOf = (p) => {
    // A code is two capital letters; Natural Earth writes `CN-TW` for Taiwan in ISO_A2 and TW in ISO_A2_EH.
    const two = (x) => (/^[A-Z]{2}$/.test(x ?? '') ? x : null);
    const three = (x) => (/^[A-Z]{3}$/.test(x ?? '') ? x : null);
    const a2 = two(p.ISO_A2) ?? two(p.ISO_A2_EH);
    // Kosovo has no ISO alpha-3 code; geoBoundaries files it as XKX.
    const a3 = three(p.ISO_A3) ?? three(p.ISO_A3_EH) ?? (a2 === 'XK' ? 'XKX' : p.ADM0_A3);
    return { a2, a3 };
  };
  for (const f of world.features) {
    const { a2, a3 } = codesOf(f.properties);
    if (a2) a2of.set(a3, a2);
  }
  const joined = [];
  for (const f of world.features) {
    const p = f.properties;
    const { a2, a3 } = codesOf(p);
    if (a2) {
      // One row per ISO code: Australia's code also covers Ashmore and Cartier Islands and the Indian Ocean Territories,
      // which are drawn as part of it. The row is the shape whose own code is the country's (ADM0_A3 = ISO alpha-3).
      const row = { a2, a3, en: p.NAME_EN, id: p.NAME_ID, other: [p.NAME, p.NAME_LONG, p.FORMAL_EN, p.NAME_SORT].filter(Boolean) };
      const at = countries.findIndex((c) => c.a2 === a2);
      if (at < 0) countries.push(row);
      else if (p.ADM0_A3 === a3) countries[at] = row;
      f.properties = { code: a2, name: p.NAME_EN };
    } else {
      const to = a2of.get(p.ADM0_A3_ID);
      if (!to) throw new Error(`no country for ${p.NAME_EN}`);
      joined.push(`${p.NAME_EN} → ${to}`);
      f.properties = { code: to, name: '' };
    }
  }
  console.log('drawn as part of a country:', joined.join(', '));
  const nameOf = new Map(countries.map((c) => [c.a2, c.en]));
  const outW = await run('-i world.json -dissolve code -simplify visvalingam weighted 25% keep-shapes -o world.topo.json format=topojson quantization=60000 precision=0.0001', {
    'world.json': JSON.stringify(world),
  });
  const w = JSON.parse(String(outW['world.topo.json']));
  for (const g of Object.values(w.objects)[0].geometries) g.properties = { code: g.properties.code, name: nameOf.get(g.properties.code) };
  outW['world.topo.json'] = JSON.stringify(w);

  await mkdir(OUT, { recursive: true });
  await writeFile(join(OUT, 'world.topo.json'), outW['world.topo.json']);
  await writeFile(join(OUT, 'ID-1.topo.json'), JSON.stringify(id1));
  await writeFile(join(OUT, 'ID-2.topo.json'), out2['ID-2.topo.json']);

  // The gazetteer. Aliases are separated by `|`. A name the rules of `geo.rs` already match is not repeated.
  const rows = [['level', 'country', 'code', 'name', 'aliases', 'parent']];
  const clean = (list, name) => [...new Set(list.filter((a) => a && squash(a) !== squash(name)))].join('|');
  for (const c of countries.sort((a, b) => a.a2.localeCompare(b.a2))) rows.push(['0', c.a2, c.a2, c.en, clean([c.a3, c.id, ...c.other], c.en), '']);
  for (const [k, [code, aliases]] of Object.entries(PROVINCES)) {
    const name = provName.get(k);
    rows.push(['1', 'ID', code, name, clean([k, ...aliases], name), 'ID']);
  }
  for (const [code, name] of kab) {
    const old = gbName.get(code);
    const more = EXTRA_ALIASES[name] ?? [];
    rows.push(['2', 'ID', code, name, clean([old && (old.startsWith('Kota ') ? old : `Kabupaten ${old}`), ...more], name), PROVINCES[code.slice(0, 2)][0]]);
  }
  await mkdir(dirname(TSV), { recursive: true });
  await writeFile(TSV, rows.map((r) => r.join('\t')).join('\n') + '\n');

  for (const f of ['world.topo.json', 'ID-1.topo.json', 'ID-2.topo.json']) console.log(f, (await stat(join(OUT, f))).size, 'bytes');
  console.log('places.tsv', rows.length - 1, 'rows');
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();

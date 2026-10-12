//! Place names and where boundaries come from (build 05, V4-D8, V4-D9, V4-D19).
//!
//! The gazetteer (`geo/places.tsv`, made by `scripts/geo/build.mjs`) lists every country (ISO 3166-1), the 38
//! provinces of Indonesia (ISO 3166-2) and its 514 kabupaten and kota (Kemendagri codes), with the forms people
//! type. `match_place` reads a cell of data as one of them. The shapes themselves are TopoJSON files in the UI
//! (`ui/public/geo/`): this module holds no geometry. Provinces and second levels of other countries are
//! downloaded once from geoBoundaries and kept in the config folder (`boundaries`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

/// The level of a place on a map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// A country (ISO 3166-1).
    Country,
    /// The first level inside a country: a province of Indonesia (ISO 3166-2).
    Province,
    /// The second level: a kabupaten or kota of Indonesia (Kemendagri code).
    Regency,
}

impl Level {
    fn of_digit(d: &str) -> Option<Level> {
        match d {
            "0" => Some(Level::Country),
            "1" => Some(Level::Province),
            "2" => Some(Level::Regency),
            _ => None,
        }
    }

    /// The level as geoBoundaries names it.
    pub fn adm(self) -> &'static str {
        match self {
            Level::Country => "ADM0",
            Level::Province => "ADM1",
            Level::Regency => "ADM2",
        }
    }
}

/// One row of the gazetteer.
#[derive(Debug, Clone, Serialize)]
pub struct Place {
    pub level: Level,
    /// ISO 3166-1 alpha-2 of the country the place is in (of itself, for a country).
    pub country: String,
    /// The standard code: ISO alpha-2 for a country, ISO 3166-2 for a province, Kemendagri for a kabupaten/kota.
    pub code: String,
    /// The standard name: English for a country, the official Indonesian name below it.
    pub name: String,
    /// Other forms people write: an ISO alpha-3 code (first, for a country), the Indonesian name of a country,
    /// short and English names of a province, older names of a kabupaten.
    pub aliases: Vec<String>,
    /// The code of the place it is in: the province of a kabupaten/kota, the country of a province.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub parent: String,
}

const TSV: &str = include_str!("geo/places.tsv");

/// Every place, in the order of the file: countries by code, then the provinces, then the kabupaten and kota.
pub fn places() -> &'static [Place] {
    static PLACES: LazyLock<Vec<Place>> = LazyLock::new(|| {
        TSV.lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                Some(Place {
                    level: Level::of_digit(f.first()?)?,
                    country: f.get(1)?.to_string(),
                    code: f.get(2)?.to_string(),
                    name: f.get(3)?.to_string(),
                    aliases: f
                        .get(4)
                        .map(|a| {
                            a.split('|')
                                .filter(|x| !x.is_empty())
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    parent: f.get(5).map(|p| p.to_string()).unwrap_or_default(),
                })
            })
            .collect()
    });
    &PLACES
}

/// The places of one level of one country, in the order of the file (for a country, `country` is ignored).
pub fn list(level: Level, country: &str) -> Vec<&'static Place> {
    places()
        .iter()
        .filter(|p| {
            p.level == level && (level == Level::Country || p.country.eq_ignore_ascii_case(country))
        })
        .collect()
}

/// A name as the matching reads it (V4-D8): lower case; dots, dashes and apostrophes are spaces; one space;
/// "administrasi" goes; "kep" is "kepulauan"; "kab"/"kabupaten" is "kabupaten"; "kota"/"kodya"/"kotamadya" is
/// "kota"; "provinsi"/"prov" in front goes. `scripts/geo/build.mjs` reads names the same way.
pub fn norm(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let spaced: String = lower
        .chars()
        .map(|c| {
            if matches!(c, '.' | '-' | '\'' | '’' | '_' | ',') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut words: Vec<&str> = spaced.split_whitespace().collect();
    words.retain(|w| *w != "administrasi");
    let mut out: Vec<String> = words
        .iter()
        .map(|w| match *w {
            "kep" => "kepulauan".to_string(),
            "kab" => "kabupaten".to_string(),
            "kodya" | "kotamadya" => "kota".to_string(),
            other => other.to_string(),
        })
        .collect();
    if matches!(
        out.first().map(String::as_str),
        Some("provinsi" | "prov" | "propinsi")
    ) {
        out.remove(0);
    }
    out.join(" ")
}

/// The key of a name: `norm` without any space, so "Banyu Asin" and "Banyuasin" are one.
fn key(raw: &str) -> String {
    norm(raw).replace(' ', "")
}

/// What a cell of data is, as a place.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Matched {
    /// The code of the place.
    One { code: String },
    /// A name that two places answer to, as a bare "Bandung" (Kabupaten Bandung and Kota Bandung). Their names,
    /// for the hint.
    Ambiguous { names: Vec<String> },
    /// No place is called so.
    None,
}

impl Matched {
    pub fn code(&self) -> Option<&str> {
        match self {
            Matched::One { code } => Some(code),
            _ => None,
        }
    }

    /// What to write instead, for an ambiguous name: "Write Kabupaten Bandung or Kota Bandung."
    pub fn hint(&self) -> Option<String> {
        match self {
            Matched::Ambiguous { names } => {
                let (last, first) = names.split_last()?;
                Some(if first.is_empty() {
                    format!("Write {last}.")
                } else {
                    format!("Write {} or {last}.", first.join(", "))
                })
            }
            _ => None,
        }
    }
}

/// The index of the gazetteer: (level, country, key) → the places. `bare` holds the kabupaten and kota under
/// their names without "Kabupaten"/"Kota", where one name may be two places.
struct Index {
    full: HashMap<(Level, String, String), Vec<usize>>,
    bare: HashMap<(Level, String, String), Vec<usize>>,
}

fn index() -> &'static Index {
    static INDEX: LazyLock<Index> = LazyLock::new(|| {
        let mut full: HashMap<(Level, String, String), Vec<usize>> = HashMap::new();
        let mut bare: HashMap<(Level, String, String), Vec<usize>> = HashMap::new();
        for (i, p) in places().iter().enumerate() {
            let country = if p.level == Level::Country {
                String::new()
            } else {
                p.country.to_ascii_uppercase()
            };
            let mut put = |k: String| {
                if k.is_empty() {
                    return;
                }
                let slot = full.entry((p.level, country.clone(), k)).or_default();
                if !slot.contains(&i) {
                    slot.push(i);
                }
            };
            put(key(&p.code));
            put(key(&p.name));
            for a in &p.aliases {
                put(key(a));
            }
            match p.level {
                // ISO 3166-2 with or without "ID-".
                Level::Province => {
                    if let Some(short) = p.code.split_once('-').map(|(_, s)| s) {
                        put(key(short));
                    }
                }
                // "32.04" and "3204" are one code; a key drops the dot already. The name and the older names
                // are also known without "Kabupaten"/"Kota".
                Level::Regency => {
                    for n in std::iter::once(&p.name).chain(&p.aliases) {
                        for prefix in ["kabupaten ", "kota "] {
                            if let Some(rest) = norm(n).strip_prefix(prefix) {
                                let slot = bare
                                    .entry((p.level, country.clone(), rest.replace(' ', "")))
                                    .or_default();
                                if !slot.contains(&i) {
                                    slot.push(i);
                                }
                            }
                        }
                    }
                }
                Level::Country => {}
            }
        }
        Index { full, bare }
    });
    &INDEX
}

/// Reads a cell of data as a place of `level` in `country` (an ISO alpha-2 code; ignored for countries) by the
/// rules of V4-D8: case, spaces and dots do not count; "Kab."/"Kab"/"Kabupaten" and "Kota"/"Kodya" are one;
/// codes with or without the dot; ISO alpha-2 or alpha-3 for a country; ISO 3166-2 with or without "ID-"; and the
/// aliases of the gazetteer. A bare name that is both a kabupaten and a kota is ambiguous. A typo is no match.
pub fn match_place(level: Level, country: &str, raw: &str) -> Matched {
    let k = key(raw);
    if k.is_empty() {
        return Matched::None;
    }
    let country = if level == Level::Country {
        String::new()
    } else {
        country.to_ascii_uppercase()
    };
    let idx = index();
    let found = idx
        .full
        .get(&(level, country.clone(), k.clone()))
        .filter(|v| !v.is_empty())
        .or_else(|| idx.bare.get(&(level, country, k)));
    match found.map(Vec::as_slice) {
        Some([i]) => Matched::One {
            code: places()[*i].code.clone(),
        },
        Some(many) if !many.is_empty() => Matched::Ambiguous {
            names: many.iter().map(|i| places()[*i].name.clone()).collect(),
        },
        _ => Matched::None,
    }
}

/// The ISO alpha-3 code of a country, from its alpha-2 code: what geoBoundaries names it by.
pub fn iso3(country: &str) -> Option<&'static str> {
    list(Level::Country, "")
        .into_iter()
        .find(|p| p.code.eq_ignore_ascii_case(country))?
        .aliases
        .first()
        .map(String::as_str)
        .filter(|a| a.len() == 3 && a.chars().all(|c| c.is_ascii_uppercase()))
}

/// The countries whose provinces and second level ship with Tenun.
pub const BUNDLED: &[&str] = &["ID"];

/// Where geoBoundaries answers which file to download.
pub const GEOBOUNDARIES: &str = "https://www.geoboundaries.org/api/current/gbOpen";

/// The largest boundary file Tenun downloads.
const MAX_BYTES: u64 = 80 * 1024 * 1024;

/// The boundaries of `level` in `country` (alpha-2), as a GeoJSON file in `dir`: downloaded from geoBoundaries
/// the first time (V4-D9, V4-D19), then read from the copy. `api` is the geoBoundaries API (`GEOBOUNDARIES`; a test
/// gives its own). The words of an error say what happened, without a link that holds anything private (there is
/// none: the request has no key and no cookie).
pub fn boundaries(country: &str, level: Level, dir: &Path, api: &str) -> anyhow::Result<PathBuf> {
    if level == Level::Country {
        anyhow::bail!("Countries ship with Tenun.");
    }
    let iso3 = iso3(country)
        .ok_or_else(|| anyhow::anyhow!("{country} is not a country code Tenun knows."))?;
    let file = dir.join(format!("{iso3}-{}.geojson", level.adm()));
    if file.is_file() {
        return Ok(file);
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();
    let meta_url = format!("{}/{iso3}/{}/", api.trim_end_matches('/'), level.adm());
    let none = || {
        anyhow::anyhow!(
            "geoBoundaries has no {} map of {country}.",
            level_words(level)
        )
    };
    let mut meta = agent.get(&meta_url).call().map_err(|e| match e {
        ureq::Error::StatusCode(404) => none(),
        e => anyhow::anyhow!("Could not reach geoBoundaries: {e}"),
    })?;
    let info: serde_json::Value =
        serde_json::from_reader(meta.body_mut().as_reader().take(1024 * 1024)).map_err(|_| {
            anyhow::anyhow!(
                "geoBoundaries has no {} map of {country}.",
                level_words(level)
            )
        })?;
    let link = info
        .get("simplifiedGeometryGeoJSON")
        .or_else(|| info.get("gjDownloadURL"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "geoBoundaries has no {} map of {country}.",
                level_words(level)
            )
        })?;
    let mut res = agent
        .get(link)
        .call()
        .map_err(|e| anyhow::anyhow!("Could not download the map of {country}: {e}"))?;
    let mut body = Vec::new();
    res.body_mut()
        .as_reader()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut body)?;
    if body.len() as u64 > MAX_BYTES {
        anyhow::bail!("The map of {country} is larger than Tenun reads (80 MB).");
    }
    // It must be GeoJSON: a page of an error is not kept.
    let parsed: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| anyhow::anyhow!("The map of {country} did not download as a map."))?;
    if parsed.get("type").and_then(|t| t.as_str()) != Some("FeatureCollection") {
        anyhow::bail!("The map of {country} did not download as a map.");
    }
    std::fs::create_dir_all(dir)?;
    // Written whole, then moved, so a half file is never read as a map. Two downloads at once each write their
    // own part.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let tmp = file.with_extension(format!("part{}-{nanos}", std::process::id()));
    std::fs::write(&tmp, &body)?;
    std::fs::rename(&tmp, &file)?;
    Ok(file)
}

fn level_words(level: Level) -> &'static str {
    match level {
        Level::Country => "country",
        Level::Province => "province",
        Level::Regency => "second-level",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(level: Level, raw: &str) -> Option<String> {
        match_place(level, "ID", raw).code().map(str::to_string)
    }

    #[test]
    fn the_gazetteer_has_every_country_province_and_kabupaten() {
        // Natural Earth 1:50m draws 237 countries with an ISO code (V4-D129).
        let countries = list(Level::Country, "");
        assert!(countries.len() >= 237, "{}", countries.len());
        let odd: Vec<&str> = countries
            .iter()
            .filter(|c| c.code.len() != 2 || iso3(&c.code).is_none())
            .map(|c| c.code.as_str())
            .collect();
        assert!(odd.is_empty(), "{odd:?}");
        let provinces = list(Level::Province, "ID");
        assert_eq!(provinces.len(), 38);
        let regencies = list(Level::Regency, "id");
        assert_eq!(regencies.len(), 514);
        let codes: std::collections::HashSet<&str> =
            regencies.iter().map(|p| p.code.as_str()).collect();
        assert_eq!(codes.len(), 514, "every code once");
        let prov: std::collections::HashSet<&str> =
            provinces.iter().map(|p| p.code.as_str()).collect();
        assert!(regencies.iter().all(|r| prov.contains(r.parent.as_str())));
        // 416 kabupaten and 98 kota (Kepmendagri).
        assert_eq!(
            regencies
                .iter()
                .filter(|r| r.name.starts_with("Kabupaten"))
                .count(),
            416
        );
        assert_eq!(
            regencies
                .iter()
                .filter(|r| r.name.starts_with("Kota"))
                .count(),
            98
        );
        // The four new provinces of Papua.
        for c in ["ID-PS", "ID-PT", "ID-PE", "ID-PD"] {
            assert!(prov.contains(c), "{c}");
        }
    }

    #[test]
    fn a_kabupaten_or_kota_is_read_by_its_code_or_any_usual_name() {
        let r = Level::Regency;
        assert_eq!(one(r, "Kabupaten Bandung").as_deref(), Some("32.04"));
        assert_eq!(one(r, "Kab. Bandung").as_deref(), Some("32.04"));
        assert_eq!(one(r, "kab bandung").as_deref(), Some("32.04"));
        assert_eq!(one(r, "  KABUPATEN   BANDUNG ").as_deref(), Some("32.04"));
        assert_eq!(one(r, "Kota Bandung").as_deref(), Some("32.73"));
        assert_eq!(one(r, "Kodya Bandung").as_deref(), Some("32.73"));
        assert_eq!(one(r, "32.04").as_deref(), Some("32.04"));
        assert_eq!(one(r, "3204").as_deref(), Some("32.04"));
        // A name that only one place has needs no "Kabupaten".
        assert_eq!(
            one(r, "Banyuasin").as_deref(),
            one(r, "Kabupaten Banyuasin").as_deref()
        );
        assert_eq!(
            one(r, "Banyu Asin").as_deref(),
            one(r, "Kabupaten Banyuasin").as_deref()
        );
        // Official forms people rarely type, and their usual forms.
        assert_eq!(one(r, "Kota Jakarta Pusat").as_deref(), Some("31.71"));
        assert_eq!(one(r, "Jakarta Pusat").as_deref(), Some("31.71"));
        assert_eq!(
            one(r, "Kota Administrasi Jakarta Pusat").as_deref(),
            Some("31.71")
        );
        assert_eq!(one(r, "Pangkalpinang"), one(r, "Kota Pangkal Pinang"));
        assert_eq!(one(r, "Palangka Raya"), one(r, "Kota Palangkaraya"));
        assert_eq!(one(r, "Baubau"), one(r, "Kota Bau Bau"));
        assert_eq!(
            one(r, "Kep. Seribu"),
            one(r, "Kabupaten Administrasi Kepulauan Seribu")
        );
        // Old names.
        assert_eq!(one(r, "Kabupaten Toba Samosir"), one(r, "Kabupaten Toba"));
        assert_eq!(one(r, "Mamuju Utara"), one(r, "Kabupaten Pasangkayu"));
        // Lubuklinggau is 16.73 (the v4 fixture had 16.74 by hand).
        assert_eq!(one(r, "Lubuklinggau").as_deref(), Some("16.73"));
        // A bare city name that is a kabupaten and a kota is ambiguous, with a hint.
        let m = match_place(r, "ID", "Bandung");
        assert_eq!(
            m.hint().as_deref(),
            Some("Write Kabupaten Bandung or Kota Bandung.")
        );
        assert!(m.code().is_none());
        // docs/maps.md says 26 bare names are a kabupaten and a kota.
        let two = list(r, "ID")
            .iter()
            .filter_map(|p| norm(&p.name).strip_prefix("kota ").map(str::to_string))
            .filter(|bare| match_place(r, "ID", bare).hint().is_some())
            .count();
        assert_eq!(two, 26);
        // A typo, another country, an empty cell: no match.
        assert_eq!(match_place(r, "ID", "Bandunk"), Matched::None);
        assert_eq!(match_place(r, "MY", "Kota Bandung"), Matched::None);
        assert_eq!(match_place(r, "ID", "  "), Matched::None);
    }

    #[test]
    fn a_province_is_read_by_iso_code_kemendagri_code_or_a_usual_name() {
        let p = Level::Province;
        for raw in [
            "ID-JB",
            "id-jb",
            "JB",
            "32",
            "Jawa Barat",
            "Provinsi Jawa Barat",
            "Jabar",
            "West Java",
        ] {
            assert_eq!(one(p, raw).as_deref(), Some("ID-JB"), "{raw}");
        }
        for raw in [
            "DKI Jakarta",
            "Jakarta",
            "ID-JK",
            "Daerah Khusus Ibukota Jakarta",
        ] {
            assert_eq!(one(p, raw).as_deref(), Some("ID-JK"), "{raw}");
        }
        for raw in [
            "DIY",
            "Yogyakarta",
            "Jogja",
            "Daerah Istimewa Yogyakarta",
            "DI Yogyakarta",
        ] {
            assert_eq!(one(p, raw).as_deref(), Some("ID-YO"), "{raw}");
        }
        for raw in ["Aceh", "NAD"] {
            assert_eq!(one(p, raw).as_deref(), Some("ID-AC"), "{raw}");
        }
        for raw in ["Bangka Belitung", "Babel", "Kepulauan Bangka Belitung"] {
            assert_eq!(one(p, raw).as_deref(), Some("ID-BB"), "{raw}");
        }
        assert_eq!(one(p, "Papua Pegunungan").as_deref(), Some("ID-PE"));
        assert_eq!(one(p, "Papua Barat Daya").as_deref(), Some("ID-PD"));
        assert_eq!(one(p, "Jawa Barrat"), None);
    }

    #[test]
    fn a_country_is_read_by_either_iso_code_or_its_english_or_indonesian_name() {
        let c = |raw: &str| {
            match_place(Level::Country, "", raw)
                .code()
                .map(str::to_string)
        };
        for raw in ["ID", "IDN", "idn", "Indonesia"] {
            assert_eq!(c(raw).as_deref(), Some("ID"), "{raw}");
        }
        for raw in ["US", "USA", "United States of America", "Amerika Serikat"] {
            assert_eq!(c(raw).as_deref(), Some("US"), "{raw}");
        }
        assert_eq!(c("Mozambique").as_deref(), Some("MZ"));
        assert_eq!(c("Mozambik").as_deref(), Some("MZ"));
        // Natural Earth's -99 cases have their codes.
        assert_eq!(c("France").as_deref(), Some("FR"));
        assert_eq!(c("Norway").as_deref(), Some("NO"));
        assert_eq!(c("Kosovo").as_deref(), Some("XK"));
        assert_eq!(iso3("XK"), Some("XKX"));
        assert_eq!(c("Indonesai"), None);
        assert_eq!(iso3("mz"), Some("MOZ"));
    }

    /// The `code` of every shape of a TopoJSON file of the UI.
    fn shapes(file: &str) -> Vec<String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../ui/public/geo")
            .join(file);
        let topo: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let objects = topo["objects"].as_object().unwrap();
        assert_eq!(objects.len(), 1, "{file}");
        objects.values().next().unwrap()["geometries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["properties"]["code"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn every_place_has_a_shape_and_every_shape_a_place() {
        for (file, level) in [
            ("world.topo.json", Level::Country),
            ("ID-1.topo.json", Level::Province),
            ("ID-2.topo.json", Level::Regency),
        ] {
            let mut drawn = shapes(file);
            drawn.sort();
            let before = drawn.len();
            drawn.dedup();
            assert_eq!(drawn.len(), before, "{file}: a code is drawn twice");
            let mut known: Vec<String> = list(level, "ID").iter().map(|p| p.code.clone()).collect();
            known.sort();
            assert_eq!(drawn, known, "{file}");
        }
    }

    #[test]
    fn the_maps_stay_small() {
        let size = |f: &str| {
            std::fs::metadata(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../ui/public/geo")
                    .join(f),
            )
            .unwrap()
            .len()
        };
        // GRANDPLAN §7: Indonesia under 1.5 MB together; the world is a quarter of a megabyte.
        assert!(size("ID-1.topo.json") + size("ID-2.topo.json") < 1_500_000);
        assert!(size("world.topo.json") < 400_000);
    }

    #[test]
    fn names_are_read_the_same_way_here_and_in_the_build_script() {
        assert_eq!(norm("Kab. Kep. Seribu"), "kabupaten kepulauan seribu");
        assert_eq!(
            norm("Kota Administrasi Jakarta Pusat"),
            "kota jakarta pusat"
        );
        assert_eq!(norm("Provinsi  Jawa-Barat"), "jawa barat");
        assert_eq!(norm("KOTAMADYA Bau-Bau"), "kota bau bau");
    }
}

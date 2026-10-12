# Where the maps and place names come from

`build.mjs` downloads these files once into `.cache/` and writes `ui/public/geo/*.topo.json` and
`crates/tenun-core/src/geo/places.tsv`. Each source is pinned, so the same run gives the same files.

| What | Source | Version | Licence |
|---|---|---|---|
| Countries (`world.topo.json`) | [Natural Earth](https://www.naturalearthdata.com/) admin-0 countries, 1:50m, from `nvkelso/natural-earth-vector` | tag `v5.1.2` | Public domain |
| Kabupaten and kota of Indonesia (`ID-2.topo.json`), and the provinces made from them (`ID-1.topo.json`) | [geoBoundaries](https://www.geoboundaries.org/) gbOpen IDN ADM2 (`IDN-ADM2-22746128`), from BPS, the World Food Programme and OCHA ROAP | commit `9469f09` of `wmgeolab/geoBoundaries`, built 12 Dec 2023, boundaries of 2020 | CC BY 3.0 IGO |
| Codes and official names of Indonesia | [`cahyadsn/wilayah`](https://github.com/cahyadsn/wilayah) `db/wilayah.sql`, following Kepmendagri No. 300.2.2-2138 Tahun 2025 (the file) and No. 300.2.2-2430 Tahun 2025 (the repository's README) | commit `7586f42` | MIT |

Other countries' provinces and second levels are not in the app. The map downloads them from geoBoundaries
gbOpen the first time it needs them (their licences are listed with each boundary on geoBoundaries).

## What the script changes

- **Codes.** geoBoundaries names a shape and gives no code. Each shape gets the Kemendagri code of the place with
  the same name: "Bandung" is Kabupaten Bandung and "Kota Bandung" is Kota Bandung. Names written apart or
  together are one name ("Banyu Asin", "Banyuasin"). Six kabupaten have new names since 2020 or read as
  something else: Kota Baru (Kabupaten Kotabaru), Mahakam Hulu (Mahakam Ulu), Maluku Tenggara Barat (Kepulauan
  Tanimbar), Mamuju Utara (Pasangkayu), Siau Tagulandang Biaro (Kep. Siau Tagulandang Biaro), Toba Samosir (Toba).
- **Shapes that are not places.** Five shapes are lakes, reservoirs or a forest (Danau, Danau Toba, Waduk Cirata,
  Wadung Kedungombo, Hutan). They are left out, so 519 shapes become 514.
- **The 38 provinces.** The kabupaten and kota are joined by the province of their code. The four provinces of
  Papua made in 2022 (Papua Selatan, Papua Tengah, Papua Pegunungan, Papua Barat Daya) come out of this, with their
  ISO 3166-2 codes (`ID-PS`, `ID-PT`, `ID-PE`, `ID-PD`).
- **Countries.** One shape per ISO 3166-1 code. Natural Earth's own codes are used where ISO has none for a shape
  it draws apart (France, Norway, Kosovo, Taiwan). Australia's code also covers Ashmore and Cartier Islands and its
  Indian Ocean Territories. Three shapes have no ISO code (Somaliland, Northern Cyprus, Siachen Glacier); they
  are drawn as part of the country Natural Earth's Indonesian point of view gives them (Somalia, Cyprus, India).
- **Size.** The kabupaten keep 3 % of their points and the provinces 2 % (Visvalingam, weighted; shared borders
  stay one line), the countries 25 %. Indonesia's two files together stay under 1.5 MB.

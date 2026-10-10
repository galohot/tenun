<p align="center">
  <img src="assets/icon.svg" width="96" height="96" alt="">
</p>

<h1 align="center">Tenun</h1>

<p align="center"><b>Weave your spreadsheets and databases into one dashboard.</b><br>
A local app. No server, no Docker, no account.</p>

<p align="center">
  <img src="docs/demo.gif" alt="Opening the sample, clicking a store to filter every chart, saving the Excel file so the numbers update, and the model view that links three files" width="900">
</p>

Tenun reads your Excel files, CSV files and databases where they are. You draw a line between a
column in one source and a column in another, and every chart can use both. Click a bar and
every other chart filters. Save the Excel file and the dashboard updates.

*Tenun* is Indonesian for woven cloth.

<p align="center">
  <img src="docs/dashboard-light.png" alt="A Tenun dashboard: four number tiles, an area chart by month and region, a donut by category, and bar charts by store and customer segment" width="900">
</p>

## Why

- **Your data is in more than one place.** Sales in an Excel file, stores in a CSV export,
  customers in a database. Most dashboard tools read one database per chart. Tenun joins them.
- **Setup should take a minute.** Download the app and open it. Everything runs on your computer.
- **Dashboards should look good without effort.** There are no colour pickers and no font menus.
  The palette is fixed and checked for colour blindness, in light and dark mode.

## What it does

- **Sources:** Excel (`.xlsx`, `.xlsm`, `.xls`, `.ods`), CSV and TSV, Parquet, SQLite, DuckDB,
  PostgreSQL and MySQL.
- **Relationships across sources:** drag from a column to a column in the Model view. Tenun
  suggests the likely ones (`store_id` → `stores.id`) and checks them against the data.
- **Six tile types:** number, bar, line, area, share of total, and table. No pies and no donuts:
  part-to-whole is a 100 % strip with the exact shares listed under it.
- **Honest comparisons:** a number tile says which period it covers and compares it fairly. If
  October is only half over, it compares October 1–14 with September 1–14, not with all of
  September. Charts draw an unfinished last period dashed or lighter, with a note.
- **Click to filter:** click a bar, a share or a table row, and every other tile filters, across sources.
- **Live:** saving an Excel or CSV file updates open dashboards in under a second. Databases
  refresh on an interval you choose.
- **Export to one HTML file:** it opens offline, anywhere, and click-to-filter still works.
- **Two ways to run it:** the desktop app, or `tenun serve` in your browser.

<p align="center">
  <img src="docs/model.png" alt="The Model view: an Orders table from an Excel file linked to stores from a CSV file and customers from a SQLite file" width="900">
</p>

## Install

**Desktop app:** download it from [Releases](https://github.com/galohot/tenun/releases).

| System | File |
|---|---|
| Windows | `Tenun_x.y.z_x64-setup.exe` or `.msi` |
| macOS (Apple silicon) | `Tenun_x.y.z_aarch64.dmg` |
| macOS (Intel) | `Tenun_x.y.z_x64.dmg` |
| Linux | `.AppImage` or `.deb` |

The apps are not code-signed yet:

- **macOS:** open the app once and close the warning. Then open **System Settings → Privacy &
  Security**, scroll down, and choose **Open Anyway**. You do this once. Or, in Terminal:
  `xattr -dr com.apple.quarantine /Applications/Tenun.app`
- **Windows:** in the SmartScreen window, choose **More info**, then **Run anyway**.

**Command line (browser mode):** download `tenun-<version>-<platform>` from the same page, then:

```sh
tenun serve                 # opens http://127.0.0.1:4310
tenun serve sales.tenun     # opens or creates a workspace file
```

## Quick start

1. Open Tenun and click **Open the sample**. It is a small coffee chain spread over three files:
   orders in Excel, stores in CSV, customers in SQLite. The files go to `Documents/Tenun Sample`.
2. Click a store in **Amount by store**. Every other tile filters to that store. The number tiles
   show October 1–14 against September 1–14, because the sample data ends on October 14.
3. Open `coffee.xlsx` in Excel, change an amount, and save. The dashboard updates.
4. Open **Model** to see how the three files are related.
5. Click **Export** to save the dashboard as one HTML file.

With your own data: **Data → Add data**, then **Model → Suggest relationships**, then a new
dashboard and **Add tile**.

## How it works

```
 Desktop window ─┐                       ┌─ Excel / SQLite  → copied into memory, reloaded on save
 or your browser ─┼─► local server (Rust) ┼─ CSV / Parquet   → read on every query
                  │   + DuckDB in memory  └─ PostgreSQL / MySQL → attached read-only, queried live
```

- The engine is [DuckDB](https://duckdb.org), embedded. Nothing is uploaded anywhere.
- A tile asks for "this value, grouped by that column". Tenun finds the path through your
  relationships and writes one SQL query with the joins. Values are always query parameters.
- A relationship needs one side with unique values, like an ID. If neither side is unique, Tenun
  refuses the link and explains why. Such a join would count every total twice.
- A workspace is one `.tenun` file (JSON) with your sources, relationships and dashboards. File
  paths are stored relative to it, so a folder with the file and its data moves as one. It is
  safe to commit to git.

## Share online with tenun.cloud

Choose **Publish** on a dashboard to give it a private link on [tenun.cloud](https://tenun.cloud). Tenun
builds the dashboard on your computer and sends only the results: not your files, and never your
database passwords. The cloud never connects to your data.

- **Who can see it:** anyone with the link, or only people you invite (they sign in with Google).
- **Keep it updated:** when the data changes, hourly, daily, or by hand. Updates run while the app is
  open; [`tenun agent`](docs/agent.md) keeps them running on a computer that stays on.
- **Always honest about age:** every hosted dashboard says when it was updated and from which
  computer, and shows a banner when an update is late.
- **Free:** up to 3 published dashboards, 10 MB each.

```sh
tenun login                    # connect this computer (approve a code in the browser)
tenun publish sales.tenun      # publish now
tenun agent sales.tenun        # keep publishing on schedule
```

## Privacy and security

- The server listens on `127.0.0.1` only. It refuses requests that name another host, which
  blocks DNS-rebinding attacks. Every change needs a header that other web pages cannot send.
- `tenun serve --host 0.0.0.0` shares Tenun on your network. It prints a link with an access
  token, and every request needs that token. Anyone with the link can read files you can read.
- Database passwords are kept in a private file in your user folder (like `~/.pgpass`), never in
  the `.tenun` file.
- Databases are opened read-only. Tenun never writes to your data.
- No telemetry. The only network use: connecting to PostgreSQL or MySQL the first time downloads
  the DuckDB connector for it.

## Build from source

You need Rust (stable), Node 22 and pnpm. On Linux the desktop app also needs WebKitGTK 4.1.

```sh
pnpm -C ui install && pnpm -C ui build    # the web UI, embedded at compile time
cargo run -p tenun -- serve               # browser mode
pnpm -C app install && pnpm -C app tauri build   # desktop app
```

The first build compiles DuckDB from C++ source and takes 10 to 20 minutes.

Checks: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`,
`pnpm -C ui check`, `pnpm -C ui test`.

## Not in v1

User accounts, shared servers, row-level security, scheduled emails, a formula language,
custom colours, pivot tables and maps. Tenun aims to do a few things well.

## Licence

`MIT OR Apache-2.0`, at your option.

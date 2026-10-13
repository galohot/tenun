//! A speed benchmark: how fast Tenun loads and answers at 100 thousand, 1 million and 10 million rows.
//! The data is made here, with a fixed seed, so every run measures the same thing.
//!
//! ```sh
//! cargo run --release --example bench -- ~/tenun-bench            # all sizes (writes about 1.5 GB of data once)
//! cargo run --release --example bench -- ~/tenun-bench --quick    # 100 thousand rows only (CI)
//! cargo run --release --example bench -- ~/tenun-bench --write    # also writes docs/benchmark.md
//! cargo run --release --example bench -- ~/tenun-bench --sizes 1000000
//! ```
//!
//! Each number is the median of 5 runs after one warm-up run. Loads use a fresh engine every time.

use serde_json::{Value, json};
use std::fmt::Write as _;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot;
use tenun_core::workspace::{FieldRef, NamedMeasure, TableSetup};

const STORES: u64 = 50;
const PRODUCTS: u64 = 200;
const DAYS: u64 = 3 * 365;
const RUNS: usize = 5;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = args.first().map(PathBuf::from) else {
        eprintln!("usage: bench <data folder> [--quick] [--write]");
        std::process::exit(2);
    };
    let quick = args.iter().any(|a| a == "--quick");
    let write = args.iter().any(|a| a == "--write");
    std::fs::create_dir_all(&dir).unwrap();
    // `--sizes 1000000,10000000` runs only those sizes.
    let chosen: Option<Vec<u64>> = args
        .iter()
        .position(|a| a == "--sizes")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.split(',').filter_map(|n| n.parse().ok()).collect());
    let sizes: Vec<u64> = match chosen {
        Some(v) => v,
        None if quick => vec![100_000],
        None => vec![100_000, 1_000_000, 10_000_000],
    };

    let mut report = String::new();
    writeln!(report, "# Benchmark\n").unwrap();
    writeln!(
        report,
        "How fast Tenun loads files and answers tiles. Measured with `cargo run --release --example bench`. \
         Each time is the median of {RUNS} runs after one warm-up run, in milliseconds (an Excel load: 3 runs; an \
         export: 3 runs at 1 million rows and 1 run at 10 million). A tile is computed in full, as the app asks \
         for it: a number tile, for example, also computes its comparison and its small chart. An export also \
         computes, for offline use, what every click on every tile does to the other tiles.\n"
    )
    .unwrap();
    writeln!(report, "Machine: {}.\n", machine()).unwrap();
    writeln!(
        report,
        "The data: orders (a date over three years, a store, a customer, a product, an amount and a quantity), \
         {STORES} stores in 5 regions, customers, and a target for each store and month. Orders link to stores and \
         customers; targets link to stores.\n"
    )
    .unwrap();

    for &n in &sizes {
        let data = make_data(&dir, n);
        println!("== {} rows", thousands(n));
        writeln!(report, "## {} rows\n", thousands(n)).unwrap();
        writeln!(report, "| Step | ms |\n|---|---:|").unwrap();
        let mut row = |name: &str, ms: f64| {
            println!("{name:<48} {ms:>10.1}");
            writeln!(report, "| {name} | {} |", fmt_ms(ms)).unwrap();
        };

        row("Load the orders (CSV)", median(|| load_orders(&data.csv)));
        row(
            "Load the orders (Parquet)",
            median(|| load_orders(&data.parquet)),
        );
        if let Some(xlsx) = &data.xlsx {
            // A big workbook takes long to read: three runs are enough.
            row(
                "Load the orders (Excel)",
                median_of(3, || load_orders(xlsx)),
            );
        }

        let (e, ids) = model(&data);
        row(
            "Suggest relationships",
            median(|| time(|| drop(e.suggest().unwrap()))),
        );
        let tiles = tiles(&ids);
        for (name, tile) in &tiles {
            let t: snapshot::Tile = serde_json::from_value(tile.clone()).unwrap();
            row(
                name,
                median(|| time(|| drop(snapshot::run_tile(&e, &t, &[]).unwrap()))),
            );
        }
        // A click on a store filters the other tiles.
        let click: Vec<snapshot::DashFilter> = serde_json::from_value(json!([{
            "table": ids.stores, "column": "name", "op": "in", "values": ["Store 7"], "fromTile": "x"
        }]))
        .unwrap();
        if std::env::var_os("BENCH_CLICKS").is_some() {
            for (name, tile) in &tiles {
                let t: snapshot::Tile = serde_json::from_value(tile.clone()).unwrap();
                let ms = median(|| time(|| drop(snapshot::run_tile(&e, &t, &click).unwrap())));
                println!("  clicked: {name:<40} {ms:>8.1}");
            }
        }
        let line: snapshot::Tile = serde_json::from_value(tiles[2].1.clone()).unwrap();
        row(
            "Line by month, filtered by a click on a store",
            median(|| time(|| drop(snapshot::run_tile(&e, &line, &click).unwrap()))),
        );

        // A calculated column, built when the table loads.
        let setup: TableSetup = serde_json::from_value(
            json!({ "calc": [{ "name": "with tax", "formula": "[amount] * 1.11" }] }),
        )
        .unwrap();
        let start = Instant::now();
        e.set_table_setup(&ids.orders, Some(setup)).unwrap();
        row(
            "Add a calculated column (the table reloads)",
            start.elapsed().as_secs_f64() * 1e3,
        );
        let calc: snapshot::Tile = serde_json::from_value(json!({
            "id": "c", "type": "bar", "measures": [{ "table": ids.orders, "column": "with tax", "agg": "sum" }],
            "dims": [{ "table": ids.stores, "column": "region" }]
        }))
        .unwrap();
        row(
            "Bar of the calculated column by region",
            median(|| time(|| drop(snapshot::run_tile(&e, &calc, &[]).unwrap()))),
        );

        // An export of a dashboard with the six tiles.
        e.set_dashboards(vec![json!({
            "id": "d1", "title": "Bench", "refresh": 0,
            "tiles": tiles.iter().enumerate().map(|(i, (_, t))| {
                let mut t = t.clone();
                t["id"] = json!(format!("t{i}"));
                t["title"] = json!("");
                t["x"] = json!((i % 2) * 6); t["y"] = json!((i / 2) * 4); t["w"] = json!(6); t["h"] = json!(4);
                t
            }).collect::<Vec<_>>()
        })]);
        let mut size = 0;
        // An export of a big table takes long: fewer runs.
        let runs = if n >= 10_000_000 {
            1
        } else if n >= 1_000_000 {
            3
        } else {
            RUNS
        };
        row(
            "Export a dashboard of six tiles",
            median_of(runs, || {
                time(|| {
                    let v = snapshot::build(&e, "d1", &[]).unwrap();
                    size = serde_json::to_vec(&v).unwrap().len();
                })
            }),
        );
        writeln!(
            report,
            "\nThe export is {} KB. Peak memory of the run so far: {} MB.\n",
            size / 1024,
            peak_mb()
        )
        .unwrap();
        println!("export {} KB, peak memory {} MB", size / 1024, peak_mb());
    }

    if write {
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/benchmark.md");
        std::fs::write(&out, report).unwrap();
        println!("Wrote {}", out.display());
    }
}

struct Data {
    csv: PathBuf,
    parquet: PathBuf,
    xlsx: Option<PathBuf>,
    stores: PathBuf,
    customers: PathBuf,
    targets: PathBuf,
}

struct Ids {
    orders: String,
    stores: String,
    targets: String,
}

/// A small, fast generator with a fixed seed (xorshift64*), so the data is the same on every machine.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Writes the files for `n` orders once; a later run reuses them.
fn make_data(dir: &Path, n: u64) -> Data {
    let d = dir.join(format!("rows-{n}"));
    std::fs::create_dir_all(&d).unwrap();
    let customers_n = (n / 20).max(100);
    let data = Data {
        csv: d.join("orders.csv"),
        parquet: d.join("orders.parquet"),
        xlsx: (n <= 1_000_000).then(|| d.join("orders.xlsx")),
        stores: d.join("stores.csv"),
        customers: d.join("customers.csv"),
        targets: d.join("targets.csv"),
    };
    if !data.csv.exists() {
        println!("Making {} orders…", thousands(n));
        let mut w = BufWriter::new(std::fs::File::create(&data.csv).unwrap());
        writeln!(w, "order_id,date,store_id,customer_id,product,amount,qty").unwrap();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let start = chrono::NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        for i in 0..n {
            let day = start + chrono::Days::new(rng.below(DAYS));
            let amount = (rng.below(50_000) as f64 + 500.0) / 100.0;
            writeln!(
                w,
                "{i},{day},{},{},P{},{amount:.2},{}",
                1 + rng.below(STORES),
                1 + rng.below(customers_n),
                1 + rng.below(PRODUCTS),
                1 + rng.below(5)
            )
            .unwrap();
        }
        w.flush().unwrap();
        let mut s = String::from("id,name,region\n");
        for i in 1..=STORES {
            writeln!(s, "{i},Store {i},Region {}", 1 + i % 5).unwrap();
        }
        std::fs::write(&data.stores, s).unwrap();
        let mut c = BufWriter::new(std::fs::File::create(&data.customers).unwrap());
        writeln!(c, "id,segment").unwrap();
        for i in 1..=customers_n {
            writeln!(
                c,
                "{i},{}",
                ["Retail", "Office", "Online"][(i % 3) as usize]
            )
            .unwrap();
        }
        c.flush().unwrap();
        let mut t = String::from("store_id,month,target\n");
        for i in 1..=STORES {
            for m in 0..36 {
                writeln!(
                    t,
                    "{i},{}-{:02}-01,{}",
                    2023 + m / 12,
                    1 + m % 12,
                    1000 + (i * 37 + m as u64 * 11) % 500
                )
                .unwrap();
            }
        }
        std::fs::write(&data.targets, t).unwrap();
    }
    if !data.parquet.exists() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "COPY (SELECT * FROM read_csv('{}')) TO '{}' (FORMAT parquet);",
            data.csv.display(),
            data.parquet.display()
        ))
        .unwrap();
    }
    if let Some(x) = &data.xlsx
        && !x.exists()
    {
        println!("Making the Excel file of {} orders…", thousands(n));
        let mut wb = rust_xlsxwriter::Workbook::new();
        let ws = wb.add_worksheet();
        let text = std::fs::read_to_string(&data.csv).unwrap();
        for (r, line) in text.lines().enumerate() {
            for (c, v) in line.split(',').enumerate() {
                let (r, c) = (r as u32, c as u16);
                match v.parse::<f64>() {
                    Ok(f) if r > 0 && c != 1 => ws.write_number(r, c, f).unwrap(),
                    _ => ws.write_string(r, c, v).unwrap(),
                };
            }
        }
        wb.save(x).unwrap();
    }
    data
}

fn engine() -> (Engine, PathBuf) {
    let d = std::env::temp_dir().join(format!("tenun-bench-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    (
        Engine::new(Secrets::new(d.join("secrets.json"))).unwrap(),
        d,
    )
}

fn add(e: &Engine, path: &Path) -> String {
    let id = e
        .add_source(AddSource::File {
            path: path.to_string_lossy().into_owned(),
        })
        .unwrap();
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    snap.tables
        .iter()
        .find(|t| t.source_id == id)
        .unwrap()
        .id
        .clone()
}

fn load_orders(path: &Path) -> f64 {
    let (e, _) = engine();
    time(|| drop(add(&e, path)))
}

/// The four tables, linked, and a measure that divides orders by targets.
fn model(data: &Data) -> (Engine, Ids) {
    let (e, _) = engine();
    let orders = add(&e, &data.csv);
    let stores = add(&e, &data.stores);
    let customers = add(&e, &data.customers);
    let targets = add(&e, &data.targets);
    let link = |a: &str, ac: &str, b: &str, bc: &str| {
        e.add_relationship(
            FieldRef {
                table: a.into(),
                column: ac.into(),
            },
            FieldRef {
                table: b.into(),
                column: bc.into(),
            },
        )
        .unwrap();
    };
    link(&orders, "store_id", &stores, "id");
    link(&orders, "customer_id", &customers, "id");
    link(&targets, "store_id", &stores, "id");
    let m: NamedMeasure = serde_json::from_value(json!({
        "id": "", "name": "Achievement", "formula": "SUM([orders].[amount]) / SUM([targets].[target])"
    }))
    .unwrap();
    e.set_measures(vec![m], true).unwrap();
    let ids = Ids {
        orders,
        stores,
        targets,
    };
    (e, ids)
}

fn tiles(ids: &Ids) -> Vec<(&'static str, Value)> {
    let amount = json!({ "table": ids.orders, "column": "amount", "agg": "sum" });
    let month = json!({ "table": ids.orders, "column": "date", "grain": "month" });
    let store = json!({ "table": ids.stores, "column": "name" });
    let region = json!({ "table": ids.stores, "column": "region" });
    vec![
        (
            "Number with a comparison",
            json!({ "id": "k", "type": "kpi", "measures": [amount], "dims": [{ "table": ids.orders, "column": "date", "grain": "month" }] }),
        ),
        (
            "Bar by store, top 10 and Other",
            json!({ "id": "b", "type": "bar", "measures": [amount], "dims": [store], "top": { "n": 10 } }),
        ),
        (
            "Line by month",
            json!({ "id": "l", "type": "line", "measures": [amount], "dims": [month] }),
        ),
        (
            "Pivot: store by month",
            json!({ "id": "p", "type": "pivot", "measures": [amount], "dims": [store], "cols": month }),
        ),
        (
            "Measure over two tables, by region",
            json!({ "id": "m", "type": "bar", "measures": [{ "named": "m1", "table": "", "column": null, "agg": "sum" }], "dims": [region] }),
        ),
        (
            "Targets by region and month",
            json!({ "id": "t", "type": "table", "measures": [{ "table": ids.targets, "column": "target", "agg": "sum" }], "dims": [region, { "table": ids.targets, "column": "month", "grain": "month" }] }),
        ),
    ]
}

fn time(f: impl FnOnce()) -> f64 {
    let start = Instant::now();
    f();
    start.elapsed().as_secs_f64() * 1e3
}

fn median(f: impl FnMut() -> f64) -> f64 {
    median_of(RUNS, f)
}

fn median_of(runs: usize, mut f: impl FnMut() -> f64) -> f64 {
    f();
    let mut v: Vec<f64> = (0..runs).map(|_| f()).collect();
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn fmt_ms(ms: f64) -> String {
    if ms >= 100.0 {
        thousands(ms.round() as u64)
    } else {
        format!("{ms:.1}")
    }
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// The largest the process grew to, from Linux's `/proc`. Elsewhere: 0.
fn peak_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
        .map_or(0, |kb| kb / 1024)
}

/// The machine in general words: CPU cores, memory and the operating system.
fn machine() -> String {
    let cores = std::thread::available_parallelism().map_or(0, |n| n.get());
    let mem = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
        .map_or(String::new(), |kb| {
            format!(", {} GB of memory", (kb as f64 / 1048576.0).round())
        });
    format!("{cores} CPU threads{mem}, {}", std::env::consts::OS)
}

//! The sample workspace: a small coffee chain spread over three sources (an
//! Excel workbook, a CSV file and a SQLite database), already related, with two
//! dashboards. The files are written to a folder the user can open and edit, so
//! saving `coffee.xlsx` in Excel shows live refresh at work.

use serde_json::json;
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::workspace::{FieldRef, Locale, Relationship, Source, SourceKind, Workspace};

const COFFEE: &[u8] = include_bytes!("../../../examples/sample/coffee.xlsx");
const STORES: &[u8] = include_bytes!("../../../examples/sample/stores.csv");
const CUSTOMERS: &[u8] = include_bytes!("../../../examples/sample/customers.sqlite");

pub fn folder() -> PathBuf {
    let home = crate::fsbrowse::home();
    let docs = home.join("Documents");
    let base = if docs.is_dir() { docs } else { home };
    base.join("Tenun Sample")
}

/// Writes the sample CSV of a starter dashboard, in the app's language, to `Starters/` in the sample
/// folder and returns its path. A file that is already there is kept, with the user's edits.
pub fn starter_file(starter: &str, lang: &str) -> anyhow::Result<PathBuf> {
    let (name, csv) = tenun_core::starter::sample_in(starter, lang)
        .ok_or_else(|| anyhow::anyhow!("unknown starter {starter}"))?;
    let dir = folder().join("Starters");
    std::fs::create_dir_all(&dir)?;
    let p = dir.join(name);
    if !p.exists() {
        std::fs::write(&p, csv)?;
    }
    Ok(p)
}

pub fn open(engine: &Engine) -> anyhow::Result<()> {
    let dir = folder();
    std::fs::create_dir_all(&dir)?;
    for (name, bytes) in [
        ("coffee.xlsx", COFFEE),
        ("stores.csv", STORES),
        ("customers.sqlite", CUSTOMERS),
    ] {
        let p = dir.join(name);
        // Keep the user's edits if the sample is opened a second time.
        if !p.exists() {
            std::fs::write(&p, bytes)?;
        }
    }
    let file = dir.join("Coffee.tenun");
    if file.exists() {
        engine.open(&file)?;
        return Ok(());
    }
    // A new sample reads in the language of the computer, like any new workspace.
    let mut ws = workspace();
    ws.locale = Some(Locale::from_env());
    engine.replace(ws, Some(file.clone()));
    engine.save(Some(file))?;
    Ok(())
}

fn f(table: &str, column: &str) -> FieldRef {
    FieldRef {
        table: table.into(),
        column: column.into(),
    }
}

fn rel(id: &str, from: FieldRef, to: FieldRef) -> Relationship {
    Relationship {
        id: id.into(),
        from,
        to,
        name: None,
    }
}

pub fn workspace() -> Workspace {
    let orders = "s1/Orders";
    let tile = |id: &str,
                ty: &str,
                x: u32,
                y: u32,
                w: u32,
                h: u32,
                measures: serde_json::Value,
                dims: serde_json::Value| {
        json!({ "id": id, "type": ty, "title": "", "x": x, "y": y, "w": w, "h": h,
                "measures": measures, "dims": dims, "horizontal": false })
    };
    let month = json!([{ "table": orders, "column": "date", "grain": "month" }]);
    let mut by_store = tile(
        "t7",
        "bar",
        0,
        6,
        6,
        4,
        json!([{ "table": orders, "column": "amount", "agg": "sum" }]),
        json!([{ "table": "s2/stores", "column": "name" }]),
    );
    by_store["horizontal"] = json!(true);
    let mut by_segment = tile(
        "t8",
        "bar",
        6,
        6,
        6,
        4,
        json!([{ "table": orders, "column": "amount", "agg": "sum" }]),
        json!([{ "table": "s3/customers", "column": "segment" }]),
    );
    by_segment["horizontal"] = json!(true);
    let dashboard = json!({
        "id": "d1",
        "title": "Coffee overview",
        "refresh": 60,
        "tiles": [
            tile("t1", "kpi", 0, 0, 3, 2, json!([{ "table": orders, "column": "amount", "agg": "sum" }]), month.clone()),
            tile("t2", "kpi", 3, 0, 3, 2, json!([{ "table": orders, "column": null, "agg": "count" }]), month.clone()),
            tile("t3", "kpi", 6, 0, 3, 2, json!([{ "table": orders, "column": "customer_id", "agg": "count_distinct" }]), month.clone()),
            tile("t4", "kpi", 9, 0, 3, 2, json!([{ "table": orders, "column": "amount", "agg": "avg" }]), month.clone()),
            tile("t5", "bar", 0, 2, 8, 4, json!([{ "table": orders, "column": "amount", "agg": "sum" }]),
                 json!([{ "table": orders, "column": "date", "grain": "month" }, { "table": "s2/stores", "column": "region" }])),
            tile("t6", "share", 8, 2, 4, 4, json!([{ "table": orders, "column": "amount", "agg": "sum" }]),
                 json!([{ "table": "s1/Products", "column": "category" }])),
            by_store,
            by_segment,
        ]
    });
    let sum_amount = json!([{ "table": orders, "column": "amount", "agg": "sum" }]);
    let mut top_products = tile(
        "t10",
        "bar",
        0,
        5,
        5,
        3,
        sum_amount.clone(),
        json!([{ "table": orders, "column": "product" }]),
    );
    top_products["horizontal"] = json!(true);
    top_products["top"] = json!({ "n": 5, "other": true });
    top_products["sort"] = json!({ "by": "value", "desc": true });
    let mut by_category = tile(
        "t11",
        "pivot",
        5,
        5,
        7,
        3,
        sum_amount.clone(),
        json!([{ "table": "s1/Products", "column": "category" }]),
    );
    by_category["cols"] = json!({ "table": "s2/stores", "column": "region" });
    by_category["totals"] = json!(true);
    let mut month_steps = tile(
        "t12",
        "waterfall",
        3,
        0,
        9,
        5,
        sum_amount.clone(),
        month.clone(),
    );
    month_steps["title"] = json!("Revenue, month by month");
    let mut goal = tile("t13", "kpi", 0, 3, 3, 2, sum_amount.clone(), month.clone());
    goal["target"] = json!({ "value": 40000 });
    let mut region_filter = tile(
        "t14",
        "filter",
        0,
        0,
        3,
        3,
        json!([]),
        json!([{ "table": "s2/stores", "column": "region" }]),
    );
    region_filter["filter"] = json!({ "style": "list" });
    let mut note = tile("t15", "text", 0, 8, 12, 2, json!([]), json!([]));
    note["text"] = json!(
        "The bar chart keeps 5 products and adds the rest as **Other**. \
         The table adds up every row and column. Pick a region in the Region tile, and every tile follows."
    );
    let more_views = json!({
        "id": "d2",
        "title": "More views",
        "refresh": 60,
        "tiles": [region_filter, goal, month_steps, top_products, by_category, note]
    });
    Workspace {
        name: "Coffee".into(),
        sources: vec![
            Source {
                id: "s1".into(),
                name: "coffee.xlsx".into(),
                kind: SourceKind::Excel {
                    path: "coffee.xlsx".into(),
                },
            },
            Source {
                id: "s2".into(),
                name: "stores.csv".into(),
                kind: SourceKind::Csv {
                    path: "stores.csv".into(),
                },
            },
            Source {
                id: "s3".into(),
                name: "customers.sqlite".into(),
                kind: SourceKind::Sqlite {
                    path: "customers.sqlite".into(),
                },
            },
        ],
        relationships: vec![
            rel("r1", f(orders, "store_id"), f("s2/stores", "id")),
            rel("r2", f(orders, "customer_id"), f("s3/customers", "id")),
            rel("r3", f(orders, "product"), f("s1/Products", "product")),
        ],
        dashboards: vec![dashboard, more_views],
        layout: json!({ "model": {
            "s1/Orders": { "x": 0, "y": 60 },
            "s1/Products": { "x": 0, "y": 330 },
            "s2/stores": { "x": 340, "y": 0 },
            "s3/customers": { "x": 340, "y": 230 }
        }}),
        ..Workspace::default()
    }
}

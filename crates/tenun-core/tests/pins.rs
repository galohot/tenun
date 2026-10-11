//! Pins the SQL of existing queries, so a change to how joins are built can prove it left them
//! alone: a link whose one side has no repeated keys is joined exactly as before (V4-D26).
//! The queries are JSON, as a v3 file or a v3 client sends them: none names a link.

use chrono::NaiveDate;
use serde_json::json;
use tenun_core::catalog::{Catalog, Column, DType, Table};
use tenun_core::links::{Health, LinkHealth, Links};
use tenun_core::query::{SemanticQuery, compile_on};
use tenun_core::workspace::Relationship;

fn table(id: &str, cols: &[(&str, DType)]) -> Table {
    let (src, name) = id.split_once('/').unwrap();
    Table {
        id: id.into(),
        source_id: src.into(),
        name: name.into(),
        label: name.into(),
        sql_ref: format!("\"s_{src}\".\"{name}\""),
        columns: cols
            .iter()
            .map(|(n, d)| Column::new(n.to_string(), *d, String::new()))
            .collect(),
        rows: None,
        notes: vec![],
        header_row: None,
        prep: true,
    }
}

fn world() -> (Catalog, Vec<Relationship>) {
    use DType::*;
    let cat = Catalog {
        tables: vec![
            table(
                "s1/orders",
                &[
                    ("date", Date),
                    ("store_id", Number),
                    ("customer", Text),
                    ("amount", Number),
                ],
            ),
            table(
                "s2/stores",
                &[("id", Number), ("region_code", Text), ("city", Text)],
            ),
            table("s3/regions", &[("code", Text), ("name", Text)]),
            table("s4/customers", &[("name", Text), ("tier", Text)]),
            table("s5/shops", &[("code", Text), ("owner", Text)]),
        ],
    };
    let rel = |id: &str, from: (&str, &str), to: (&str, &str)| -> Relationship {
        serde_json::from_value(json!({
            "id": id,
            "from": {"table": from.0, "column": from.1},
            "to": {"table": to.0, "column": to.1},
        }))
        .unwrap()
    };
    let rels = vec![
        rel("r1", ("s1/orders", "store_id"), ("s2/stores", "id")),
        rel("r2", ("s2/stores", "region_code"), ("s3/regions", "code")),
        rel("r3", ("s1/orders", "customer"), ("s4/customers", "name")),
        // A number column against a text column: the join compares them as text.
        rel("r4", ("s1/orders", "store_id"), ("s5/shops", "code")),
    ];
    (cat, rels)
}

fn queries() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "two hops",
            json!({"measures": [{"table": "s1/orders", "column": "amount", "agg": "sum"}],
                   "dimensions": [{"table": "s3/regions", "column": "name"}]}),
        ),
        (
            "month, a filter and a sort",
            json!({"measures": [{"table": "s1/orders", "column": "amount", "agg": "sum"}],
                   "dimensions": [{"table": "s1/orders", "column": "date", "grain": "month"}],
                   "filters": [{"table": "s2/stores", "column": "city", "op": "in", "values": ["Jakarta", null]}],
                   "sort": [{"by": 1, "desc": true}], "limit": 12}),
        ),
        (
            "a ratio across two links",
            json!({"measures": [{"table": "s1/orders", "column": "amount", "agg": "sum",
                                 "per": {"table": "s4/customers", "column": "name", "agg": "count_distinct"}}],
                   "dimensions": [{"table": "s2/stores", "column": "city"}, {"table": "s3/regions", "column": "name"}],
                   "filters": [{"table": "s1/orders", "column": "amount", "op": "between", "values": [10, 500]},
                               {"table": "s4/customers", "column": "tier", "op": "not_in", "values": ["a"]}]}),
        ),
        (
            "groups only",
            json!({"dimensions": [{"table": "s2/stores", "column": "city"}, {"table": "s3/regions", "column": "name"}]}),
        ),
        (
            "rows of a table that other tables point at, by a column of the table it points at",
            json!({"measures": [{"table": "s2/stores", "column": null, "agg": "count"}],
                   "dimensions": [{"table": "s3/regions", "column": "name"}],
                   "filters": [{"table": "s1/orders", "column": "date", "op": "relative", "values": ["last_30_days"]}]}),
        ),
        (
            "a number joined to text",
            json!({"measures": [{"table": "s1/orders", "column": "amount", "agg": "avg"}],
                   "dimensions": [{"table": "s5/shops", "column": "owner"}]}),
        ),
        (
            "a field of a table nobody points to",
            json!({"measures": [{"table": "s3/regions", "column": null, "agg": "count"}],
                   "dimensions": [{"table": "s1/orders", "column": "customer"}]}),
        ),
    ]
}

#[test]
fn existing_queries_compile_to_the_sql_they_always_did() {
    let (cat, rels) = world();
    let today = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
    let mut got = Vec::new();
    for (name, q) in queries() {
        let q: SemanticQuery = serde_json::from_value(q).unwrap();
        let out = match compile_on(&q, &cat, &rels, today) {
            Ok(c) => format!("{}\n  params: {:?}", c.sql, c.params),
            Err(e) => format!("error: {e}"),
        };
        got.push((name, out));
    }
    if std::env::var("PRINT_PINS").is_ok() {
        for (n, o) in &got {
            println!("PIN\t{n}\t{}", o.replace('\n', "\\n"));
        }
        return;
    }
    for (n, o) in &got {
        let want = PINS
            .iter()
            .find(|(m, _)| m == n)
            .unwrap_or_else(|| panic!("no pin for {n}"));
        assert_eq!(o, &want.1.replace("\\n", "\n"), "{n}");
    }
}

/// A link whose one side is clean and whose many side repeats keys, which is the usual one to
/// many link, is joined as it was when nothing was known about the data.
#[test]
fn links_the_data_calls_clean_change_nothing() {
    let (cat, rels) = world();
    let today = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
    let health: Health = rels
        .iter()
        .map(|r| {
            (
                r.id.clone(),
                LinkHealth {
                    one_rows: 50,
                    many_dups: 20,
                    ..LinkHealth::default()
                },
            )
        })
        .collect();
    for (name, q) in queries() {
        let q: SemanticQuery = serde_json::from_value(q).unwrap();
        let plain = compile_on(&q, &cat, &rels, today).map(|c| (c.sql, c.params));
        let known = compile_on(&q, &cat, Links::new(&rels, Some(&health)), today)
            .map(|c| (c.sql, c.params));
        assert_eq!(plain, known, "{name}");
    }
}

/// What the code printed before links could be named (`PRINT_PINS=1` prints the current output).
const PINS: &[(&str, &str)] = &[
    (
        "two hops",
        "SELECT CAST(t2.\"name\" AS VARCHAR) AS \"d0\", CAST(sum(t0.\"amount\") AS DOUBLE) AS \"m1\" FROM \"s_s1\".\"orders\" AS t0 LEFT JOIN \"s_s2\".\"stores\" AS t1 ON t0.\"store_id\" = t1.\"id\" LEFT JOIN \"s_s3\".\"regions\" AS t2 ON t1.\"region_code\" = t2.\"code\" GROUP BY 1 ORDER BY 2 DESC NULLS LAST, 1 ASC NULLS LAST LIMIT 1000\n  params: []",
    ),
    (
        "month, a filter and a sort",
        "SELECT strftime(date_trunc('month', t0.\"date\"), '%Y-%m-%d') AS \"d0\", CAST(sum(t0.\"amount\") AS DOUBLE) AS \"m1\" FROM \"s_s1\".\"orders\" AS t0 LEFT JOIN \"s_s2\".\"stores\" AS t1 ON t0.\"store_id\" = t1.\"id\" WHERE (CAST(t1.\"city\" AS VARCHAR) IN (?) OR CAST(t1.\"city\" AS VARCHAR) IS NULL) GROUP BY 1 ORDER BY 2 DESC NULLS LAST, 1 ASC NULLS LAST LIMIT 12\n  params: [Text(\"Jakarta\")]",
    ),
    (
        "a ratio across two links",
        "SELECT CAST(t1.\"city\" AS VARCHAR) AS \"d0\", CAST(t2.\"name\" AS VARCHAR) AS \"d1\", (CAST(sum(t0.\"amount\") AS DOUBLE)) / NULLIF((CAST(count(DISTINCT t3.\"name\") AS DOUBLE)), 0) AS \"m2\" FROM \"s_s1\".\"orders\" AS t0 LEFT JOIN \"s_s2\".\"stores\" AS t1 ON t0.\"store_id\" = t1.\"id\" LEFT JOIN \"s_s3\".\"regions\" AS t2 ON t1.\"region_code\" = t2.\"code\" LEFT JOIN \"s_s4\".\"customers\" AS t3 ON t0.\"customer\" = t3.\"name\" WHERE CAST(t0.\"amount\" AS DOUBLE) BETWEEN ? AND ? AND NOT COALESCE((CAST(t3.\"tier\" AS VARCHAR) IN (?)), FALSE) GROUP BY 1, 2 ORDER BY 3 DESC NULLS LAST, 1 ASC NULLS LAST, 2 ASC NULLS LAST LIMIT 1000\n  params: [Double(10.0), Double(500.0), Text(\"a\")]",
    ),
    (
        "groups only",
        "SELECT DISTINCT CAST(t0.\"city\" AS VARCHAR) AS \"d0\", CAST(t1.\"name\" AS VARCHAR) AS \"d1\" FROM \"s_s2\".\"stores\" AS t0 LEFT JOIN \"s_s3\".\"regions\" AS t1 ON t0.\"region_code\" = t1.\"code\" ORDER BY 1 ASC, 2 ASC NULLS LAST LIMIT 1000\n  params: []",
    ),
    (
        "rows of a table that other tables point at, by a column of the table it points at",
        "SELECT CAST(t1.\"name\" AS VARCHAR) AS \"d0\", CAST(count(*) AS DOUBLE) AS \"m1\" FROM \"s_s2\".\"stores\" AS t0 LEFT JOIN \"s_s3\".\"regions\" AS t1 ON t0.\"region_code\" = t1.\"code\" GROUP BY 1 ORDER BY 2 DESC NULLS LAST, 1 ASC NULLS LAST LIMIT 1000\n  params: []",
    ),
    (
        "a number joined to text",
        "SELECT CAST(t1.\"owner\" AS VARCHAR) AS \"d0\", CAST(avg(t0.\"amount\") AS DOUBLE) AS \"m1\" FROM \"s_s1\".\"orders\" AS t0 LEFT JOIN \"s_s5\".\"shops\" AS t1 ON CAST(t0.\"store_id\" AS VARCHAR) = CAST(t1.\"code\" AS VARCHAR) GROUP BY 1 ORDER BY 2 DESC NULLS LAST, 1 ASC NULLS LAST LIMIT 1000\n  params: []",
    ),
    (
        "a field of a table nobody points to",
        "error: orders is not related to regions. Draw a line between them in Model.",
    ),
];

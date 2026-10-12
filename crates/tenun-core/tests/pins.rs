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
            table(
                "s6/transfers",
                &[
                    ("from_store", Number),
                    ("to_store", Number),
                    ("qty", Number),
                ],
            ),
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
        // Two links between the same two tables, as an old file can have them: neither is named.
        rel("r5", ("s6/transfers", "from_store"), ("s2/stores", "id")),
        rel("r6", ("s6/transfers", "to_store"), ("s2/stores", "id")),
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

/// The queries of a file with two links to one table and no names: no `via` means the first link
/// that was made, and the SQL is what it was before links could be named. Written from the rule of
/// that code (the first link claims the table), and the engine agrees with it.
#[test]
fn two_unnamed_links_to_one_table_read_the_first_link_as_they_always_did() {
    let (cat, rels) = world();
    let today = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
    let by_city = |via: Option<&str>| -> SemanticQuery {
        serde_json::from_value(json!({
            "measures": [{"table": "s6/transfers", "column": "qty", "agg": "sum"}],
            "dimensions": [{"table": "s2/stores", "column": "city", "via": via}],
        }))
        .unwrap()
    };
    let first = "SELECT CAST(t1.\"city\" AS VARCHAR) AS \"d0\", CAST(sum(t0.\"qty\") AS DOUBLE) AS \"m1\" \
                 FROM \"s_s6\".\"transfers\" AS t0 \
                 LEFT JOIN \"s_s2\".\"stores\" AS t1 ON t0.\"from_store\" = t1.\"id\" \
                 GROUP BY 1 ORDER BY 2 DESC NULLS LAST, 1 ASC NULLS LAST LIMIT 1000";
    let sql = |q: &SemanticQuery| compile_on(q, &cat, &rels, today).unwrap().sql;
    assert_eq!(sql(&by_city(None)), first);
    // The same query with no `via` key at all, as an old file writes it.
    let old: SemanticQuery = serde_json::from_value(json!({
        "measures": [{"table": "s6/transfers", "column": "qty", "agg": "sum"}],
        "dimensions": [{"table": "s2/stores", "column": "city"}],
    }))
    .unwrap();
    assert_eq!(sql(&old), first);
    // Naming the first link changes nothing.
    assert_eq!(sql(&by_city(Some("r5"))), first);
    // The shape of a query through the second link: the same joins, the other column.
    assert_eq!(
        sql(&by_city(Some("r6"))),
        first.replace("t0.\"from_store\"", "t0.\"to_store\"")
    );
    // Both links in one query are two joins of one table, each with its own alias.
    let both: SemanticQuery = serde_json::from_value(json!({
        "measures": [{"table": "s6/transfers", "column": "qty", "agg": "sum"}],
        "dimensions": [
            {"table": "s2/stores", "column": "city", "via": "r5"},
            {"table": "s2/stores", "column": "city", "via": "r6"}
        ],
    }))
    .unwrap();
    assert_eq!(
        sql(&both),
        "SELECT CAST(t1.\"city\" AS VARCHAR) AS \"d0\", CAST(t2.\"city\" AS VARCHAR) AS \"d1\", \
         CAST(sum(t0.\"qty\") AS DOUBLE) AS \"m2\" FROM \"s_s6\".\"transfers\" AS t0 \
         LEFT JOIN \"s_s2\".\"stores\" AS t1 ON t0.\"from_store\" = t1.\"id\" \
         LEFT JOIN \"s_s2\".\"stores\" AS t2 ON t0.\"to_store\" = t2.\"id\" \
         GROUP BY 1, 2 ORDER BY 3 DESC NULLS LAST, 1 ASC NULLS LAST, 2 ASC NULLS LAST LIMIT 1000"
    );
}

/// A link whose two sides are both unique reads the same way forward as it did when nothing was
/// known about the data. It also works backward, which is new: a query that could not be
/// answered can be now.
#[test]
fn a_one_to_one_link_joins_forward_exactly_as_before() {
    let (cat, rels) = world();
    let today = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
    let health: Health = rels
        .iter()
        .map(|r| {
            (
                r.id.clone(),
                LinkHealth {
                    one_rows: 50,
                    many_rows: 50,
                    ..LinkHealth::default()
                },
            )
        })
        .collect();
    for (name, q) in queries() {
        let q: SemanticQuery = serde_json::from_value(q).unwrap();
        let plain = compile_on(&q, &cat, &rels, today);
        let both = compile_on(&q, &cat, Links::new(&rels, Some(&health)), today);
        match plain {
            // A query that was answered, with every field read, is written as it was.
            Ok(p) if p.ignored_filters.is_empty() => {
                let b = both.unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!((p.sql, p.params), (b.sql, b.params), "{name}");
            }
            // A filter that could not reach the table was left out. Now it can: more is read.
            Ok(p) => {
                let b = both.unwrap_or_else(|e| panic!("{name}: {e}"));
                assert!(b.ignored_filters.len() < p.ignored_filters.len(), "{name}");
            }
            // Reading the regions through the stores to the orders was not possible, and is now.
            Err(_) => assert!(both.is_ok(), "{name}"),
        }
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

/// A file made before calculated columns existed: a Tenun 0.3.0 file, and one from the build that
/// added named links. Each is read and written back byte for byte, so opening and saving an old
/// workspace never changes it (V3-D11). The texts are written out by hand; they are the pin.
const FILE_V3: &str = r##"{
  "version": 1,
  "name": "Coffee",
  "sources": [
    {
      "id": "s1",
      "name": "coffee.xlsx",
      "kind": "excel",
      "path": "coffee.xlsx"
    },
    {
      "id": "s2",
      "name": "stores.csv",
      "kind": "csv",
      "path": "stores.csv"
    }
  ],
  "relationships": [
    {
      "id": "r1",
      "from": {
        "table": "s1/Orders",
        "column": "store_id"
      },
      "to": {
        "table": "s2/stores",
        "column": "id"
      }
    }
  ],
  "dashboards": [
    {
      "id": "d1",
      "refresh": 60,
      "tiles": [
        {
          "dims": [],
          "h": 2,
          "id": "t1",
          "measures": [
            {
              "agg": "sum",
              "column": "amount",
              "table": "s1/Orders"
            }
          ],
          "title": "",
          "type": "kpi",
          "w": 3,
          "x": 0,
          "y": 0
        }
      ],
      "title": "Coffee overview"
    }
  ],
  "layout": {
    "model": {
      "s1/Orders": {
        "x": 0,
        "y": 60
      }
    }
  },
  "locale": "id",
  "tables": {
    "s1/Orders": {
      "header_row": 4,
      "columns": {
        "amount": {
          "label": "Sales",
          "dtype": "number",
          "hidden": true,
          "description": "What the customer paid"
        },
        "date": {
          "dtype": "date",
          "date_format": "dmy"
        }
      },
      "transforms": [
        {
          "kind": "unpivot",
          "keep": [
            "store_id"
          ],
          "name": "Month",
          "value": "Sales"
        }
      ]
    },
    "s2/stores": {
      "columns": {
        "id": {
          "label": "Store"
        }
      }
    }
  }
}"##;

const FILE_NAMED_LINKS: &str = r##"{
  "version": 1,
  "name": "Coffee",
  "sources": [
    {
      "id": "s1",
      "name": "coffee.xlsx",
      "kind": "excel",
      "path": "coffee.xlsx"
    },
    {
      "id": "s2",
      "name": "stores.csv",
      "kind": "csv",
      "path": "stores.csv"
    }
  ],
  "relationships": [
    {
      "id": "r1",
      "from": {
        "table": "s1/Orders",
        "column": "store_id"
      },
      "to": {
        "table": "s2/stores",
        "column": "id"
      },
      "name": "Sold at"
    },
    {
      "id": "r2",
      "from": {
        "table": "s1/Orders",
        "column": "ship_store_id"
      },
      "to": {
        "table": "s2/stores",
        "column": "id"
      },
      "name": "Shipped from"
    }
  ],
  "dashboards": [
    {
      "id": "d1",
      "refresh": 60,
      "tiles": [
        {
          "dims": [],
          "h": 2,
          "id": "t1",
          "measures": [
            {
              "agg": "sum",
              "column": "amount",
              "table": "s1/Orders"
            }
          ],
          "title": "",
          "type": "kpi",
          "w": 3,
          "x": 0,
          "y": 0
        }
      ],
      "title": "Coffee overview"
    }
  ],
  "layout": {
    "model": {
      "s1/Orders": {
        "x": 0,
        "y": 60
      }
    }
  },
  "locale": "id",
  "tables": {
    "s1/Orders": {
      "header_row": 4,
      "columns": {
        "amount": {
          "label": "Sales",
          "dtype": "number",
          "hidden": true,
          "description": "What the customer paid"
        },
        "date": {
          "dtype": "date",
          "date_format": "dmy"
        }
      },
      "transforms": [
        {
          "kind": "unpivot",
          "keep": [
            "store_id"
          ],
          "name": "Month",
          "value": "Sales"
        }
      ]
    },
    "s2/stores": {
      "columns": {
        "id": {
          "label": "Store"
        }
      }
    }
  }
}"##;

#[test]
fn an_old_file_is_written_back_byte_for_byte_when_no_calculated_column_is_added() {
    use tenun_core::workspace::Workspace;
    let dir = std::env::temp_dir().join(format!("tenun-pin-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in [("v3", FILE_V3), ("named-links", FILE_NAMED_LINKS)] {
        let path = dir.join(format!("{name}.tenun"));
        std::fs::write(&path, text).unwrap();
        let ws = Workspace::load(&path).unwrap();
        // Nothing of the new field is in the file, and the setup of a table has none.
        assert!(ws.tables.values().all(|t| t.calc.is_empty()), "{name}");
        let back = dir.join(format!("{name}-again.tenun"));
        ws.save(&back).unwrap();
        assert_eq!(std::fs::read_to_string(&back).unwrap(), text, "{name}");
    }
}

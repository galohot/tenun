//! Links that stay honest (V4-D26 to V4-D28): a key that repeats on the one side never doubles a
//! total, one to one links work both ways, and one table can be reached through two named links.
//!
//! Small CSV files stand for orders, stores and transfers between stores. Every expected number
//! is worked out by hand in the comments, so a wrong join cannot hide behind a wrong test.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tenun::{Options, Running};
use tenun_core::Engine;
use tenun_core::engine::{AddSource, QueryResult};
use tenun_core::filters;
use tenun_core::formula::{CheckReply, CheckRequest, Kind};
use tenun_core::query::Filter;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile};
use tenun_core::workspace::{FieldRef, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-links-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Orders by store: store 1 has 100 + 250, store 2 has 40 + 10, store 3 has 75.5. In all 475.5.
const ORDERS: &str = "id,store_id,amount\n1,1,100\n2,1,250\n3,2,40\n4,3,75.5\n5,2,10\n";

/// Kopi A and C are in the West, Kopi B in the East. By region: West 350 + 75.5 = 425.5, East 50.
const STORES: &str = "id,name,region\n1,Kopi A,West\n2,Kopi B,East\n3,Kopi C,West\n";

/// Who sent what to whom. Store 3 never sends.
///
/// Quantity by the region of the store that sent: store 1 (West) 10 + 5 + 3 = 18, store 2 (East)
/// 7 + 4 = 11. By the region of the store that received: store 2 (East) 10 + 3 = 13, store 3
/// (West) 5 + 4 = 9, store 1 (West) 7, so West 16 and East 13. In all 29.
const TRANSFERS: &str = "from_store,to_store,qty,date\n1,2,10,2026-01-05\n1,3,5,2026-01-06\n\
                         2,1,7,2026-01-07\n2,3,4,2026-01-08\n1,2,3,2026-01-09\n";

struct Lab {
    e: Engine,
    d: PathBuf,
}

fn field(t: &str, c: &str) -> FieldRef {
    FieldRef {
        table: t.into(),
        column: c.into(),
    }
}

impl Lab {
    /// Orders (`s1/orders`), stores (`s2/stores`), transfers (`s3/transfers`). No links yet.
    fn new(tag: &str) -> Lab {
        let d = dir(tag);
        std::fs::write(d.join("orders.csv"), ORDERS).unwrap();
        std::fs::write(d.join("stores.csv"), STORES).unwrap();
        std::fs::write(d.join("transfers.csv"), TRANSFERS).unwrap();
        let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
        for f in ["orders.csv", "stores.csv", "transfers.csv"] {
            e.add_source(AddSource::File {
                path: d.join(f).to_string_lossy().into_owned(),
            })
            .unwrap();
        }
        assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
        Lab { e, d }
    }

    /// The link from the orders to the stores. Its id is `r1`.
    fn with_order_link(self) -> Lab {
        let id = self
            .e
            .add_relationship(field("s1/orders", "store_id"), field("s2/stores", "id"))
            .unwrap();
        assert_eq!(id, "r1");
        self
    }

    /// The two links from the transfers to the stores: `r1` (from_store) and `r2` (to_store).
    fn with_transfer_links(self) -> Lab {
        for col in ["from_store", "to_store"] {
            self.e
                .add_relationship(field("s3/transfers", col), field("s2/stores", "id"))
                .unwrap();
        }
        self
    }

    /// A save of a file while the app is open: the file changes and its source loads again.
    fn save(&self, file: &str, text: &str, source: &str) {
        std::fs::write(self.d.join(file), text).unwrap();
        self.e.load(source);
    }

    fn note(&self, link: &str) -> Option<String> {
        self.e
            .snapshot()
            .health
            .get(link)
            .and_then(|h| h.note.clone())
    }
}

fn tile(measure: Value, dims: Value) -> Value {
    json!({ "id": "t1", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
            "measures": [measure], "dims": dims })
}

fn run(e: &Engine, tile: Value, filters: Vec<DashFilter>) -> anyhow::Result<QueryResult> {
    let t: Tile = serde_json::from_value(tile).unwrap();
    run_tile(e, &t, &filters)
}

fn sum_amount() -> Value {
    json!({ "table": "s1/orders", "column": "amount", "agg": "sum" })
}

fn by_region_of_store() -> Value {
    json!([{ "table": "s2/stores", "column": "region" }])
}

/// The rows of a result with one group and one value, as a map.
fn groups(res: &QueryResult) -> BTreeMap<String, f64> {
    res.rows
        .iter()
        .map(|r| {
            (
                r[0].as_str().unwrap_or("(blank)").to_string(),
                r[1].as_f64().unwrap(),
            )
        })
        .collect()
}

fn map(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

fn click(table: &str, column: &str, via: Option<&str>, value: &str) -> DashFilter {
    serde_json::from_value(json!({
        "table": table, "column": column, "op": "in", "values": [value],
        "via": via, "fromTile": "other"
    }))
    .unwrap()
}

// ---- a key that repeats on the one side ---------------------------------------------------

#[test]
fn a_repeated_store_id_never_doubles_a_total_and_the_tile_says_so() {
    let lab = Lab::new("dup").with_order_link();
    let before = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
    assert_eq!(groups(&before), map(&[("West", 425.5), ("East", 50.0)]));
    assert!(before.notes.is_empty());
    assert!(lab.note("r1").is_none());

    // Someone adds a second row for store 1 to the file while the app is open. A naive join
    // would count the orders of store 1 twice: 350 more.
    lab.save(
        "stores.csv",
        &format!("{STORES}1,Kopi A (copy),East\n"),
        "s2",
    );
    let after = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
    // The first row of store 1 is the one that counts: West. The totals are the same.
    assert_eq!(groups(&after), map(&[("West", 425.5), ("East", 50.0)]));
    // Two of the five orders belong to store 1: the reader can weigh the warning.
    let note = "stores.csv has 2 rows with id 1. Tenun used the first. 2 of 5 orders rows meet a repeated key. Fix the file to remove this note.";
    assert_eq!(after.notes, [note]);
    // What would leave the computer has no key and no file name.
    assert_eq!(
        after.public_notes,
        [
            "The stores table repeats 1 value of id. Tenun used the first row of it. 2 of 5 orders rows meet a repeated key."
        ]
    );

    let snap = lab.e.snapshot();
    let h = &snap.health["r1"];
    assert_eq!(h.duplicates, 1);
    assert_eq!(h.sample, [json!("1")]);
    assert_eq!(h.sample_rows, [2]);
    assert!(!h.one_to_one && !h.from_unique && !h.unusable);
    assert_eq!(h.kind, "one-many");
    assert_eq!(h.note.as_deref(), Some(note));

    // A tile that does not cross the link says nothing, and neither does a total of orders alone.
    let orders_only = run(&lab.e, tile(sum_amount(), json!([])), vec![]).unwrap();
    assert!(orders_only.notes.is_empty());

    // The duplicate goes: the note goes, the totals stay.
    lab.save("stores.csv", STORES, "s2");
    let fixed = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
    assert_eq!(groups(&fixed), map(&[("West", 425.5), ("East", 50.0)]));
    assert!(fixed.notes.is_empty());
    assert!(lab.note("r1").is_none());
    assert_eq!(lab.e.snapshot().health["r1"].duplicates, 0);
}

#[test]
fn several_repeated_keys_are_listed_with_the_most_repeated_first() {
    let lab = Lab::new("many-dups").with_order_link();
    // Fourteen rows for nine stores: ids 1 (three rows), 2, 4 and 5 (two rows each) repeat, so
    // five rows are too many. Fewer than half of the rows repeat, so the stores are still a key.
    lab.save(
        "stores.csv",
        "id,name,region\n1,a,West\n1,b,West\n1,c,West\n2,d,East\n2,e,East\n3,f,West\n\
         4,g,East\n4,h,East\n5,i,East\n5,j,East\n6,k,East\n7,l,East\n8,m,East\n9,n,East\n",
        "s2",
    );
    let h = lab.e.snapshot().health["r1"].clone();
    assert_eq!((h.duplicates, h.keys), (5, 4));
    assert_eq!(h.sample, [json!("1"), json!("2"), json!("4"), json!("5")]);
    assert_eq!(h.sample_rows, [3, 2, 2, 2]);
    // Four of the five orders (stores 1, 1, 2, 2) meet one of those keys.
    assert_eq!(
        h.note.unwrap(),
        "stores.csv repeats id: 1, 2, 4 and 5 (5 rows too many). Tenun used the first row of each. 4 of 5 orders rows meet a repeated key. Fix the file to remove this note."
    );
    // Orders still add up.
    let r = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
    assert_eq!(groups(&r), map(&[("West", 425.5), ("East", 50.0)]));
}

#[test]
fn a_column_that_is_mostly_repeated_keys_is_not_a_key_and_the_link_is_not_used() {
    let lab = Lab::new("many-to-many").with_order_link();
    // Three of the four distinct ids repeat (1, 2 and 3 twice): more than half. The stores table
    // is not a list of stores any more, and the orders repeat their store too: many to many.
    lab.save(
        "stores.csv",
        "id,name,region\n1,a,West\n1,b,East\n2,c,West\n2,d,East\n3,e,West\n3,f,East\n4,g,East\n",
        "s2",
    );
    let h = lab.e.snapshot().health["r1"].clone();
    assert!(h.unusable);
    assert_eq!(h.kind, "many-many");
    // The keys are named even though the link is not used.
    assert_eq!(h.sample, [json!("1"), json!("2"), json!("3")]);
    assert_eq!(
        h.note.unwrap(),
        "This link was one to many and is now many to many. orders.csv and stores.csv both repeat the values of this link (store_id and id), so Tenun cannot tell which row to use. Repeated id: 1, 2 and 3. The link is not used until one of them is fixed."
    );
    let err = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap_err();
    assert_eq!(
        err.to_string(),
        "stores cannot be used with orders now: the link between them repeats its values on both sides. Open Model to see which file to fix."
    );
    // The link is still saved. It works again when the file is right.
    assert_eq!(lab.e.snapshot().workspace.relationships.len(), 1);
    lab.save("stores.csv", STORES, "s2");
    let ok = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
    assert_eq!(groups(&ok), map(&[("West", 425.5), ("East", 50.0)]));
}

#[test]
fn one_key_repeated_ten_times_among_real_ids_is_a_warning_and_never_a_dead_link() {
    let lab = Lab::new("ten-rows").with_order_link();
    // The stores file has its three real stores, and ten rows of a placeholder id, 99. One key
    // repeats among four. The link must go on working: totals unchanged, a warning on the tiles.
    let mut text = String::from(STORES);
    for i in 0..10 {
        text.push_str(&format!("99,Unknown {i},East\n"));
    }
    lab.save("stores.csv", &text, "s2");
    let h = lab.e.snapshot().health["r1"].clone();
    assert!(!h.unusable);
    assert_eq!(h.kind, "one-many");
    assert_eq!((h.duplicates, h.keys), (9, 1));
    let r = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
    assert_eq!(groups(&r), map(&[("West", 425.5), ("East", 50.0)]));
    // No order has store 99.
    assert_eq!(
        r.notes,
        [
            "stores.csv has 10 rows with id 99. Tenun used the first. No orders row meets a repeated key. Fix the file to remove this note."
        ]
    );
}

// ---- one to one ---------------------------------------------------------------------------

#[test]
fn a_one_to_one_link_works_from_either_table_and_one_way_after_a_repeat() {
    let lab = Lab::new("one-one");
    // One card for each store: both columns are unique.
    std::fs::write(
        lab.d.join("cards.csv"),
        "store_id,card\n1,visa\n2,amex\n3,visa2\n",
    )
    .unwrap();
    lab.e
        .add_source(AddSource::File {
            path: lab.d.join("cards.csv").to_string_lossy().into_owned(),
        })
        .unwrap();
    lab.e
        .add_relationship(field("s4/cards", "store_id"), field("s2/stores", "id"))
        .unwrap();
    assert_eq!(lab.e.snapshot().health["r1"].kind, "one-one");
    assert!(lab.e.snapshot().health["r1"].one_to_one);

    // Cards by region of the store, from the cards (the stored way)...
    let cards_count = json!({ "table": "s4/cards", "column": null, "agg": "count" });
    let r = run(&lab.e, tile(cards_count, by_region_of_store()), vec![]).unwrap();
    assert_eq!(groups(&r), map(&[("West", 2.0), ("East", 1.0)]));
    // ...and stores by card, from the stores (the other way). One card each.
    let stores_count = json!({ "table": "s2/stores", "column": null, "agg": "count" });
    let by_card = json!([{ "table": "s4/cards", "column": "card" }]);
    let r = run(&lab.e, tile(stores_count.clone(), by_card.clone()), vec![]).unwrap();
    assert_eq!(
        groups(&r),
        map(&[("visa", 1.0), ("amex", 1.0), ("visa2", 1.0)])
    );

    // A card is written twice for store 2. The cards are no longer unique: the link goes one
    // way again, and a tile that starts at the stores says why it cannot reach the cards.
    lab.save(
        "cards.csv",
        "store_id,card\n1,visa\n2,amex\n2,amex2\n3,visa2\n",
        "s4",
    );
    assert_eq!(lab.e.snapshot().health["r1"].kind, "one-many");
    let err = run(&lab.e, tile(stores_count, by_card), vec![]).unwrap_err();
    // The words say which table repeats its key, not that the tables are not related (case C).
    assert_eq!(
        err.to_string(),
        "Tenun cannot read cards from stores: cards repeats store_id. See the link between them in Model."
    );
    // And the link says that it changed.
    assert_eq!(
        lab.note("r1").unwrap(),
        "This link was one to one and is now one to many. A tile that reads the many side from the one side cannot do it now."
    );
    // The stored way still works.
    let cards_count = json!({ "table": "s4/cards", "column": null, "agg": "count" });
    let r = run(&lab.e, tile(cards_count, by_region_of_store()), vec![]).unwrap();
    assert_eq!(groups(&r), map(&[("West", 2.0), ("East", 2.0)]));
}

// ---- named links --------------------------------------------------------------------------

#[test]
fn two_links_to_one_table_are_two_roles_with_their_own_numbers() {
    let lab = Lab::new("roles").with_transfer_links();
    lab.e
        .rename_link("r1", Some("Shipped from".into()))
        .unwrap();
    lab.e.rename_link("r2", Some("Sold at".into())).unwrap();
    let qty = json!({ "table": "s3/transfers", "column": "qty", "agg": "sum" });
    let region =
        |via: Option<&str>| json!({ "table": "s2/stores", "column": "region", "via": via });

    let from = run(
        &lab.e,
        tile(qty.clone(), json!([region(Some("r1"))])),
        vec![],
    )
    .unwrap();
    assert_eq!(groups(&from), map(&[("West", 18.0), ("East", 11.0)]));
    let to = run(
        &lab.e,
        tile(qty.clone(), json!([region(Some("r2"))])),
        vec![],
    )
    .unwrap();
    assert_eq!(groups(&to), map(&[("West", 16.0), ("East", 13.0)]));
    // No via: the first link, as before links had names.
    let plain = run(&lab.e, tile(qty.clone(), json!([region(None)])), vec![]).unwrap();
    assert_eq!(groups(&plain), groups(&from));

    // Both in one tile: from region by to region. Two joins of one table, two aliases.
    let both = run(
        &lab.e,
        tile(qty.clone(), json!([region(Some("r1")), region(Some("r2"))])),
        vec![],
    )
    .unwrap();
    let cells: BTreeMap<(String, String), f64> = both
        .rows
        .iter()
        .map(|r| {
            (
                (r[0].as_str().unwrap().into(), r[1].as_str().unwrap().into()),
                r[2].as_f64().unwrap(),
            )
        })
        .collect();
    // Store 1 (West) to store 2 (East): 10 + 3. West to West: 5. East to West: 7 + 4.
    assert_eq!(cells.len(), 3);
    assert_eq!(cells[&("West".into(), "East".into())], 13.0);
    assert_eq!(cells[&("West".into(), "West".into())], 5.0);
    assert_eq!(cells[&("East".into(), "West".into())], 11.0);
    assert_eq!(cells.values().sum::<f64>(), 29.0);

    // A click on a role filters by that role only. West as the sender: store 1, so its
    // transfers (10 + 5 + 3 = 18), which went to store 2 (East, 13) and store 3 (West, 5).
    let by_to = tile(qty.clone(), json!([region(Some("r2"))]));
    let r = run(
        &lab.e,
        by_to.clone(),
        vec![click("s2/stores", "region", Some("r1"), "West")],
    )
    .unwrap();
    assert_eq!(groups(&r), map(&[("East", 13.0), ("West", 5.0)]));
    // West as the receiver: stores 3 and 1: 5 + 4 + 7 = 16, all of it grouped by the receiver.
    let r = run(
        &lab.e,
        by_to.clone(),
        vec![click("s2/stores", "region", Some("r2"), "West")],
    )
    .unwrap();
    assert_eq!(groups(&r), map(&[("West", 16.0)]));

    // "Show rows" behind a mark reads the same role, and names the columns of that role: three
    // transfers went to a store in the West (1 to 3, 2 to 3, 2 to 1).
    let t: Tile = serde_json::from_value(by_to).unwrap();
    let click: Filter = serde_json::from_value(json!({
        "table": "s2/stores", "column": "region", "op": "in", "values": ["West"], "via": "r2"
    }))
    .unwrap();
    let rows = filters::rows_behind(&lab.e, &t, &[], Some(click)).unwrap();
    assert_eq!(rows.total, 3);
    assert_eq!(rows.columns.last().unwrap().label, "Sold at · region");
}

#[test]
fn old_files_with_two_unnamed_links_keep_the_numbers_of_the_first_link() {
    let lab = Lab::new("old").with_transfer_links();
    let qty = json!({ "table": "s3/transfers", "column": "qty", "agg": "sum" });
    let r = run(
        &lab.e,
        tile(qty, json!([{ "table": "s2/stores", "column": "region" }])),
        vec![],
    )
    .unwrap();
    assert_eq!(groups(&r), map(&[("West", 18.0), ("East", 11.0)]));
    assert!(r.notes.is_empty());
}

#[test]
fn a_link_gets_a_name_that_is_new_and_a_pair_of_links_keeps_both() {
    let lab = Lab::new("names").with_transfer_links();
    let e = &lab.e;
    // Words are cleaned up: spaces at the ends and doubled spaces go.
    e.rename_link("r1", Some("  Shipped   from ".into()))
        .unwrap();
    assert_eq!(
        e.snapshot().workspace.relationships[0].name.as_deref(),
        Some("Shipped from")
    );
    // Another link has this name, ignoring case.
    let err = e
        .rename_link("r2", Some("shipped FROM".into()))
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Another link is called shipped FROM. Give this one another name."
    );
    // A table has this name: a formula could not tell them apart.
    let err = e.rename_link("r2", Some("Stores".into())).unwrap_err();
    assert!(
        err.to_string().starts_with("A table is called Stores."),
        "{err}"
    );
    // The name goes only when the link is alone between its tables.
    let err = e.rename_link("r1", None).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Two links join these tables, so each one needs a name. Give this link a name."
    );
    assert!(e.rename_link("r9", Some("x".into())).is_err());
    assert!(e.rename_link("r2", Some("x".repeat(61))).is_err());
    e.rename_link("r2", Some("Sold at".into())).unwrap();
    // Removing one link leaves the other free to lose its name.
    e.remove_relationship("r2");
    e.rename_link("r1", None).unwrap();
    assert_eq!(e.snapshot().workspace.relationships[0].name, None);
    // A name that is not set is not written to the file.
    let text = serde_json::to_string(&e.snapshot().workspace).unwrap();
    assert!(
        !text.contains("\"name\":null") && !text.contains("\"via\""),
        "{text}"
    );
}

#[test]
fn a_formula_can_name_a_link_and_a_table_two_links_lead_to_is_an_error() {
    let lab = Lab::new("formula").with_transfer_links();
    let check = |formula: &str, kind: Kind, table: Option<&str>| -> CheckReply {
        lab.e
            .check_formula(&CheckRequest {
                formula: formula.into(),
                kind,
                table: table.map(String::from),
            })
            .unwrap()
    };

    // Two links and no names: the table name is ambiguous, and the words say how to fix it.
    let r = check("[stores].[region]", Kind::Column, Some("transfers"));
    assert!(!r.ok);
    assert_eq!(
        r.error.unwrap().message,
        "[stores].[region] could mean two links: \"stores (from_store)\" or \"stores (to_store)\". Give the links a name in Model, then write the name of the link instead of the table."
    );

    lab.e
        .rename_link("r1", Some("Shipped from".into()))
        .unwrap();
    lab.e.rename_link("r2", Some("Sold at".into())).unwrap();
    let r = check("[stores].[region]", Kind::Column, Some("transfers"));
    assert_eq!(
        r.error.unwrap().message,
        "[stores].[region] could mean two links: \"Shipped from\" or \"Sold at\". Write the name of the link instead of the table, like [Shipped from].[region]."
    );

    // The name of a link reads the table through that link. The preview holds all five rows (the
    // order of a join is not promised, so the names are sorted): the stores that sent were Kopi A
    // three times and Kopi B twice; the stores that received were A once, B twice and C twice.
    let sorted = |r: CheckReply| -> Vec<String> {
        let mut v: Vec<String> = r
            .preview
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect();
        v.sort();
        v
    };
    let r = check("[Shipped from].[name]", Kind::Column, Some("transfers"));
    assert!(r.ok, "{r:?}");
    assert_eq!(
        sorted(r),
        ["Kopi A", "Kopi A", "Kopi A", "Kopi B", "Kopi B"]
    );
    let r = check("[sold at].[name]", Kind::Column, Some("transfers"));
    assert_eq!(
        sorted(r),
        ["Kopi A", "Kopi B", "Kopi B", "Kopi C", "Kopi C"]
    );
    // Both in one formula use two joins, each row read twice: West to East is store 1 to store 2.
    let r = check(
        "[Shipped from].[region] & \" to \" & [Sold at].[region]",
        Kind::Column,
        Some("transfers"),
    );
    assert_eq!(
        sorted(r),
        [
            "East to West",
            "East to West",
            "West to East",
            "West to East",
            "West to West"
        ]
    );

    // In a total: how many different stores sent something, and how many received something.
    let r = check(
        "COUNTD([Shipped from].[id])",
        Kind::Measure,
        Some("transfers"),
    );
    assert!(r.ok, "{r:?}");
    assert_eq!(r.preview.unwrap(), [json!(2.0)]);
    let r = check("COUNTD([Sold at].[id])", Kind::Measure, Some("transfers"));
    assert_eq!(r.preview.unwrap(), [json!(3.0)]);
    // A total that reads the stores through the transfers is ambiguous too, with the table name.
    let r = check(
        "SUMIF([transfers].[qty], [stores].[region] = \"West\")",
        Kind::Measure,
        None,
    );
    assert!(!r.ok, "{r:?}");
    assert!(r.error.unwrap().message.contains("could mean two links"));
    // A link name says which: the quantity sent from the West is stores 1: 10 + 5 + 3.
    let r = check(
        "SUMIF([transfers].[qty], [Shipped from].[region] = \"West\")",
        Kind::Measure,
        None,
    );
    assert!(r.ok, "{r:?}");
    assert_eq!(r.preview.unwrap(), [json!(18.0)]);
    let r = check(
        "SUMIF([transfers].[qty], [Sold at].[region] = \"West\")",
        Kind::Measure,
        None,
    );
    assert_eq!(r.preview.unwrap(), [json!(16.0)]);

    // A link name that is nowhere gets a suggestion that includes the names of links.
    let r = check("[Shiped from].[region]", Kind::Column, Some("transfers"));
    assert_eq!(
        r.error.unwrap().message,
        "No table [Shiped from]. Did you mean [Shipped from]?"
    );
}

#[test]
fn a_link_with_one_path_needs_no_name_in_a_formula() {
    let lab = Lab::new("formula-one").with_order_link();
    let r = lab
        .e
        .check_formula(&CheckRequest {
            formula: "[stores].[region]".into(),
            kind: Kind::Column,
            table: Some("orders".into()),
        })
        .unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(r.preview.unwrap()[0], json!("West"));
}

// ---- files and snapshots ------------------------------------------------------------------

#[test]
fn an_old_workspace_file_is_written_back_byte_for_byte() {
    // A workspace file exactly as the previous version wrote it (Tenun 0.3.0): two links with no
    // names, a tile with a ratio and no `via`, a locale and a column label.
    let old = r#"{
  "version": 1,
  "name": "Coffee",
  "sources": [
    {
      "id": "s1",
      "name": "orders.csv",
      "kind": "csv",
      "path": "orders.csv"
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
        "table": "s1/orders",
        "column": "store_id"
      },
      "to": {
        "table": "s2/stores",
        "column": "id"
      }
    },
    {
      "id": "r2",
      "from": {
        "table": "s1/orders",
        "column": "id"
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
      "tiles": [
        {
          "dims": [
            {
              "column": "region",
              "grain": null,
              "table": "s2/stores"
            }
          ],
          "filter": {
            "style": "list"
          },
          "id": "t1",
          "measures": [
            {
              "agg": "sum",
              "column": "amount",
              "per": {
                "agg": "count",
                "column": null,
                "table": "s1/orders"
              },
              "table": "s1/orders"
            }
          ],
          "type": "bar"
        }
      ],
      "title": "Overview"
    }
  ],
  "layout": {
    "model": {
      "s1/orders": {
        "x": 0,
        "y": 0
      }
    }
  },
  "locale": "id",
  "tables": {
    "s1/orders": {
      "columns": {
        "amount": {
          "label": "Total"
        }
      }
    }
  }
}"#;
    let d = dir("roundtrip");
    std::fs::write(d.join("orders.csv"), ORDERS).unwrap();
    std::fs::write(d.join("stores.csv"), STORES).unwrap();
    let path = d.join("old.tenun");
    std::fs::write(&path, old).unwrap();
    // Straight through the file format.
    let ws = Workspace::load(&path).unwrap();
    let again = d.join("again.tenun");
    ws.save(&again).unwrap();
    assert_eq!(std::fs::read_to_string(&again).unwrap(), old);
    // And through the engine, which also checks the links on open.
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.open(&path).unwrap();
    let saved = d.join("saved.tenun");
    e.save(Some(saved.clone())).unwrap();
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), old);
    // The tile asks the same question it always did.
    let r = run(&e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
    assert_eq!(groups(&r), map(&[("West", 425.5), ("East", 50.0)]));
}

#[test]
fn a_new_name_and_a_via_are_written_and_read_back() {
    let lab = Lab::new("write").with_transfer_links();
    lab.e
        .rename_link("r1", Some("Shipped from".into()))
        .unwrap();
    let path = lab.d.join("w.tenun");
    lab.e.save(Some(path.clone())).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"name\": \"Shipped from\""), "{text}");
    // Only the named link carries a name.
    assert_eq!(text.matches("\"name\": \"Shipped from\"").count(), 1);
    let ws = Workspace::load(&path).unwrap();
    assert_eq!(ws.relationships[0].name.as_deref(), Some("Shipped from"));
    assert_eq!(ws.relationships[1].name, None);
    let second = serde_json::to_value(&ws.relationships[1]).unwrap();
    assert!(second.get("name").is_none(), "{second}");
}

#[test]
fn snapshots_and_exports_carry_the_note_of_each_tile() {
    let lab = Lab::new("snapshot").with_order_link();
    lab.save(
        "stores.csv",
        &format!("{STORES}1,Kopi A (copy),East\n"),
        "s2",
    );
    let by_name = json!([{ "table": "s2/stores", "column": "name" }]);
    let mut t3 = tile(sum_amount(), by_name);
    t3["id"] = json!("t3");
    let mut t2 = tile(sum_amount(), json!([]));
    t2["id"] = json!("t2");
    let dash = json!({ "id": "d1", "title": "Orders", "tiles": [
        tile(sum_amount(), by_region_of_store()), t2, t3,
    ]});
    lab.e.set_dashboards(vec![dash]);
    let snap = snapshot::build(&lab.e, "d1", &[]).unwrap();
    // What leaves the computer has no key value and no file name (V4-D52).
    let note = "The stores table repeats 1 value of id. Tenun used the first row of it. 2 of 5 orders rows meet a repeated key.";
    assert_eq!(snap["base"]["t1"]["notes"], json!([note]));
    assert!(snap["base"]["t1"].get("publicNotes").is_none());
    assert_eq!(snap["base"]["t3"]["notes"], json!([note]));
    // The tile that never meets the link has no note, and the field is not even written.
    assert!(snap["base"]["t2"].get("notes").is_none());
    // The offline cross-filter results carry it too: a click on West re-asks the other tiles.
    assert_eq!(snap["cross"]["t1"]["West"]["t3"]["notes"], json!([note]));
    // Even a tile with no group meets the link when the click filters it by a store's region.
    assert_eq!(snap["cross"]["t1"]["West"]["t2"]["notes"], json!([note]));
}

// ---- the server ---------------------------------------------------------------------------

async fn get(running: &Running, path: &str) -> Value {
    let url = format!("http://{}{path}", running.addr);
    tokio::task::spawn_blocking(move || {
        let mut res = ureq::get(url).header("x-tenun", "1").call().unwrap();
        serde_json::from_str(&res.body_mut().read_to_string().unwrap()).unwrap()
    })
    .await
    .unwrap()
}

async fn patch(running: &Running, path: &str, body: Value) -> (u16, Value) {
    let url = format!("http://{}{path}", running.addr);
    tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let mut res = agent
            .patch(url)
            .header("content-type", "application/json")
            .header("x-tenun", "1")
            .send(body.to_string())
            .unwrap();
        let status = res.status().as_u16();
        let text = res.body_mut().read_to_string().unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_save_with_a_repeated_key_shows_the_warning_within_seconds() {
    let d = dir("live");
    std::fs::write(d.join("orders.csv"), ORDERS).unwrap();
    std::fs::write(d.join("stores.csv"), STORES).unwrap();
    let running = tenun::start(Options {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        workspace: Some(d.join("live.tenun")),
        picker: None,
    })
    .await
    .unwrap();
    let e = &running.state.engine;
    for f in ["orders.csv", "stores.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    e.add_relationship(field("s1/orders", "store_id"), field("s2/stores", "id"))
        .unwrap();
    running.state.rewatch();
    let w = get(&running, "/api/workspace").await;
    assert!(w["health"]["r1"].get("note").is_none(), "{w}");

    let start = std::time::Instant::now();
    std::fs::write(
        d.join("stores.csv"),
        format!("{STORES}1,Kopi A (copy),East\n"),
    )
    .unwrap();
    let mut seen = None;
    while start.elapsed() < std::time::Duration::from_secs(10) {
        let w = get(&running, "/api/workspace").await;
        if let Some(n) = w["health"]["r1"]["note"].as_str() {
            seen = Some((n.to_string(), start.elapsed()));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let (note, took) = seen.expect("the warning shows after the file is saved");
    assert!(
        note.starts_with("stores.csv has 2 rows with id 1."),
        "{note}"
    );
    eprintln!("the warning showed {took:?} after the save");
    assert!(took < std::time::Duration::from_secs(3), "{took:?}");

    // The same note comes with the tile.
    let r = run(e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
    assert_eq!(r.notes.len(), 1);
    assert_eq!(groups(&r), map(&[("West", 425.5), ("East", 50.0)]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_names_a_link_and_refuses_a_bad_name_in_words() {
    let d = dir("route");
    std::fs::write(d.join("orders.csv"), ORDERS).unwrap();
    std::fs::write(d.join("stores.csv"), STORES).unwrap();
    std::fs::write(d.join("transfers.csv"), TRANSFERS).unwrap();
    let running = tenun::start(Options {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        workspace: None,
        picker: None,
    })
    .await
    .unwrap();
    let e = &running.state.engine;
    for f in ["orders.csv", "stores.csv", "transfers.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    for col in ["from_store", "to_store"] {
        e.add_relationship(field("s3/transfers", col), field("s2/stores", "id"))
            .unwrap();
    }
    let (status, v) = patch(
        &running,
        "/api/relationships/r1",
        json!({ "name": "Shipped from" }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(v["workspace"]["relationships"][0]["name"], "Shipped from");
    assert!(v["workspace"]["relationships"][1].get("name").is_none());
    let (status, v) = patch(
        &running,
        "/api/relationships/r2",
        json!({ "name": "shipped from" }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        v["error"],
        "Another link is called shipped from. Give this one another name."
    );
    // A body with no name is not a request to clear it.
    let (status, v) = patch(&running, "/api/relationships/r1", json!({})).await;
    assert_eq!(status, 400);
    assert_eq!(
        v["error"],
        "Send the name of the link as text. An empty name clears it."
    );
    let (status, _) = patch(&running, "/api/relationships/r1", json!({ "name": null })).await;
    assert_eq!(status, 400);
    let w = get(&running, "/api/workspace").await;
    assert_eq!(w["workspace"]["relationships"][0]["name"], "Shipped from");
    // Two links swap their names in one request.
    let url = format!("http://{}/api/relationships/names", running.addr);
    let status = tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let mut res = agent
            .put(url)
            .header("content-type", "application/json")
            .header("x-tenun", "1")
            .send(json!({ "names": { "r1": "Sold at", "r2": "Shipped from" } }).to_string())
            .unwrap();
        let _ = res.body_mut().read_to_string();
        res.status().as_u16()
    })
    .await
    .unwrap();
    assert_eq!(status, 200);
    let w = get(&running, "/api/workspace").await;
    assert_eq!(w["workspace"]["relationships"][0]["name"], "Sold at");
    assert_eq!(w["workspace"]["relationships"][1]["name"], "Shipped from");
    // The health of a clean link is there and says nothing is wrong.
    let w = get(&running, "/api/workspace").await;
    assert_eq!(w["health"]["r1"]["kind"], "one-many");
    assert_eq!(w["health"]["r1"]["duplicates"], 0);
}

// ---- the review of 01b --------------------------------------------------------------------

#[test]
fn nothing_is_counted_twice_while_a_file_is_being_reloaded() {
    // The window after a save: the new file is read, the link is not checked yet. A total in that
    // window must not be the doubled one (V4-D51). The check is held open for a second.
    let lab = std::sync::Arc::new(Lab::new("race").with_order_link());
    lab.e
        .slow_link_checks(std::time::Duration::from_millis(900));
    std::fs::write(
        lab.d.join("stores.csv"),
        format!("{STORES}1,Kopi A (copy),East\n"),
    )
    .unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader = {
        let (lab, stop) = (lab.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut seen = 0;
            let mut wrong = Vec::new();
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                let r = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
                seen += 1;
                if groups(&r) != map(&[("West", 425.5), ("East", 50.0)]) {
                    wrong.push(groups(&r));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            (seen, wrong)
        })
    };
    lab.e.load("s2");
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let (seen, wrong) = reader.join().unwrap();
    assert!(seen > 10, "the reader ran {seen} times");
    assert!(wrong.is_empty(), "a total was wrong: {wrong:?}");
    // The check has run and the warning is there.
    assert!(lab.note("r1").is_some());
}

#[test]
fn a_question_planned_before_a_reload_is_asked_again() {
    // A question planned on the clean link waits; meanwhile a save repeats a key and the reload swaps the data in.
    // Run as planned, it would join the repeated key twice. It sees that the data changed and asks again (V4-D51).
    let lab = std::sync::Arc::new(Lab::new("plan").with_order_link());
    lab.e.slow_plans(std::time::Duration::from_millis(400));
    let asker = {
        let lab = lab.clone();
        std::thread::spawn(move || {
            run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap()
        })
    };
    std::thread::sleep(std::time::Duration::from_millis(100));
    std::fs::write(
        lab.d.join("stores.csv"),
        format!("{STORES}1,Kopi A (copy),East\n"),
    )
    .unwrap();
    lab.e.load("s2");
    let r = asker.join().unwrap();
    assert_eq!(groups(&r), map(&[("West", 425.5), ("East", 50.0)]));
}

#[test]
fn a_check_that_cannot_read_the_data_leaves_the_link_deduplicated() {
    let lab = Lab::new("failed").with_order_link();
    // The stores file loses the column the link needs. The tables load, the check fails.
    lab.save("stores.csv", "name,region\nKopi A,West\n", "s2");
    assert!(!lab.e.snapshot().health.contains_key("r1"));
    // The query itself says the column is gone; nothing joins raw.
    let err = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap_err();
    assert!(err.to_string().contains("id"), "{err}");
    // The file comes back with a repeated id: nothing is counted twice.
    lab.save(
        "stores.csv",
        &format!("{STORES}1,Kopi A (copy),East\n"),
        "s2",
    );
    let r = run(&lab.e, tile(sum_amount(), by_region_of_store()), vec![]).unwrap();
    assert_eq!(groups(&r), map(&[("West", 425.5), ("East", 50.0)]));
}

/// Customers with a repeated e-mail, and sales that point at them by e-mail.
fn email_lab(tag: &str) -> Lab {
    let d = dir(tag);
    // The customers are unique when the link is made, as a link needs, and repeat an e-mail after.
    std::fs::write(
        d.join("customers.csv"),
        "email,tier\nana@example.com,gold\nbo@example.com,gold\n",
    )
    .unwrap();
    std::fs::write(
        d.join("sales.csv"),
        "id,email,amount\n1,ana@example.com,10\n2,bo@example.com,20\n3,ana@example.com,5\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in ["customers.csv", "sales.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    e.add_relationship(field("s2/sales", "email"), field("s1/customers", "email"))
        .unwrap();
    let lab = Lab { e, d };
    lab.save(
        "customers.csv",
        "email,tier\nana@example.com,gold\nana@example.com,silver\nbo@example.com,gold\n",
        "s1",
    );
    lab
}

#[test]
fn a_page_that_leaves_the_computer_carries_no_key_and_no_file_name() {
    let lab = email_lab("email");
    let by_tier = json!([{ "table": "s1/customers", "column": "tier" }]);
    let amount = json!({ "table": "s2/sales", "column": "amount", "agg": "sum" });
    // In the app the note names the e-mail and the file.
    let r = run(&lab.e, tile(amount.clone(), by_tier.clone()), vec![]).unwrap();
    assert_eq!(
        r.notes,
        [
            "customers.csv has 2 rows with email ana@example.com. Tenun used the first. 2 of 3 sales rows meet a repeated key. Fix the file to remove this note."
        ]
    );
    // Totals are not doubled: ana's first row is gold, so gold has 10 + 5 + 20.
    assert_eq!(groups(&r), map(&[("gold", 35.0)]));
    lab.e.set_dashboards(vec![json!({
        "id": "d1", "title": "Sales", "tiles": [tile(amount, by_tier)]
    })]);
    // The snapshot behind an exported page says it without them.
    let payload = snapshot::build(&lab.e, "d1", &[]).unwrap();
    for what in [
        "ana@example.com",
        "bo@example.com",
        "customers.csv",
        "sales.csv",
    ] {
        assert!(
            !payload.to_string().contains(what),
            "the snapshot holds {what}"
        );
    }
    assert_eq!(
        payload["base"]["t1"]["notes"][0],
        "The customers table repeats 1 value of email. Tenun used the first row of it. 2 of 3 sales rows meet a repeated key."
    );
    // The picture of a tile is made from the public notes too.
    assert!(!r.public_notes[0].contains("example.com"));
    let t: Tile = serde_json::from_value(tile(
        json!({ "table": "s2/sales", "column": "amount", "agg": "sum" }),
        json!([{ "table": "s1/customers", "column": "tier" }]),
    ))
    .unwrap();
    let out = snapshot::run_tile_public(&lab.e, &t, &[]).unwrap();
    assert_eq!(out.notes, r.public_notes);
    assert!(out.public_notes.is_empty());
}

#[test]
fn the_notes_of_a_tile_come_from_every_query_it_runs() {
    // The table of sales by shop crosses no link; its trend column reads the opening date of the
    // shop, and that crosses the link with the repeated shop id (V4-D45).
    let d = dir("spark");
    std::fs::write(d.join("sales.csv"), "shop_id,amount\n1,10\n2,20\n1,5\n").unwrap();
    std::fs::write(
        d.join("shops.csv"),
        "id,opened\n1,2026-01-05\n2,2026-02-10\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in ["sales.csv", "shops.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    e.add_relationship(field("s1/sales", "shop_id"), field("s2/shops", "id"))
        .unwrap();
    // Shop 1 is written twice after the link was made.
    std::fs::write(
        d.join("shops.csv"),
        "id,opened\n1,2026-01-05\n2,2026-02-10\n1,2026-03-01\n",
    )
    .unwrap();
    e.load("s2");
    let mut t = tile(
        json!({ "table": "s1/sales", "column": "amount", "agg": "sum" }),
        json!([{ "table": "s1/sales", "column": "shop_id" }]),
    );
    t["type"] = json!("table");
    // Without the trend column the tile crosses nothing and has no note.
    assert!(run(&e, t.clone(), vec![]).unwrap().notes.is_empty());
    t["spark"] = json!({ "table": "s2/shops", "column": "opened", "grain": "month" });
    let r = run(&e, t, vec![]).unwrap();
    assert!(r.sparks.is_some(), "the trend column was not drawn");
    assert_eq!(r.notes.len(), 1, "{:?}", r.notes);
    assert!(
        r.notes[0].starts_with("shops.csv has 2 rows with id 1."),
        "{:?}",
        r.notes
    );
}

#[test]
fn a_link_note_goes_to_the_link_when_its_kind_changes_and_goes_when_it_changes_back() {
    let lab = Lab::new("kind");
    std::fs::write(
        lab.d.join("cards.csv"),
        "store_id,card\n1,visa\n2,amex\n3,visa2\n",
    )
    .unwrap();
    lab.e
        .add_source(AddSource::File {
            path: lab.d.join("cards.csv").to_string_lossy().into_owned(),
        })
        .unwrap();
    lab.e
        .add_relationship(field("s4/cards", "store_id"), field("s2/stores", "id"))
        .unwrap();
    assert!(lab.note("r1").is_none());
    lab.save(
        "cards.csv",
        "store_id,card\n1,visa\n2,amex\n2,x\n3,v\n",
        "s4",
    );
    assert!(
        lab.note("r1")
            .unwrap()
            .starts_with("This link was one to one and is now one to many.")
    );
    // A reload that changes nothing keeps the note: the link is still not what it was.
    lab.save(
        "cards.csv",
        "store_id,card\n1,visa\n2,amex\n2,x\n3,v\n4,w\n",
        "s4",
    );
    assert!(lab.note("r1").is_some());
    lab.save("cards.csv", "store_id,card\n1,visa\n2,amex\n3,v\n", "s4");
    assert!(lab.note("r1").is_none());
}

#[test]
fn names_can_swap_and_a_table_name_clash_is_reported() {
    let lab = Lab::new("swap").with_transfer_links();
    let e = &lab.e;
    e.rename_link("r1", Some("Shipped from".into())).unwrap();
    e.rename_link("r2", Some("Sold at".into())).unwrap();
    // One at a time the swap cannot work; as a set it can.
    assert!(e.rename_link("r1", Some("Sold at".into())).is_err());
    e.rename_links(BTreeMap::from([
        ("r1".to_string(), Some("Sold at".to_string())),
        ("r2".to_string(), Some("Shipped from".to_string())),
    ]))
    .unwrap();
    let names = |lab: &Lab| -> Vec<Option<String>> {
        lab.e
            .snapshot()
            .workspace
            .relationships
            .iter()
            .map(|r| r.name.clone())
            .collect()
    };
    assert_eq!(
        names(&lab),
        [Some("Sold at".into()), Some("Shipped from".into())]
    );
    // Two links cannot end with one name, and a table id is a table's name too.
    assert!(
        e.rename_links(BTreeMap::from([
            ("r1".to_string(), Some("same".to_string())),
            ("r2".to_string(), Some("Same".to_string())),
        ]))
        .is_err()
    );
    let err = e.rename_link("r1", Some("s2/stores".into())).unwrap_err();
    assert!(
        err.to_string().starts_with("A table is called s2/stores."),
        "{err}"
    );
    assert_eq!(names(&lab)[0].as_deref(), Some("Sold at"));
    // A table that arrives later with a link's name is a clash the Model view can show.
    assert!(e.snapshot().link_clashes.is_empty());
    std::fs::write(lab.d.join("Sold at.csv"), "a,b\n1,2\n").unwrap();
    e.add_source(AddSource::File {
        path: lab.d.join("Sold at.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let clashes = e.snapshot().link_clashes;
    assert_eq!(clashes.len(), 1);
    assert!(
        clashes["r1"].starts_with("A table is called Sold at too"),
        "{clashes:?}"
    );
}

async fn post_bytes(running: &Running, path: &str, body: Value) -> (u16, Vec<u8>) {
    let url = format!("http://{}{path}", running.addr);
    tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let mut res = agent
            .post(url)
            .header("content-type", "application/json")
            .header("x-tenun", "1")
            .send(body.to_string())
            .unwrap();
        let status = res.status().as_u16();
        (status, res.body_mut().read_to_vec().unwrap())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_export_route_writes_a_page_with_no_key_and_no_file_name() {
    let d = dir("export");
    std::fs::write(
        d.join("customers.csv"),
        "email,tier\nana@example.com,gold\nbo@example.com,gold\n",
    )
    .unwrap();
    std::fs::write(
        d.join("sales.csv"),
        "id,email,amount\n1,ana@example.com,10\n2,bo@example.com,20\n3,ana@example.com,5\n",
    )
    .unwrap();
    let running = tenun::start(Options {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        workspace: None,
        picker: None,
    })
    .await
    .unwrap();
    let e = &running.state.engine;
    for f in ["customers.csv", "sales.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    e.add_relationship(field("s2/sales", "email"), field("s1/customers", "email"))
        .unwrap();
    std::fs::write(
        d.join("customers.csv"),
        "email,tier\nana@example.com,gold\nana@example.com,silver\nbo@example.com,gold\n",
    )
    .unwrap();
    e.load("s1");
    e.set_dashboards(vec![json!({
        "id": "d1", "title": "Sales", "tiles": [tile(
            json!({ "table": "s2/sales", "column": "amount", "agg": "sum" }),
            json!([{ "table": "s1/customers", "column": "tier" }]),
        )]
    })]);
    let (status, html) =
        post_bytes(&running, "/api/export-html", json!({ "dashboard": "d1" })).await;
    assert_eq!(status, 200);
    let html = String::from_utf8(html).unwrap();
    for what in [
        "ana@example.com",
        "bo@example.com",
        "customers.csv",
        "sales.csv",
    ] {
        assert!(!html.contains(what), "the page holds {what}");
    }
    assert!(html.contains("The customers table repeats 1 value of email."));
    // The Excel file of the tile has the same sentence.
    let (status, xlsx) = post_bytes(
        &running,
        "/api/export-xlsx",
        json!({ "tile": tile(
            json!({ "table": "s2/sales", "column": "amount", "agg": "sum" }),
            json!([{ "table": "s1/customers", "column": "tier" }]),
        ), "filters": [], "headers": [], "title": "Sales" }),
    )
    .await;
    assert_eq!(status, 200);
    use calamine::{DataType, Reader};
    let mut wb: calamine::Xlsx<_> =
        calamine::open_workbook_from_rs(std::io::Cursor::new(xlsx)).unwrap();
    let name = wb.sheet_names()[0].clone();
    let text: Vec<String> = wb
        .worksheet_range(&name)
        .unwrap()
        .rows()
        .flat_map(|r| r.iter().filter_map(|c| c.as_string()))
        .collect();
    assert!(
        text.iter()
            .any(|t| t.starts_with("The customers table repeats 1 value of email.")),
        "{text:?}"
    );
    assert!(
        !text
            .iter()
            .any(|t| t.contains("example.com") || t.contains(".csv")),
        "{text:?}"
    );
}

#[test]
fn a_large_whole_number_key_is_written_in_full() {
    let d = dir("bigkey");
    let big = "1234567890123456789";
    std::fs::write(d.join("owners.csv"), format!("id,name\n{big},a\n2,b\n")).unwrap();
    std::fs::write(
        d.join("cars.csv"),
        format!("owner,plate\n{big},x\n2,y\n{big},z\n"),
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in ["owners.csv", "cars.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    e.add_relationship(field("s2/cars", "owner"), field("s1/owners", "id"))
        .unwrap();
    std::fs::write(
        d.join("owners.csv"),
        format!("id,name\n{big},a\n2,b\n{big},c\n"),
    )
    .unwrap();
    e.load("s1");
    let h = e.snapshot().health["r1"].clone();
    // Not 1.2345678901234568e+18.
    assert_eq!(h.sample, [json!(big)]);
    assert!(h.note.unwrap().contains(&format!("with id {big}")));
}

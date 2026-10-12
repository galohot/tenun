# Relationships

A relationship tells Tenun that a column in one table points at a column in another table. A column of
orders that holds a store id points at the column of stores that holds the store id. With the link,
a tile can group orders by the city of the store.

Tenun never changes your files. A relationship lives in the `.tenun` file and nowhere else.

## How Tenun picks the one side

Every link has a **many side** and a **one side**. Many rows of orders point at one row of stores,
so orders are the many side and stores are the one side.

You do not choose. You draw a line from a column to a column, and Tenun reads the data:

- The side whose values are all different (an ID) is the one side.
- If both sides are all different, the first column you dragged from is the many side. The link is
  **one to one** and works in both directions (see below).
- If neither side is all different, Tenun refuses the link. Such a join would count every total
  twice. The message names both columns.

The line in the Model view shows what the data says, next to the dot of the one side:

| Label | Meaning |
|---|---|
| **1 – many** | The usual link. One store, many orders. The one side may have a few repeated keys (see below). |
| **1 – 1** | Both sides are all different. |
| **many – 1** | The one side is not a key any more, and the many side is all different. Tenun reads the link the other way round. |
| **many – many** | Neither direction works. The link is saved and not used. |

## What happens when a key stops being unique

Files change while the app is open. Someone adds a row to `stores.csv` with a store id that is
already there. A plain join would then count every order of that store twice, and the totals would
be too big.

**When Tenun checks.** A link between files, sheets, pasted tables and web links is checked each
time one of its tables loads: when you open the workspace, when a file is saved, when you press
reload, and when a web link is fetched again. The check is quick and finishes before the load does.
It does not run again for each tile. The answer is kept until the table loads again.

A link that touches a **database** table (PostgreSQL, MySQL, or a DuckDB file that Tenun reads where
it is) is checked in the background, so opening the workspace or pressing reload never waits for the
database. The check reads at most the first 1,000,000 rows of each side, and the note says so when it
stopped there. It runs again when you open or reload, each time a dashboard refreshes (the
interval, or **Refresh now**), and before a page is exported or published, because a database
changes without a load.

While a table is loading, and while a check has not finished or could not read the data, Tenun does
not know the link. It reads one row for each key then, so a total is never counted twice in that
time. A join into a database table always reads one row for each key, whatever the last check said.

When the one side repeats a key:

1. Tenun reads **one row for each key**. For a file, a sheet, a pasted table or a web link, this is
   the first row in the file. For a database table, which has no order a person can see, it is one
   of the rows of the key, and which one can change.
2. **The totals do not change.** An order is counted once.
3. The line in the Model view shows a warning mark, and the key column shows one too. Open the line
   to read the sentence and the repeated keys.
4. Every tile that crosses the link shows the same sentence under it, from every question the tile
   asks, its trend column too. The sentence says which keys repeat, and how many rows of the
   many side meet one of them: `stores.csv has 2 rows with id 7. Tenun used the first. 12 of 3,000
   orders rows meet a repeated key. Fix the file to remove this note.`
5. A picture, an exported or published page and an Excel file carry the sentence **without the key
   values and without file names**, because they leave your computer: `The stores table repeats 1
   value of id. Tenun used the first row of it. 12 of 3,000 orders rows meet a repeated key.`

When you remove the repeated row and save, the warning goes away within a second.

**When the one side is not a key.** A few repeated keys are a warning. A column in which more than
half of the distinct keys repeat is another kind of column, and one row for each key would throw
most of it away. For example, four of the seven distinct ids repeat. Then:

- If the many side is all different, Tenun reads the link the other way round (**many – 1**).
- If the many side repeats too, the link is **many – many**. Tiles that need it say why, and the
  note names the repeated keys. The link stays in the file and works again when the file is fixed.

One id written ten times next to five real ids is one repeated key out of six. That is a warning
with the first row used, not a link that stops.

## One to one

When both sides are all different, a tile can go along the link from either table. A table of
orders can show the card of each store, and a table of stores can show the number of cards. The line
stays as it was drawn: you do not need to change the file.

A link works from the one side as long as the many side (the column you dragged from) is all
different. If a later reload makes **that** column repeat a value, the link goes back to one
direction, the line changes its label to **1 – many**, and the link says what it was and what it is:
"This link was one to one and is now one to many." A tile that needs the other direction says which
table repeats which column, and does not say that the tables are not related.

If the **one** side gains a repeated key instead, the link still works in both directions. It
shows a warning, and the way into the one side reads one row for each key.

## Two links to one table

A table of transfers has two columns that hold a store id: the store that sent, and the store that
received. Both point at the stores table. These are two links between the same two tables, and each
one needs a name.

1. Drag the second line. Tenun opens the sheet **Name these links** and proposes a name from the
   column: `Stores (from_store)` and `Stores (to_store)`.
2. Type better names: **Shipped from** and **Sold at**. A name is new: it cannot be the label or the
   id of a table, or the name of another link. Both names are saved together, so two links can swap
   their names. If a table arrives later with the name of a link, a formula would read the table, so
   the Model view says so on that link.
3. To rename a link later, click its label in the Model view.

A tile picks the link when you pick the field. The list of fields shows the stores once for each
link, headed by the name of the link:

```
Transfers
Shipped from   (Stores)    id, name, kota …
Sold at        (Stores)    id, name, kota …
```

A tile with **Quantity** grouped by **Shipped from › kota** adds up the quantity by the city of the
store that sent it. Grouped by **Sold at › kota**, it adds up the quantity by the city of the store
that received it. The two are different numbers. Both can be in one tile: a pivot with the sending
city in the rows and the receiving city in the columns.

A filter tile measures nothing, but its field list also shows the stores once for each link, so a filter can read either one.

A click on a bar filters by the same link as the field of the bar. A click on a **Sold at** bar
does not filter the **Shipped from** numbers.

In a formula, write the name of the link instead of the table:
`[Shipped from].[kota]` and `[Sold at].[kota]`. `[Stores].[kota]` is an error when two links lead to
Stores, and the message names both links. See [formulas](formulas.md).

An older workspace can have two links between the same tables with no names. Its tiles keep their
numbers: a field with no link uses the first link. The Model view marks the pair **Name these
links**.

The same happens one step further away. If Regions can be reached through Shipped from or through
Sold at, with the same number of steps, the Model view lists it under **More than one way**, and the
list of fields offers Regions once for each way.

## Many to many with a link table

Some data is many to many. A programme has several funders, and a funder pays for several programmes.
A third table says which: `programme_funders`, with a `programme_id`, a `funder_id` and maybe a `share`.
This table is a **link table**. It holds nothing but links.

Draw its two lines first: `programme_id` to the programmes, `funder_id` to the funders. Both are the
usual one-to-many links. Then **mark the table as a link table**:

1. Open **Model**. A table with two links to two different tables, and little else, shows **Looks like a
   link table** under its name. Click **Mark it**.
2. Any other table with two or more links from it shows **Mark as link table** when you point at it.
3. A marked table shows **Link table** under its name. Click the cross beside it to unmark it. The mark
   is saved in the `.tenun` file as `linkTable`. A file made before this has none.

Marked, the table can be crossed once, from the one side to the many side. A tile of **spending** can
now group by **funders › name**, and a tile of **programmes** can group by the funders too. A table that
is not marked is never crossed. Tenun does not guess.

### What the numbers mean

Take four programmes with spending of 35 (A), 30 (B), 90 (C) and 60 (D). A has the funders Ford and
Gates, B has Ford and Wellcome, C has Gates and EU, and D has none. Spending by funder:

| Funder | Spending |
|---|---|
| Ford | 65 (A and B) |
| Gates | 125 (A and C) |
| EU | 90 (C) |
| Wellcome | 30 (B) |
| (blank) | 60 (D) |

- **A row is under each of its funders.** The spending of A is in the bar of Ford and in the bar of
  Gates. The spending of D has no funder, so it is in the blank group, as it always is.
- **The bars add up to more than the total.** They add up to 370. The spending is 215. The total
  counts each row once, because the total has no group and so does not cross the link table. The
  number tile, the total row of a table, the totals of a pivot and the subtotals all come from a
  question without the link table.
- **The tile says so.** Under the chart: *A row of spending with two funders is under both. The total
  counts it once.* The words use the names of the tables only, so they go with a picture, an exported
  page and an Excel file as they are.
- **A pair written twice counts once.** If `programme_funders` lists C and EU two times, EU still
  has 90. Tenun reads the link table as distinct pairs.
- **A row counts once in a group, however many of its funders are in it.** Group by the kind of
  funder: A has two private funders, and its spending is in **Private** once.
- **The one-row-per-key rule holds on both one sides.** A programme or a funder listed twice is read
  as one row, with the warning of the link, as for any link. This holds for a database table, and for
  a link whose health Tenun does not know yet.

### Filters

A filter on a field behind a link table keeps **a row when at least one of its funders passes the
filter**. It keeps the row once.

- A click on the Gates bar filters a monthly line to the spending of the programmes Gates funds:
  A and C. A programme with Ford and Gates is not counted twice.
- A filter on **Ford or Gates** keeps A once, not twice.
- **Not Ford and not Wellcome** keeps a row that has a funder that is neither. B has only those two,
  so B goes.
- Two filters on the funders mean one funder that passes both: *Gates* and *Public* keeps nothing,
  because no funder is both.
- When the chart itself groups by the funder, a filter on the funder works on the pairs the chart
  shows. **Keep only Ford** leaves one bar. **Exclude Ford** removes the bar and leaves the other bars
  as they were.
- A chart with **Top N** and **Other** counts **Other** as the rows that have at least one funder
  outside the top N, each row once. The Other bar and the top bars can share a row.

### What Tenun refuses, and why

Everything that needs the groups to add up to the whole is off for a group behind a link table:

- A **share strip**, a **donut** and a **pie**, **100 % stacked bars**, **stacked bars**, and the
  value forms **% of total** and **% of group**. The words: *These shares would add up to more than
  100 %, because one row can be in two groups.*
- A **running total** and a **year to date** along the groups, a **waterfall by category**, and the
  **Other bar that the app makes itself** when a chart has many bars.
- A **Sankey** chart with a stage behind a link table. It follows each row from one stage to the next.

A change, a rank and a moving average do not need the groups to add up, so they work.

A **value** cannot come from behind a link table. A chart of spending can group by funders, but it
cannot add up a column of funders, because a funder would count once for each row of spending. The
tile says so. Use the funders as a group or a filter, or start the tile from the funders.

A path crosses **one** link table. If a second link table hangs off the funders, the tables behind it
are not reachable from spending. Start the tile from the funders to cross it.

A link table whose two links lead to the **same** table (friends of friends, stock sent from a store to
a store) does not give a second copy of that table. Use two named links for that, as in
[Two links to one table](#two-links-to-one-table).

A formula (a calculated column, a measure) never crosses a link table. It goes from the many side to
the one side only. A named measure works across a link table because each of its tables is totalled on
its own and the parts meet on the group, as they do for any group: `SUM([Spending].[amount]) /
SUM([Programmes].[budget])` by funder is the spending of the programmes of each funder over their
budgets.

## What Tenun refuses

- **A table linked to itself.**
- **Many to many in one link.** If neither column is all different, Tenun does not make the link.
  Use a link table (see above).

## If you know Laravel or Power BI

| Laravel (Eloquent) | Power BI | Tenun |
|---|---|---|
| `hasMany`, `belongsTo` | Many-to-one relationship | A line from the many column to the one column. Tenun reads which side is which. |
| `hasOne` | One-to-one relationship | A line where both sides are all different. It works in both directions. |
| `belongsToMany` | Many-to-many with a bridge table | Mark the bridge table as a link table. A group behind it puts a row under each of its values, and the total counts the row once. |
| Two relations to one model (`shipStore()` and `saleStore()`) | Active and inactive relationships | Two named links. Each tile says which one it uses. There is no "inactive" link. |
| Cardinality in the migration | Cardinality menu | No menu. The data decides, and Tenun checks again each time a table loads. |
| Duplicate key breaks the join | Duplicate key breaks the link | The first row is used, and a warning shows on the link and on the tiles. |

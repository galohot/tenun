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

## What Tenun refuses

- **A table linked to itself.**
- **Many to many.** If neither column is all different, Tenun does not make the link. A link table
  for many-to-many is planned for a later version.

## If you know Laravel or Power BI

| Laravel (Eloquent) | Power BI | Tenun |
|---|---|---|
| `hasMany`, `belongsTo` | Many-to-one relationship | A line from the many column to the one column. Tenun reads which side is which. |
| `hasOne` | One-to-one relationship | A line where both sides are all different. It works in both directions. |
| `belongsToMany` | Many-to-many with a bridge table | Not yet. Tenun refuses a link when neither side is unique. |
| Two relations to one model (`shipStore()` and `saleStore()`) | Active and inactive relationships | Two named links. Each tile says which one it uses. There is no "inactive" link. |
| Cardinality in the migration | Cardinality menu | No menu. The data decides, and Tenun checks again each time a table loads. |
| Duplicate key breaks the join | Duplicate key breaks the link | The first row is used, and a warning shows on the link and on the tiles. |

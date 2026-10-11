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
| **1 – many** | The usual link. One store, many orders. |
| **1 – 1** | Both sides are all different. |
| **many – 1** | The stored direction stopped working, so Tenun reads the link the other way round. |
| **many – many** | Neither direction works. The link is saved and not used. |

## What happens when a key stops being unique

Files change while the app is open. Someone adds a row to `stores.csv` with a store id that is
already there. A plain join would then count every order of that store twice, and the totals would
be too big.

Tenun checks every link each time a table loads: when you open the workspace, when a file is saved,
when you press reload, and when a database refreshes. It does not check again for each tile. The
answer is kept until the table loads again.

When the one side repeats a key:

1. Tenun reads **one row for each key**. For a file, a sheet, a pasted table or a web link, this is
   the first row in the file. For a database table, which has no order a person can see, it is the
   row that sorts first when its columns are read from left to right. It is the same row every time.
2. **The totals do not change.** An order is counted once.
3. The line in the Model view shows a warning mark, and the key column shows one too. Open the line
   to read the sentence and the repeated keys.
4. Every tile that crosses the link shows the same sentence under it. The sentence also goes into
   exported files, published pages, PNG pictures and Excel files of the tile.

Example: `stores.csv has 2 rows with id 7. Tenun used the first. Fix the file to remove this note.`

When you remove the repeated row and save, the warning goes away within a second.

If the one side repeats most of its values (half of its rows or more), it is not a list of unique
things any more, and Tenun does not use one row for each key:

- If the many side is all different, Tenun reads the link the other way round (**many – 1**).
- If the many side repeats too, the link is **many – many**. Tiles that need it say why. The link
  stays in the file and works again when the file is fixed.

## One to one

When both sides are all different, a tile can go along the link from either table. A table of
orders can show the card of each store, and a table of stores can show the number of cards. The line
stays as it was drawn: you do not need to change the file.

If a later reload makes a column repeat a value, the link goes back to one direction. The line in
the Model view changes its label, and a tile that needs the other direction says it cannot reach
the table.

## Two links to one table

A table of transfers has two columns that hold a store id: the store that sent, and the store that
received. Both point at the stores table. These are two links between the same two tables, and each
one needs a name.

1. Drag the second line. Tenun opens the sheet **Name these links** and proposes a name from the
   column: `Stores (from_store)` and `Stores (to_store)`.
2. Type better names: **Shipped from** and **Sold at**. A name is new: it cannot be the name of a
   table or of another link.
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

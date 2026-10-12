# Data prep

Real spreadsheets are messy. Tenun cleans the common problems for you and shows what it did, so
that you can undo it. Your original files are never changed.

## Excel sheets with notes and headers

Tenun looks for the header row. It is the first row with text in at least half of the columns,
followed by a row of data.

- **Title rows above the header** are skipped. A note says, for example, "Skipped 3 title rows".
- **Footer rows** such as "Total" or "Jumlah" are dropped when they follow a blank row.
- **Two header rows** (a year over the months) join into one name, such as "2025 Jan".
- **A blank header** takes the name of the column on its left, plus a number.
- **Grouped rows:** when a column shows a value once and leaves the rows below blank, Tenun fills it down.
- **Header row override:** if the guess is wrong, open the **Data** view and type the right row number.

The notes show under the table name in the **Data** view.

## Rename, retype, hide and describe columns

Click a column heading in the **Data** view. The menu has:

- **Rename:** the new name shows everywhere in the app. The file keeps the old name.
- **Type:** read the column as text, number or date. For dates you choose the format: automatic,
  dd/mm/yyyy or mm/dd/yyyy. A value that cannot be read becomes blank, and Tenun tells you how many.
- **Hide:** the column leaves the field pickers. Relationships can still use it.
- **Description:** shows as a tooltip in the field pickers.

- **Edit formula** and **Delete column** (Advanced, calculated columns only): see
  [Add a column](formulas.md#add-a-column).

Hover a heading to see a **column profile**: the share of blanks, the number of different values,
the smallest and biggest value, and the top values or a small histogram.

## A column that is calculated

With Advanced features on, **Add column** at the top of a table adds a column made from a formula,
such as `[amount] * 1.11` or `[stores].[region]`. It works everywhere a column works, and your file
is not changed. It is written in [formulas](formulas.md#add-a-column).

## Months as columns (unpivot)

A sheet with one column for each month is easy to read but hard to chart. When a table has three
or more number columns that look like months, years or quarters, the **Data** view offers
**Turn these columns into rows**.

1. Review the preview. It shows five rows before and after.
2. Name the two new columns. Tenun keeps every column that is not a month, for example `product` and `region`.
3. Confirm. You get one row for each product, region and month, with a name column and a value column.

If every heading reads as a period, the new name column becomes a date. You can undo the change
from the same menu.

## A folder of files

Choose **Add data**, then **A folder of files**. Tenun stacks all CSV and Excel files that have
the same shape into one table. It adds a first column named `file`, so you can tell the files
apart. The default pattern is `*.csv;*.xlsx`.

- Columns are matched by name. A column that a file lacks is blank for its rows.
- A file that shares no column with the others is skipped. The source lists it.
- Add, change or remove a file in the folder and the table updates.

## Paste a table

Copy cells from Excel or Google Sheets, open the **Data** view and press Ctrl+V (Cmd+V on a Mac).
Tenun shows a name and a preview of five rows. Tick **First row is the header** when it is.

Tenun saves the data as a CSV file in a `<workspace name> data` folder next to your `.tenun` file,
and adds it as a normal source. You can edit that file later. Save the workspace first if it has
no file yet.

## A web link (CSV or JSON)

Choose **Add data**, then **Link**. The link can give CSV or JSON data. Tenun finds out which. It
works with:

- a Google Sheet that you publish to the web as CSV (in Google Sheets: **File**, **Share**,
  **Publish to web**, then pick CSV);
- a CSV file on the web;
- an API that answers with JSON, or with one JSON object on each line (NDJSON).

For every link:

- Only `https` links work. A link to an address on your own computer may use `http`. Tenun sends
  no cookies.
- The data can be up to 50 MB. For a link with pages, this is the total of all pages.
- Choose how often to fetch it: every 15 minutes, every hour (the default), every day, or by hand.
- The source shows the time of the last fetch and any error. If a fetch fails, Tenun keeps the
  last copy and shows the error beside the source.
- Tenun keeps the copy as a CSV file in the `<workspace name> data` folder next to your `.tenun`
  file.

Press **Preview** to see the first five rows before you add the link. For JSON, the preview also
shows **Records** and **More pages**. If you skip the preview, Tenun picks the records itself and
reads one page.

### JSON: records

A JSON answer holds a list of records somewhere in it. Tenun lists every list of objects it finds,
down to four levels, for example `data.items - 120 rows`. Choose one under **Records**. A path has
dots between keys and a number for a position in a list: `data.items`, `1`, `result.0.rows`.

If you do not choose, Tenun uses the answer itself when it is a list. Otherwise it takes the
longest list. A small list such as `errors` or `links` often comes first, so the first list is not
the best choice. An answer like `[meta, [records]]` (the World Bank shape) works as well.

Each record becomes one row:

- A nested object becomes columns named with dots: `user.name`, `user.address.city`. Tenun goes
  down five levels. Deeper parts become JSON text in one cell.
- A list inside a record becomes JSON text in one cell.
- `null` is an empty cell. A true or false value reads `true` or `false`.
- Columns keep the order in which they first appear. A table has up to 1,000 columns. The notes of
  the table say how many were left out.
- A list of plain values (not objects) becomes one column named `value`.

### JSON: more pages

Many APIs send the data in pages. Under **More pages**, choose how to find the next page:

- **Follow the Link header:** the `Link` header with `rel="next"` (GitHub and many others).
- **A field with the next link:** a field of the answer that holds the next link, for example
  `paging.next`. The link may be complete or relative to the current link.
- **A cursor field:** a field that holds a token, for example `next_cursor`. Tenun sends the token
  back as a link parameter, for example `cursor`.
- **A page number:** a link parameter, for example `page`. It starts at the number in your link,
  or at 1, and goes up by 1. If the answer says how many pages there are, Tenun stops there.

The preview picks the likely choice. Tenun stops when there is no next page, when a page has no
records, when a next link repeats, at 200 pages, or at 50 MB in all. At 200 pages or 50 MB, Tenun
keeps what it has read and adds a note to the table, for example "Read the first 200 pages
(24,000 rows). The link has more." The table still loads.

### A key

Some APIs need a key. Open **Key (optional)** and choose how to send it:

- **Header:** for example the name `Authorization` and the value `Bearer abc123`, or `X-Api-Key`.
- **Link parameter:** for example the name `api_key`. Tenun adds it to the link when it fetches.

If you paste a link that has a key in it (`?api_key=...`), Tenun moves the key into this section.

Tenun keeps the key in its own settings file on this computer, in your user settings folder. The
file can only be read by you. The key is **never** written to the workspace file, to the copy of the
data, or to an exported dashboard. Someone who opens your workspace file adds their own key. A key
goes only to the same host and port as your link. A page link or a redirect to another host gets no key. If an API
repeats the key inside its data (for example in a link in each record), Tenun shows it as `•••` in
the copy and in the preview.

To change or remove the key later, open **Link settings** under the source in the Data view. The
value field stays empty. Leave it empty to keep the saved key. A key that expires must be pasted
again.

### What is not supported

POST requests, sign-in through a browser (OAuth), XML and GraphQL.

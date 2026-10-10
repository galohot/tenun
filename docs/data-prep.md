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

Hover a heading to see a **column profile**: the share of blanks, the number of different values,
the smallest and biggest value, and the top values or a small histogram.

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

## A CSV link

Choose **Add data**, then **A CSV link**. This works with a Google Sheet that you publish to the
web as CSV (in Google Sheets: **File**, **Share**, **Publish to web**, then pick CSV).

- Only `https` links work. Tenun sends no cookies.
- The file can be up to 50 MB.
- Choose how often to fetch it: every 15 minutes, every hour (the default), every day, or by hand.
- The source shows the time of the last fetch and any error.

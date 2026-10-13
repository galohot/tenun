# Formulas

A formula is a small calculation written the way Excel writes it. Tenun reads it, checks it, and
asks the database to do the work. Your files are never changed.

Formulas arrive with calculated columns and measures in Tenun 0.4. This page describes the language,
and how to [add a column](#add-a-column) with one.

## Writing a formula

- **A column** is written in square brackets: `[amount]`.
- **A column of another table** has the table first: `[Stores].[rent]`.
- **A column through a named link** has the name of the link first: `[Shipped from].[city]`. Use it
  when two links lead to the same table. `[Stores].[city]` is then an error that names both links.
  See [relationships](relationships.md).
- **A bracket inside a name** is written twice: the column `a]b` is `[a]]b]`.
- **Numbers** are written as `12`, `1.5`, `.5` or `1e3`.
- **Text** goes in double quotes: `"Java"`. A quote inside the text is written twice: `"say ""hi"""`.
- **True and false** are `TRUE` and `FALSE`.
- **A date** is built with `DATE(2026, 1, 31)`.
- **Operators:** `+ - * / ^ & = <> < > <= >=`. The `&` joins text.
- **Brackets** `( )` change the order. Powers go before products, and products before sums.
- **A comma** separates the parts of a function. A semicolon is an error.
- **A leading `=`** is ignored, as in Excel: `=[amount] * 2` is the same as `[amount] * 2`.

Names do not care about case: `[Amount]`, `[amount]` and `sum` all work. A column is found by the
name you see in the app, or by its real name in the file.

When a name is wrong, Tenun says so and suggests the closest one: `No column [amout]. Did you mean [amount]?`
The editor underlines the part that is wrong.

## Three places for a formula

### Calculated column

A calculated column works on one row at a time. It adds a new column to a table.

Example: `IF([amount] > 100, "big", "small")`

- You can read a column of a related table. `[Stores].[rent]` gives the rent of the store of this
  order. The relationship must lead from this table to that one: many rows to one row, in one step
  or more.
- Reading the other way is an error. One store has many orders, so `[Orders].[amount]` has no single
  value on a store. Total it in a measure instead, with `SUMIF` or `COUNTROWS`.
- Totals such as `SUM` are not allowed here. They belong in a measure.

### Measure

A measure is a formula of totals. It is calculated for each group on a chart: each month, each region.

Every column must sit inside a total. `[amount] * 2` is an error. `SUM([amount]) * 2` is right.

Measure example: `SUM([Orders].[amount]) / SUM([Targets].[target])`

- The two totals come from two tables. Tenun adds up each table on its own and then divides. An
  order is never counted once for each target.
- If you name a table for the measure, `[amount]` means a column of that table.
- Totals can sit in other functions: `ROUND(SUM([Orders].[amount]) / COUNTROWS([Orders]), 2)`.

### Group value

A group value is a total inside a new table that groups another table. It works like a measure,
but it reads one table: the table that is grouped, and the tables related to it.

## Rules to know

- **A failure gives a blank, not an error.** Dividing by zero, `VALUE("abc")` and `SQRT(-1)` all give
  a blank. One odd row never stops a chart.
- **A blank in a calculation makes the result blank.** `[amount] + BLANK()` is blank. Use `IFERROR`
  or `IF(ISBLANK(...))` to give it a value.
- **A total of nothing is 0 for sums and counts, and blank for the rest.** `SUM`, `SUMIF`, `COUNT`,
  `COUNTA`, `COUNTD`, `COUNTIF` and `COUNTROWS` give 0 when there are no rows. `AVERAGE`, `MEDIAN`,
  `MIN`, `MAX`, `STDEV`, `VAR` and `PERCENTILE` give a blank. So `SUM([Sales].[amount]) -
  SUM([Refunds].[amount])` still works for a store with no refunds.
- **Empty text is blank.** `ISBLANK`, `IFERROR`, `COUNTA`, `COUNTD` and `[note] = ""` treat empty
  text the same as a cell with nothing in it. `LEN` of a blank cell is 0.
- **`=` and `<>` never give a blank.** A blank cell equals only another blank, and `[note] = ""` is
  true for a blank cell. The other comparisons (`<`, `>`) give a blank when one side is blank.
- **`IF` treats a blank test as false.**
- **The two results of `IF` must be the same kind.** `IF([amount] > 0, [amount], "n/a")` is an
  error, because one result is a number and the other is text. Turn the number into text:
  `IF([amount] > 0, TEXT([amount], "0"), "n/a")`.
- **Text ignores case** when it is compared: `"Ana" = "ana"` is true. `FIND` and `SUBSTITUTE` do care.
- **`&` writes a number without a trailing zero:** `"n=" & 1` is `n=1`. A blank adds nothing.
- **Dates move by days.** `[date] + 30` is 30 days later. `[date2] - [date1]` is the number of days.
- **`TODAY()` and `NOW()` are the date and time when the workspace was loaded or last refreshed,**
  not the moment you look at a chart. A dashboard that stays open overnight shows yesterday until
  its data refreshes. In a calculated column they are fixed when the column is built. It is built
  again when the day changes: when a dashboard refreshes, before a page is published or exported,
  and on each run of the agent. The column says "as of" and the day.
- **A minus sign binds tighter than a power, as in Excel.** `-2^2` is 4. Write `-(2^2)` for -4.
- **Only the four formats of `TEXT` exist,** and they always use a comma for thousands and a dot
  for decimals.
- **A formula has limits.** At most 10,000 characters and 100 levels of nesting. A chain of
  more than 100 `+` signs counts as nesting too. Past a limit you see "too long" or "nested too deeply".

## Functions

### Logic

- `IF(test, a, b)`: `a` when the test is true, else `b`. With two values, a false test gives a blank.
  Example: `IF([amount] > 100, "big", "small")`
- `IFS(test1, a1, test2, a2, ...)`: the result of the first true test. No true test gives a blank.
  Example: `IFS([amount] > 200, "high", [amount] > 50, "mid", TRUE, "low")`
- `SWITCH(x, k1, a1, k2, a2, ..., default)`: the result for the first key that equals `x`.
  Example: `SWITCH([region], "Java", 1, "Bali", 2, 0)`
- `AND(a, b, ...)`: true when every test is true.
  Example: `AND([amount] > 50, [paid])`
- `OR(a, b, ...)`: true when one test is true.
  Example: `OR([region] = "Java", [region] = "Bali")`
- `NOT(test)`: the opposite.
  Example: `NOT([paid])`
- `IFERROR(x, alt)`: `alt` when `x` is blank or empty text. Failures such as a division by zero are blank.
  Example: `IFERROR([amount] / [qty], 0)`
- `ISBLANK(x)`: true when the value is blank. Empty text counts as blank.
  Example: `ISBLANK([note])`
- `BLANK()`: a blank value.
  Example: `IF([amount] > 0, [amount], BLANK())`

### Math

- `ABS(x)`: the number without its sign.
  Example: `ABS([amount] - 100)`
- `ROUND(x, n)`: rounds to `n` places. A half goes away from zero, as in Excel. A negative `n` rounds
  to tens, hundreds, and so on. Without `n` the result is a whole number.
  Example: `ROUND([amount] / 3, 2)`
- `ROUNDUP(x, n)`: rounds away from zero.
  Example: `ROUNDUP([amount] / 1000, 0)`
- `ROUNDDOWN(x, n)`: rounds toward zero.
  Example: `ROUNDDOWN([amount], -2)`
- `INT(x)`: rounds down to a whole number. `INT(-2.5)` is -3.
  Example: `INT([amount] / 7)`
- `MOD(a, b)`: the rest of `a / b`, with the sign of `b`, as in Excel. `b = 0` gives a blank.
  Example: `MOD([qty], 2)`
- `POWER(a, b)`: `a` to the power of `b`. The same as `a ^ b`.
  Example: `POWER([qty], 2)`
- `SQRT(x)`: the square root. A negative number gives a blank.
  Example: `SQRT([amount])`
- `EXP(x)`: `e` to the power of `x`.
  Example: `EXP([qty])`
- `LN(x)`: the natural logarithm. Zero or less gives a blank.
  Example: `LN([amount])`
- `LOG10(x)`: the logarithm to base 10.
  Example: `LOG10([amount])`
- `SIGN(x)`: -1, 0 or 1.
  Example: `SIGN([amount] - 100)`
- `MIN(a, b, ...)`: the smaller of two or more values, row by row. It works on dates too. A blank is skipped.
  Example: `MIN([amount], 500)`
- `MAX(a, b, ...)`: the larger of two or more values, row by row.
  Example: `MAX([amount], 0)`
- `DIVIDE(a, b, alt)`: `a / b`, and `alt` when `b` is zero or blank. Without `alt` the result is blank.
  Example: `DIVIDE([amount], [qty], 0)`

### Text

Where a function wants text, a number or a date is turned into text the same way `&` does it.

- `a & b`: joins text.
  Example: `[customer] & " (" & [region] & ")"`
- `CONCAT(a, b, ...)`: the same as `&`.
  Example: `CONCAT([customer], " - ", [region])`
- `LEFT(text, n)`: the first `n` characters. The default is 1.
  Example: `LEFT([customer], 3)`
- `RIGHT(text, n)`: the last `n` characters. The default is 1.
  Example: `RIGHT([region], 2)`
- `MID(text, start, n)`: `n` characters from position `start`. The first character is 1.
  Example: `MID([region], 2, 3)`
- `LEN(text)`: the number of characters. A blank cell has 0.
  Example: `LEN([note])`
- `UPPER(text)`: capital letters.
  Example: `UPPER([region])`
- `LOWER(text)`: small letters.
  Example: `LOWER([region])`
- `PROPER(text)`: a capital letter at the start of each word.
  Example: `PROPER([customer])`
- `TRIM(text)`: removes spaces at both ends, and turns a run of spaces into one.
  Example: `TRIM([note])`
- `SUBSTITUTE(text, old, new)`: replaces every `old` with `new`. Case matters.
  Example: `SUBSTITUTE([note], "hello", "hi")`
- `FIND(find, text, start)`: the position of the first `find` in `text`. Case matters. Not found gives a blank.
  Example: `FIND("o", [note])`
- `SEARCH(find, text, start)`: the same, but case does not matter. No wildcards.
  Example: `SEARCH("WORLD", [note])`
- `VALUE(text)`: reads text as a number. Only plain numbers with a dot for decimals work, like
  `"1234.5"` or `"1e3"`. Anything else gives a blank, including `"1,234"`, `"12%"`, `"1,5"` and
  `"1.500,50"`. For numbers written the Indonesian way, take the dots out and change the comma:
  `VALUE(SUBSTITUTE(SUBSTITUTE(text, ".", ""), ",", "."))` reads `"1.500,50"` as 1500.5.
  Example: `VALUE([note])`
  Example: `VALUE(SUBSTITUTE(SUBSTITUTE([note], ".", ""), ",", "."))`
- `TEXT(x, format)`: a number as text. The formats are `"0"`, `"0.0"`, `"#,##0"` and `"0%"`.
  Example: `TEXT([amount], "#,##0")`

### Dates

- `DATE(year, month, day)`: builds a date. A month above 12 or a day above the month rolls over,
  as in Excel.
  Example: `DATE(2026, [qty], 1)`
- `YEAR(date)`, `MONTH(date)`, `DAY(date)`: the parts of a date.
  Example: `YEAR([date])`
  Example: `MONTH([date])`
  Example: `DAY([date])`
- `WEEKDAY(date)`: 1 for Monday to 7 for Sunday. This differs from Excel's default.
  Example: `WEEKDAY([date])`
- `WEEKNUM(date)`: the ISO week number, 1 to 53. Week 1 is the week with the first Thursday of the
  year, and weeks start on Monday. This differs from Excel's default: 2026-01-04 is in week 1 here
  and in week 2 in Excel.
  Example: `WEEKNUM([date])`
- `QUARTER(date)`: 1 to 4.
  Example: `QUARTER([date])`
- `FISCALYEAR(date)`: the year by the workspace's calendar (Settings › Year and week). A year that starts in
  April 2026 is 2027, the year it ends. With a year that starts in January it is `YEAR`.
  Example: `FISCALYEAR([date])`
- `FISCALQUARTER(date)`: 1 to 4, counted from the month the workspace's year starts in. With an April start,
  April to June is 1.
  Example: `FISCALQUARTER([date])`
- `EOMONTH(date, months)`: the last day of the month, `months` months after the date.
  Example: `EOMONTH([date], 0)`
- `EDATE(date, months)`: the same day, `months` months later. The day is cut to the end of a short month.
  Example: `EDATE([date], 1)`
- `DATEDIF(from, to, unit)`: whole days (`"D"`), months (`"M"`) or years (`"Y"`) between two dates.
  A month is complete when the day of the month has come round again, as in Excel: from
  2000-01-31 to 2000-02-29 is 0 months. If `to` is before `from`, the result is blank.
  Example: `DATEDIF([date], TODAY(), "M")`
- `DAYS(end, start)`: the days from `start` to `end`.
  Example: `DAYS(TODAY(), [date])`
- `TODAY()`: the date when the workspace was loaded or last refreshed.
  Example: `TODAY()`
- `NOW()`: the date and time when the workspace was loaded or last refreshed.
  Example: `NOW()`

### Totals (measures and group values)

A total reads many rows and gives one number. In a calculated column it is an error.

- `SUM(x)`, `AVERAGE(x)`: the sum and the mean of a number column.
  Measure example: `SUM([Orders].[amount])`
  Measure example: `AVERAGE([Orders].[amount])`
- `MIN(x)`, `MAX(x)`: the smallest and the largest. Numbers, dates and text work.
  Measure example: `MAX([Orders].[date])`
  Measure example: `MIN([Orders].[amount])`
- `COUNT(x)`: how many numbers or dates are filled. Use `COUNTA` for text.
  Measure example: `COUNT([Orders].[amount])`
- `COUNTA(x)`: how many values are filled, of any kind. Empty text does not count.
  Measure example: `COUNTA([Orders].[customer])`
- `COUNTD(x)`: how many different values.
  Measure example: `COUNTD([Orders].[customer])`
- `MEDIAN(x)`: the middle value.
  Measure example: `MEDIAN([Orders].[amount])`
- `PERCENTILE(x, p)`: the value below which the share `p` of the rows lie. `p` is a number from 0 to 1.
  Measure example: `PERCENTILE([Orders].[amount], 0.9)`
- `STDEV(x)`, `VAR(x)`: the spread of the values, computed on the sample.
  Measure example: `STDEV([Orders].[amount])`
  Measure example: `VAR([Orders].[amount])`
- `SUMIF(x, test)`: the sum of `x` over the rows where the test is true. The test reads the same table.
  Measure example: `SUMIF([Orders].[amount], [Orders].[region] = "Java")`
- `COUNTIF(x, test)`: how many filled values of `x` there are where the test is true.
  Measure example: `COUNTIF([Orders].[amount], [Orders].[amount] > 100)`
- `AVERAGEIF(x, test)`: the mean of `x` where the test is true.
  Measure example: `AVERAGEIF([Orders].[amount], [Orders].[qty] >= 2)`
- `COUNTROWS([Table])`: the number of rows of a table.
  Measure example: `COUNTROWS([Orders])`

A total can read a related column: `SUM([Orders].[amount] * [Stores].[rent])`. All the columns
in one total must lead, many to one, from one table.

The same total written twice is worked out once.

## Related columns

In a calculated column, `[Table].[Column]` reads the one row it relates to, like `RELATED` in Power BI.

Example: `[amount] / [Stores].[rent]`

A formula goes from the many side to the one side and never the other way. That holds for a link
table too (see [relationships](relationships.md#many-to-many-with-a-link-table)): a formula does not
cross it. A chart can group by a table behind a link table, and a measure works with that group,
because each of its tables is totalled on its own.

## Add a column

A calculated column is a formula that becomes a real column of a table. It works in a chart, a
filter, a map, a link, the column profile and an export, like any column from your file. Your file
is never changed.

1. Turn on **Advanced features** in Settings.
2. Open the **Data** view and pick the table.
3. Choose **Add column** (the ƒ button at the top of the table).
4. Give the column a name, and write the formula. Type `[` to see the columns and the related
   tables. Type the start of a function to see its name, what it takes and what it does. Use the
   arrow keys to move, **Enter** or **Tab** to insert and **Esc** to close the list.
5. Tenun checks the formula as you type. If it is wrong, the part that is wrong is underlined and
   a sentence says why. If it is right, you see its type and the first five values.
6. Choose **Add column**.

Examples for the Coffee sample:

- `Revenue incl. tax` is `[amount] * 1.11`.
- `Region` is `[stores].[region]`. The order reads the region of its store, from another file.

The column shows with an ƒ mark in the column list, in the field lists and in the Model view. Open
its menu for **Edit formula** and **Delete column**. A column menu can still **Rename** it. The name
you gave stays the real name, so tiles that use the column keep working. Rename changes only the
name shown.

### How a calculated column works

- **It is computed by the database each time the table loads.** The result is not stored in your file.
- **A column may use the ones above it,** never itself and never the ones below. The editor
  offers only the columns above. If you write the name of a later one, you are told.
- **A column of a related table gives one value for each row.** `[stores].[region]` is blank for
  an order whose store is not in the stores file. If the other table repeats a key, Tenun uses the
  first row of it (the first in the file; for a database, the row that sorts first by the key).
  A table never gets more rows from a calculated column, so no total is counted twice. The link
  keeps its warning in the Model view.
- **Two links to one table:** write the name of the link, `[Shipped from].[region]`. Name the links
  in the Model view first. See [relationships](relationships.md).
- **A table that reads another table's calculated column** is built after it. A chain across three
  tables works. Two tables that read each other are refused, and the message names both columns.
- **A column that reads another table cannot be the key of a link.** Its value would depend on
  another link. A calculated column that reads only its own table can be a key.
- **A column that cannot be computed is blank, and the table says why.** This happens when the file
  loses a column the formula reads, when a link is removed, or when the kinds no longer fit. The
  rest of the table loads. A tile that uses the column says so in a note. A published page, an
  exported file and a picture say only "The table: the column could not be computed", with no
  formula.
- **Names follow renames.** If you change the name shown for a column, every formula that wrote the
  old name is rewritten, also a formula of another table that writes it after the table or after
  the name of a link. The same happens when you rename a link, and when an unpivot gives its two new
  columns other names. Two columns can swap names in one save. The text between the names does not
  change. A SQL column is not rewritten, so a rename of an unpivot column that the SQL names is
  refused, with the name of the column.
- **A name shown must be new.** It cannot be a name another column of the table answers to, by its
  real name or the name shown, whatever the case. Tenun says so when you save it.
- **A name** may not be the name of a column of the table, may not hold `[` or `]`, and has at most
  100 letters.
- **A table can have up to 50 calculated columns.**
- **A database table works too.** The calculated column is made in Tenun. Nothing is written to the database.
- **An old workspace opens as it was.** A file without calculated columns is saved back exactly
  as it was read.

### Use SQL instead (Advanced)

For something the function list cannot do, choose **Use SQL instead** in the sheet. The formula is
then one DuckDB expression over the columns of the table, for example `strftime(date, '%Y-%m')` or
`CASE WHEN amount > 100 THEN 'big' ELSE 'small' END`. A name with spaces or capitals goes between
double quotes: `"Total sales" / 2`. A cell that cannot be worked out is blank, as in a formula.

A workspace file can come from someone else, and a published page may show what a column holds. So
SQL in a column is limited to **one expression**, checked before it is saved and every time the file
opens:

- **Allowed:** numbers, text (a `;` inside a text is only a character), the columns of the table,
  `+ - * / % ^ // || = <> < > <= >=`, `AND OR NOT`, `CASE`, `CAST` and `::`, `IN`, `BETWEEN`, `LIKE`,
  `IS [NOT] NULL`, `IS [NOT] DISTINCT FROM`, `DATE '2026-01-31'`, `INTERVAL`, and about 300 plain
  functions of numbers, text, dates and lists, from a list that people have read one by one:
  `lower`, `round`, `coalesce`, `strftime`, `date_part`, `regexp_matches`, `split_part`, and more.
  A function that DuckDB adds later is refused until it is added to the list.
- **Refused:** a `;`, a comment (`--` or `/* */`), a subquery, `SELECT`, `FROM`, `WITH`, `VALUES`,
  `JOIN`, `UNION`, `OVER` and any other word that starts a query. Any function that reads a file,
  the environment or the network (`read_text`, `read_csv`, `glob`, `getenv`, `current_setting`,
  `query`, `query_table`, `sniff_csv`), any function that changes from one call to the next
  (`random`, `uuid`, `nextval`), any function whose result is as big as an argument says (`repeat`,
  `lpad`, `rpad`, `range`, `generate_series`, `list_resize`, `printf`, `format`, `bar`), any
  lambda (`x -> x + 1`, `lambda x: …`, and so `list_transform` and `list_reduce`), totals such as
  `sum` (they belong to measures), and any name that is not a column of the table. The message
  names the word and the place is underlined.
- **Write `date_part('year', d)`, not `EXTRACT(year FROM d)`,** and `substr(s, 2, 3)`, not
  `SUBSTRING(s FROM 2 FOR 3)`. The word `FROM` is refused everywhere except in `IS DISTINCT FROM`.

How the limit works: Tenun reads the text itself and **denies by default.** Every word must be on the
short list above, on the list of functions, or be a column. A subquery needs a word that is not
on the list, and a table function can only follow `FROM`, so neither can be written. The expression
is then bound by DuckDB (`EXPLAIN`) and runs inside `TRY`. The check as a whole has 2 seconds: past
that, the column is refused with "Tenun stopped checking this column after 2 seconds." The limit is
the language of the list: an expression that needs something outside it is not possible in a column,
and the other tools of Tenun (formulas, links, measures) are the way to do it.

The same limits hold when a file is **opened**. A calculated column past the 50th, or with more than
10,000 characters, stays in the file and is not built. The table says so in its notes, and the
columns before it work as usual.

## Measures

A measure is a formula of totals with a name. You write it once and use it in any tile: a number, a
bar by store, a line by month. It can add up tables from different files and databases.

Example: `Achievement` is `SUM([sales].[amount]) / SUM([targets].[target])`.

1. Turn on **Advanced features** in Settings.
2. Open the **Data** view. Under the sources, choose **New measure**.
3. Give the measure a name and write the formula. Type `[` to see the tables, then `.[` to see the
   columns of the table you chose. Type the start of a function to see what it takes.
4. Tenun checks the formula as you type. You see the type, and the result for all the data. If the
   formula is wrong, the part that is wrong is underlined and a sentence says why.
5. Optional: choose how the number is written (a currency, decimals, short or full, a percent). A tile
   can still set its own format.
6. Choose **Add measure**.

In the tile editor, open the field list of a value. The measures are at the top, under **Measures**,
with an ƒ mark. A tile that uses a measure keeps working when you change the name or the formula, and it
shows the new numbers at once. Turning Advanced features off hides how to make and change measures, but
the tiles that use them go on working.

### Why the numbers are right

`sales` has many rows for each store and month. `targets` has one row for each store and month. If
Tenun joined the two tables first and added up after, each target would be counted once for each
sale. A store with three sales would show three times its target.

So Tenun adds up **each table on its own,** for the groups of the tile (the store, the month). Then it
puts the results side by side on the group values and runs your formula. The target is counted once. This holds
for every grouping: the whole, a store, a month, a store in a month.

- **A group that one table does not have** counts as 0 for a sum or a count, and as blank for an
  average, a middle value, and the like. A store with sales but no target has an achievement that is blank,
  because you cannot divide by 0. A month with a target and no sales has an achievement of 0.
- **A blank group is a group.** Sales with no region and targets with no region meet on "blank".
- **A measure beside a plain value** in one tile works. The plain value is calculated by itself, as it
  is in a tile of its own.

### Dates: the main date

A time group needs a date in each table. A chart of `Achievement` by month groups `sales` by its
date. The table `targets` is not related to `sales`, so it uses its own date, cut to the same month.
That date is the **main date** of the table.

- A table with one date column uses it. Nothing to do.
- A table with two or more date columns needs you to pick one. In the Data view, open the table and
  use **Main date** (with Advanced features on). The control shows when a table has two dates, or when
  a measure reads the table.
- **Only a main date is read through another table's main date.** The date of `sales` can line up
  `targets` because both tables hold events: some link starts from each of them, or no link touches
  the table at all (a `targets.csv` with a month and a target for the whole company, and no links, is a
  table of events). The launch date of a product, or the day a store opened, only describes a row:
  other tables point at that table, and none of its links starts from it. A group or a filter on it follows the plain
  rule: a table that cannot reach it is not split or not filtered by it, and says so. If you pick a
  main date for a table yourself, it counts, whatever the table is for.
- **The same holds for the table of a part.** `SUM([sales].[amount]) / SUM([stores].[sqm])` by the month
  of the sale: `stores` has only the day each store opened, so the floor space is not split by month. It
  is the same in every month, and the tile says *stores is not split by date. Its date opened is not a
  main date: pick it in Data if it is one.*
- A table with no links at all counts as a table of events. When it is not a part of the formula, a
  group on its date reads each part by the part's own date, and the tile says so: *products has no link
  to sales, so sales is split by its own date, date.* Draw the link, and the group follows it.
- A date filter works the same way. A filter on "last 30 days" of `sales.date` also filters `targets`
  by its main date.
- If a table has no main date, a chart over time cannot split it. It says so (see below).

**A main date that holds one row for each month** (or each quarter, or each year, always on the first
day) is read by that period. Tenun finds this from the data, once each time the data loads. A row
counts when the window holds its first day. So a filter from 10 March to 20 May counts the targets of
April and May, not March, and it counts May whole. The tile says so in words, and nothing is prorated:

- `targets is read by month: March is left out because the window starts on 10 March.`
- `targets is read by month: May is counted whole because the window ends on 20 May.`

A window across years names the years. "Last 12 months" starts in the middle of a month, so the first
month of the chart has no target and its achievement is blank, with the note above. A group by day is
finer than months: only the first day of each month has a target, and the note says so.

### What a measure does in each tile

- **A number, a bar, a line, an area, a table, a pivot, a heat map, a funnel, a scatter chart, a bullet
  chart, a compare chart, a calendar, a waterfall** can use a measure. The total row of a table, the
  rows and columns of a pivot, the "Other" of a top N, the comparison with last year and the number tile's
  comparison with the period before are all calculated again by Tenun with the same formula. Nothing is
  added up from numbers already added up.
- **Show as.** A share of the total, a running total and a year to date add values up. They work with
  a value that adds up: a sum, a count, or the difference of two of them. For a ratio, an average, a
  middle value, a distinct count, a lowest or a highest value (a plain value too), Tenun shows the value as
  it is and says why. The tile editor does not offer the ways that cannot work.
- **Share charts, waterfalls by category, Sankey charts,** a split chart's "Other", and a pace on a number tile
  also add values up, so they need a measure that adds up. A ratio like `Achievement` does not
  make a share, because the parts do not add up to a whole.
- **A histogram, a box plot, a cohort grid and a Gantt chart** read columns, not a formula. A measure
  is not offered there.
- **Show rows** lists the rows of the table behind a number. If a measure adds up two tables, there is
  no single list of rows, and Tenun says so. A click on a mark is read the way the measure reads it: for a
  measure of the targets, a click on a month of the sales is a click on that month of the targets.
- **A calculated column that could not be computed** makes the part that reads it blank, not 0, and the
  tile says which column. The same words go with a download or an exported file, with no formula in them.

### Changes and limits

- Rename a column, a name shown or a link, and the formulas of your measures follow.
- Deleting a measure that tiles use shows the tiles and asks. If you say yes, those tiles show an error
  until you pick another measure.
- A workspace uses up to 200 measures, and a formula has up to 10,000 characters. A file with more keeps
  them, and a tile that uses one says why it does not work.
- A measure is a formula only. There is no SQL mode for measures.
- A target on a number tile for a value written as a percent is typed as 95 for 95 %.


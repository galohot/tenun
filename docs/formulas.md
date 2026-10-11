# Formulas

A formula is a small calculation written the way Excel writes it. Tenun reads it, checks it, and
asks the database to do the work. Your files are never changed.

Formulas arrive with calculated columns and measures in Tenun 0.4. This page describes the language.

## Writing a formula

- **A column** is written in square brackets: `[amount]`.
- **A column of another table** has the table first: `[Stores].[rent]`.
- **A bracket inside a name** is written twice: the column `a]b` is `[a]]b]`.
- **Numbers** are written as `12`, `1.5`, `.5` or `1e3`.
- **Text** goes in double quotes: `"Java"`. A quote inside the text is written twice: `"say ""hi"""`.
- **True and false** are `TRUE` and `FALSE`.
- **A date** is built with `DATE(2026, 1, 31)`.
- **Operators:** `+ - * / ^ & = <> < > <= >=`. The `&` joins text.
- **Brackets** `( )` change the order. Powers go before products, and products before sums.
- **A comma** separates the parts of a function. A semicolon is an error.

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
- **`=` and `<>` never give a blank.** A blank cell equals only another blank, and `[note] = ""` is
  true for a blank cell. The other comparisons (`<`, `>`) give a blank when one side is blank.
- **`IF` treats a blank test as false.**
- **Text ignores case** when it is compared: `"Ana" = "ana"` is true. `FIND` and `SUBSTITUTE` do care.
- **`&` writes a number without a trailing zero:** `"n=" & 1` is `n=1`. A blank adds nothing.
- **Dates move by days.** `[date] + 30` is 30 days later. `[date2] - [date1]` is the number of days.
- **Only the four formats of `TEXT` exist,** and they always use a comma for thousands and a dot
  for decimals.

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
- `IFERROR(x, alt)`: `alt` when `x` is blank. Failures such as a division by zero are blank.
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
- `LEN(text)`: the number of characters.
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
- `VALUE(text)`: reads text as a number. Text that is not a number gives a blank.
  Example: `VALUE([note])`
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
- `WEEKNUM(date)`: the ISO week number, 1 to 53.
  Example: `WEEKNUM([date])`
- `QUARTER(date)`: 1 to 4.
  Example: `QUARTER([date])`
- `EOMONTH(date, months)`: the last day of the month, `months` months after the date.
  Example: `EOMONTH([date], 0)`
- `EDATE(date, months)`: the same day, `months` months later. The day is cut to the end of a short month.
  Example: `EDATE([date], 1)`
- `DATEDIF(from, to, unit)`: whole days (`"D"`), months (`"M"`) or years (`"Y"`) between two dates.
  If `to` is before `from`, the result is blank.
  Example: `DATEDIF([date], TODAY(), "M")`
- `DAYS(end, start)`: the days from `start` to `end`.
  Example: `DAYS(TODAY(), [date])`
- `TODAY()`: today's date.
  Example: `TODAY()`
- `NOW()`: the date and time now.
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

# Number and date formats

Tenun writes numbers so they are easy to read without any setting: `18.4K` on a chart, `18,412` in a
table, `Rp 1,2 jt` in Indonesian. When you want more, every number can carry a format.

## Where a format lives

A format can sit in four places. The more specific one wins, field by field.

1. **A column** (Data view, column menu, **Number format**). Every tile that shows a sum, an average, a
   minimum, a maximum, a median or the 90th percentile of the column uses it. A count does not, because
   a count of rupiah is not rupiah.
2. **A measure** (the measure editor). Every tile that uses the measure uses its format.
3. **A tile's value** (the tile editor, under the value). It goes over the column's or the measure's
   format. Set only what should differ; the rest stays.
4. **A parameter** (the parameter editor).

A date column can also have a **date format** (column menu, **Date format**).

## What a format can say

| Setting | Example |
|---|---|
| Currency: any of 162 currencies | `Rp 1.234.567` · `$1,234` · `£1,234` · `CHF 1,234` · `RM 1.234` |
| Show the code instead of the symbol | `USD 1,234` |
| Decimals, 0 to 6 | `1,234.50` |
| Significant digits, 1 to 6 | `1,230` · `0.012` |
| Size: automatic, short, every digit | `1.2M` · `1,234,567` |
| Size: always thousands, millions, billions or trillions | `1,234.6K` · `1.2M` · `1,2 jt` |
| Negative numbers: minus, brackets, red, brackets and red | `−1,234` · `(1,234)` |
| Text before and after | `~12.9K` · `1,234 pcs` · `21.5°C` · `$1,234/mo` |
| A percent | `25.6%` |
| A duration in seconds, minutes, hours or days | `1:02:05` in a table, `1h 2m` on a chart |
| What zero reads | `–` |
| What an empty cell reads (tables) | `n/a` |
| A format code (Advanced) | see below |

The editor shows a preview: the number short, in full, negative and zero.

A space goes between the text and the number when a letter or a digit touches it: `1,234 pcs`, but
`21.5°C`. Red negatives colour the cell in a table or a pivot, and the big number of a number tile.

## Digits

Settings › **Number and date format** › **Digits** picks how a workspace groups digits and marks the
decimal:

| Choice | Example |
|---|---|
| As the language (the default) | `1,234,567.5` in English, `1.234.567,5` in Indonesian |
| Comma and point | `1,234,567.5` |
| Point and comma | `1.234.567,5` |
| Space and comma | `1 234 567,5` |
| Apostrophe and point | `1’234’567.5` |
| Indian | `12,34,567.5` |

The words of a short number follow the language: `1.2M` in English, `1,2 jt` in Indonesian.

## Format codes (Advanced)

A format code uses Excel's syntax, so a code you already use in Excel works here. When a code is set, it
decides the whole text.

| Code | 1234.5 | −1234.5 | 0 |
|---|---|---|---|
| `#,##0.00` | `1,234.50` | `−1,234.50` | `0.00` |
| `#,##0.00;(#,##0.00);"–"` | `1,234.50` | `(1,234.50)` | `–` |
| `#,##0;[Red](#,##0)` | `1,235` | `(1,235)` in red | `0` |
| `#,##0.0,,"M"` (for 1,234,567) | `1.2M` | | |
| `[>=1000000]0.0,,"M";[>=1000]0.0,"K";0` | `1.2K` | `−1235` | `0` |
| `0.0%` (for 0.256) | `25.6%` | | |
| `0.00E+00` (for 1,234,567) | `1.23E+06` | | |
| `000-0000` (for 5551234) | `555-1234` | | |

What a code can hold:

- Up to four parts separated by `;`: positive; negative; zero; text. With one part, a negative number gets a
  minus sign. With two or more, the negative part is written without a sign.
- Conditions such as `[>=1000]` at the start of a part. The first part whose condition holds is used; a part
  without a condition takes the rest.
- `[Red]` and `[Green]` colour the value.
- `0` (a digit, always), `#` (a digit, if there is one), `?` (a digit or a space), `.` the decimal mark,
  `,` between digits for grouping, `,` after the last digit to divide by 1,000, `%`, `E+00`.
- Text in quotes (`"pcs"`), a single character after `\`, and `[$€-407]` for a currency symbol.

The marks follow the workspace's digit style, as Excel follows the reader's computer. If a code cannot
be read, the editor says why, and the other settings are used until it is fixed.

## Date formats

| Pattern | 15 October 2026, 14:30 |
|---|---|
| `d MMM yyyy` | `15 Oct 2026` (`15 Okt 2026` in Indonesian) |
| `dd/MM/yyyy` | `15/10/2026` |
| `yyyy-MM-dd` | `2026-10-15` |
| `EEE, d MMM yyyy` | `Thu, 15 Oct 2026` |
| `d MMM yyyy HH:mm` | `15 Oct 2026 14:30` |
| `d MMM yyyy h:mm a` | `15 Oct 2026 2:30 PM` |

Parts: `yyyy` `yy` (year), `M` `MM` `MMM` `MMMM` (month), `d` `dd` (day), `EEE` `EEEE` (weekday), `H` `HH`
`h` `hh` (hour), `mm` (minutes), `ss` (seconds), `a` (AM or PM), and `'text'` in single quotes.

A date format is used for single days: the Data view, tables, filters and charts by day. Weeks, months,
quarters and years keep their names (`Oct 2026`, `Q4 2026`).

## Everywhere the same

A format reads the same in the app, in an exported HTML file, on tenun.cloud, in an alert message and
in an Excel export. Excel gets an Excel number format, so the cells stay numbers. Excel shows them with
the marks of the computer that opens the file.

# Changelog

## 0.5.0 (2026-10-14)

This release adds drill-down, saved views, what-if parameters, forecasts, unusual values and alerts. A
published dashboard can now have a password, sit inside another site, and (soon) send email updates. An
AI app you already use can build dashboards in Tenun. A 0.4.0 workspace opens as it is.

Most new features are **Advanced**: turn on **Advanced features** in Settings. Anyone can pick a saved view.

### Read a dashboard more ways

- **Saved views (making them is Advanced):** a view remembers the clicks, what the filter tiles have chosen, the time grain and how far
  each chart is drilled. Save one for the Monday meeting and one for each region; choose which one the
  dashboard opens on. Exported and published dashboards hold every view.
- **Drill down (Advanced):** a chart over time goes from years to quarters, months and days; a chart by region
  goes to the stores of a region. Set it up once; anyone who reads the dashboard can use it, offline too.
- **Which tiles a click filters (Advanced):** a click on a tile can filter every tile, no tile, or the tiles
  you tick.
- **A filter for every page (Advanced):** pick Java on one page, and every page of the workspace shows Java.
- **Your year and your week (Advanced):** a business year that starts in April (FY 2027), and a week that
  starts on Saturday or Sunday. Year to date, "this year" and week numbers follow them.

### What-if, totals and balances (Advanced)

- **Parameters:** a value a reader moves on the dashboard, on a slider or from a list. A measure reads it with
  `PARAM("Growth")`; a Top N and a target can use it too. What if prices rise by 5 %?
- **Totals of a bigger group:** `TOTAL(...)` and `UNFILTERED(...)` compare a group with a bigger one: the share
  of a store in its region, or a month against the whole year. A calculated column can add up a related
  table's rows: each customer's orders.
- **Stock and balances:** `CLOSING(...)` and `OPENING(...)` read the last (first) day of each period, so the
  stock of a month is the stock on its last day, not the sum of every day.

### Analytics (Advanced)

- **Forecast:** a line over time shows its next periods as a dashed line with a 90 % band. Tenun fits it on
  your computer from the chart's own numbers, and says so in words when there are too few periods.
- **Unusual values:** a ring marks each value far from the periods around it, with a list under the chart.
- **More in the tooltip:** up to three more values and a small trend of each group when you point at it.

### Alerts (Advanced)

- **Alert me…** on a number tile: when its value goes below, above or outside the numbers you set, Tenun
  sends one message to Slack, Microsoft Teams, Discord, Telegram or any web address, and one more when it
  is back to normal. The app checks after each reload and once a minute; `tenun agent` checks on a computer
  that stays on; `tenun alerts check` checks once. The address stays on your computer. See
  [Keep published dashboards updated](docs/agent.md#alerts).

### Share online with tenun.cloud

- **Anyone with the link and the password:** the password stays on your computer and goes to tenun.cloud
  once; tenun.cloud keeps only a hash of it. A new password closes the page again for everyone.
- **Embed it in another site (Advanced):** "Allow embedding" gives the `<iframe>` code. A page for invited
  people cannot be embedded.
- **Email updates (Advanced):** after each update, daily or weekly, to you and to invited people who ask on
  the page. *tenun.cloud does not send emails yet:* sending needs a paid Cloudflare service. Your choice is
  kept, and the emails start when it does.
- `tenun publish --password-from-env VAR` sets a password from the command line.

### Use your own AI app

- **`tenun mcp`** lets Claude Desktop, Claude Code or any MCP app read a workspace and build dashboards in it.
  Each change shows in the Tenun app at once, with **Undo**. Tenun calls no AI and sends nothing anywhere;
  the AI app sends what it reads to its own company. It cannot run SQL, read files, see passwords or publish.
  See [Use your own AI app](docs/ai.md).

### Real files, faster

- Tested on 50 public files from government and open-data sites: title lines above a CSV header, Windows
  text, decimal commas, `.xls` files that are really `.xlsx`, a quote deep in a large CSV. All 49 readable
  ones load; a workbook with a password says so in words.
- A CSV is read once and kept in memory (up to 1 GB): tiles are 5 to 10 times faster, and an export works
  out its clicks in parallel. See [the benchmark](docs/benchmark.md).
- Fixed: a tile asked while a file reloaded could count rows twice.

## 0.4.0 (2026-10-13)

This release adds calculations, maps, number cards you design, pie and donut charts, text with live
numbers, story pages and sites. It builds on 0.3.0. A 0.3.0 workspace opens as it is.

### Calculations (Advanced)

Turn on **Advanced features** in Settings.

- **Calculated columns:** write a formula once, such as `[amount] * 1.11` or
  `IF([region] = "Java", "West", "East")`. The new column works in charts, filters, maps and exports.
  There are 64 functions with Excel names, and a formula can read a column of a related table. A
  column can use SQL instead.
- **Measures:** write `SUM([Orders].[amount]) / SUM([Targets].[target])` once, name it "Achievement",
  and use it in any tile, though the two tables come from different files. Tenun adds up each table on
  its own, so a target is never counted once for each sale.
- **Main date:** choose the date that a table's time charts use, so orders by day and targets by month
  line up by month.
- **New tables:** append tables (one file for each month), merge two tables on their keys, or group a
  table. A new table reads the tables you pick and never writes to your files.

See [the formula guide](docs/formulas.md).

### Relationships

- Tenun checks each link again every time a table loads. A key that stops being unique never counts a
  row twice, and the link and its tiles say so.
- A one-to-one link works in both directions.
- Two links to one table get names, such as "Shipped from" and "Sold at".
- A table that only links two others can be marked as a link table, so many to many works: sales by
  tag, with a note that the groups overlap.

See [the relationships guide](docs/relationships.md).

### Maps

- **Map tile:** colour places by a value. Indonesia's 38 provinces and 514 kabupaten and kota, and
  every country, ship with Tenun. The provinces and second level of another country download once,
  the first time a map needs them.
- Places are read from the standard names and codes (ISO 3166, Kemendagri) and the usual short forms
  (`Jabar`, `DKI Jakarta`, `Kab. Bandung`). A chip lists the names Tenun could not place.
- **Points** from a latitude and a longitude column, sized by a value.
- Click a place to filter the other tiles. An exported file carries its maps and works offline.

See [the map guide](docs/maps.md).

### Number cards

- Choose the parts of a number tile (the value, the change, a target, a small chart), their order,
  their size and the background. Rules colour the value and say why in words ("Below target").
- Six presets. Save a look as a **card style** and use it on many tiles. Change the style once, and
  every tile that uses it changes.

### Pie and donut

- Pie and donut charts are back, drawn with care: at most six slices and Other, largest first, the
  share on each slice, and only for values that add up to a whole. A donut shows the total in its
  hole.

### Text and stories

- **Rich text:** headings, numbered lists, quotes, callouts, lines, and links to other dashboards.
- **Live numbers in text:** "Revenue is 18.4K in Oct 1–14, ▲ 6.3 % against the same days before."
  The numbers follow the filters.
- **Story pages:** a dashboard that reads like a document. Text, numbers, charts and cards go in one
  column, and two half-width tiles can sit side by side. The PDF is A4 pages that never cut a chart.

### Sites

- **Make a workspace a site:** a title, a top bar or a sidebar, groups of pages, a home page, pages
  that only a link opens, and a footer line. Preview the site in the app.
- **Drill-through:** a click on a bar, a slice, a place or a row can open another page, kept to the
  value you clicked. The page says where you came from, and **Back** returns.
- **Export the whole site** as one HTML file, or as a folder (ZIP) for any web host. Both work offline.
  A drill-through works offline for the first 50 values of the field.
- **Publish the whole site** to tenun.cloud at one link (up to 10 pages).

### Copy and paste

- Copy tiles (Shift-click picks several) or a whole dashboard, and paste them on any dashboard, in
  this workspace or another. Named measures and card styles come along. From another workspace,
  **Match fields** asks which table and column holds the same data. **Duplicate dashboard** is in the
  dashboard's **⋯** menu.

### tenun.cloud

- Sites, as above. A site counts as one of the 3 free items.
- A published map draws from the shapes it carries.

## 0.3.0 (2026-10-10)

This release adds ways to shape numbers, a way to clean messy Excel sheets, more sources, filters,
more output, and an **Advanced features** switch. The switch is free. It builds on 0.2.0.

### Simple

- **Show value as:** % of total, % of group, running total, moving average, change, % change, rank
  and year to date. A time chart can draw the same period last year as a grey line.
- **More values:** median, 90th percentile, and a ratio of two values (for example amount per order).
- **Lines:** reference lines for target, average, median, minimum and maximum, and a trend line. Bars and good/bad colours in tables.
- **Data prep:** Tenun finds the header row, skips title and total rows, fills down grouped cells
  and turns months-as-columns into rows. You can rename, retype and hide columns, and see a
  profile of each column.
- **New sources:** a folder of files, a table pasted from the clipboard, and a web link to CSV or
  JSON data (for example a Google Sheet published to the web, or an API with pages and a key; the
  key stays outside the workspace file).
- **Filters:** filter tiles with a list, dropdown, number range or date range. Text filters.
  **Keep only** and **Exclude**. **Show rows** lists the data behind a bar. One time grain for the
  whole dashboard. Focus a tile.
- **Output:** a tile menu (duplicate, copy to another dashboard, PNG, CSV, Excel). The dashboard
  as a PNG or PDF. A phone layout for small screens. Keyboard and screen reader support.
- **Sample:** a second dashboard, **More views**, with a top 5 + Other bar chart, a pivot with
  totals, a waterfall by month, a number with a target, a text note and a filter.

### Advanced

Turn on **Advanced features** in Settings.

- **More charts:** heat map, funnel, box plot, bullet chart, compare two values (dumbbell and
  slope), Sankey, calendar heat map, cohort grid and Gantt chart.
- **Several values:** small multiples, separate panels for values with different units, indexed
  lines, and bars plus a line.
- **Tables and pivots:** a trend column in tables, bubble size on a scatter chart, and pivot groups
  that open and close with subtotals.
- **Also:** standard deviation and variance, a manual sort order, and reference bands.

Dashboards that use Advanced features open and work with the switch off.

## 0.2.0 (2026-10-10)

- **Settings, undo and redo,** a freshness stamp, and a banner when a refresh fails. Blanks show as
  "(blank)", missing periods are filled, and **Clear all** removes every filter.
- **Top N + Other:** keep the biggest groups and add the rest as one bar. Sort by value or label.
  Month of year, weekday and hour as groupings.
- **Number and date formats** for English and Indonesian.
- **New charts:** scatter, histogram, waterfall, and stack modes (stacked, side by side, 100 %).
- **Pivot table** with totals, and a **table** total row.
- **Number tile target** with a pace check, and **text** and **image** tiles.
- **Fixes:** stack default by unit, colours that follow the group, wording for numbers where lower
  is better, clearer table headings.
- The **Publish** button is hidden until tenun.cloud is live.

## 0.1.0

The first release: Excel, CSV and database sources, relationships across sources, click-to-filter,
live refresh and export to one HTML file.

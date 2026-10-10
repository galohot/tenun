# Changelog

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
- **New sources:** a folder of files, a table pasted from the clipboard, and a CSV link (for
  example a Google Sheet published to the web).
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

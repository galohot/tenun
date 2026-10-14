# Chart types

Pick a type in the **Inspector** (the panel on the right when a tile is selected). Most types
need only a value and a field to group by. Tenun can also suggest a few ways to show your fields:
look under **Also try** in the Inspector.

The types marked **Advanced** appear under **More charts** after you turn on **Advanced features**
in Settings. The switch is free. It only shows more controls. A dashboard that uses an Advanced
chart still opens, filters and exports the same way when the switch is off.

Every chart follows these rules: a fixed colour palette that is checked for colour blindness,
bars that start at zero, a legend when there are two or more series, and a word or an icon next
to every good or bad colour.

## Simple

| Type | Use it when | Fields |
|---|---|---|
| **Number** | You want one total, and how it compares with the last period. | One value. Optional: a date field for the period, a **Target**, and which direction is good. |
| **Bar** | You compare groups. | One or more values, a field to group by, optionally a field to split by. Horizontal or vertical. Stacked, side by side or 100 %. |
| **Line** | You follow a value over time. | One or more values, a date field, optionally a split. |
| **Area** | You follow a total over time and want to see its parts. | The same as a line. A split stacks by default. |
| **Share of total** | You show parts of a whole. | One value and one field to group by. Drawn as a 100 % strip with the exact shares listed. |
| **Pie** and **Donut** | You show parts of a whole, and there are only a few. | One value and one field to group by. At most six slices and Other (five unless you choose), largest first from twelve o'clock. The share is on each slice, or beside a small one; the value shows when you point at it. A donut shows the total in its centre, or the slice you point at. A value that does not add up (an average, a ratio) or one below 0 cannot be cut into slices: the tile says so and offers a bar chart. |
| **Map** | Your data names places: countries, provinces, kabupaten or kota. | One value and a field of place names or codes, or a latitude and a longitude for points. See [the map guide](maps.md). |
| **Table** | You need the exact numbers. | Any number of values and fields. Optional: a total row, bars or good/bad colours in the cells, a find box. |
| **Pivot table** | You know Excel pivots. | Rows (1 to 3 fields), one field for columns, and 1 to 4 values. Totals are on by default. |
| **Scatter** | You ask if two values move together. | Two values (X and Y), and a field for one dot each. Optional: a split for colour. |
| **Histogram** | You want to see how a number is spread. | One number field. Tenun picks the bin width. You can type your own. |
| **Waterfall** | You explain how a total builds up or changes. | One value and a field. With a category, each bar adds to the total. With a date, each bar is the change from the period before. |
| **Filter** | You want a control on the dashboard. | One field. Style: list, dropdown, number range or dates. |
| **Text** | You want a title, a note or a short story about the numbers. | Blocks of text: three sizes of heading, lists, quotes, callouts, a line, bold, italic, links to the web or to another dashboard, and live numbers. See [Text and live numbers](#text-and-live-numbers). |
| **Image** | You want a logo or a picture. | A PNG, JPEG or WebP file up to 1 MB. Add alt text. |

### Options that most charts share

- **Top N:** keep the biggest N groups. The rest become one **Other** bar, so the total stays true.
- **Sort:** by value or by label, up or down.
- **Show value as:** % of total, % of group, running total, moving average, change from the
  previous point, % change, rank, year to date.
- **Compare with:** the same period last year, drawn as a grey line.
- **Divide by:** a ratio of two values, such as amount per order.
- **More values:** median, 90th percentile, and up to four values in one chart.
- **Reference lines:** target, average, median, minimum, maximum, and a trend line with its R².
- **Data labels:** Auto, All or None.
- **Format:** decimals, currency, compact numbers and percent for each value.
- **Blanks:** a blank value is shown as **(blank)**. It is never dropped.
- **Missing periods:** a month with no rows is filled with zero for sums and counts.

## Number cards

A number tile can be laid out as a card. Select the tile and open **Card** in the Inspector.

- **Presets:** Classic (the number tile as it always was), Big number, With target, Compact, Trend
  and Split. Each shows a small live picture of your number.
- **Parts:** the title, the value, the period, the change, the target, the trend (columns or a
  line), another value of the tile, a note and a line. Drag a part to move it, or use the arrows.
  The eye hides a part and keeps its settings. **Another value** asks which value to show; it can
  add a value to the tile.
- **Size, Alignment, Ground:** three sizes of the value, left or centred, and a plain, tinted or
  accent background. Every colour comes from the theme and is checked for contrast in light and
  dark.
- **Rules:** "Below 100 000 is bad", "Above the target is good". The first rule the value meets
  colours it and adds words, such as "Below target". The words are always there, so the colour is
  never the only sign.
- **A short tile:** parts that do not fit drop from the bottom. A note goes first, then the trend,
  then other values. A "…" says that something is hidden. In a wide tile, a trend at the end sits
  beside the value, as on a classic number tile.
- **Card styles:** **Save as card style** keeps the look under a name. **Apply a style** gives it
  to another card. Change the style with **Update style from this card**, and every card that uses
  it follows. A card can still change one field for itself; a dot marks that field, and **Reset to
  style** takes it back. **Detach** keeps the look and drops the link. **Manage card styles**
  renames, copies and deletes styles. A card whose style is deleted keeps its look.

An exported file and a published dashboard carry every card as it looks, so they need no styles.

## Text and live numbers

A text tile is written in blocks. Select the tile: the Inspector shows each block with its kind on the left.

- **Kinds:** text, three headings, a bulleted or numbered list (one item on each line), a quote, a callout (Note,
  Good news or Warning: a tinted block with an icon and a word), and a line across. Click the kind to change it, type
  **/** in an empty block, or click **+** between blocks. Drag a block by its handle to move it.
- **Words:** select words, then **B**, **I**, the link button, or Ctrl+B, Ctrl+I, Ctrl+K (⌘ on a Mac). A link goes to
  a web address or to another dashboard of the workspace.
- **Live numbers:** **Insert number** picks a named measure and what to show: its value, its change against the period
  before ("▲ 6.3 %"), or the name of the period ("Oct 1–14, 2026"). The number follows the dashboard's filters and its
  grain, and a click on a bar changes it, as it changes every tile. Point at a number to see which measure it is. A
  measure that was deleted shows "—".
- **Style:** Body, Lead (a larger first paragraph) or Note (small and muted), left or centred.
- **Edit as text** shows the text itself, for people who like to type the marks: `###`, `1.`, `>`, `> [!note]`,
  `---`, `[a page](page:<dashboard id>)` and `{{measure:<id>}}`, with `|compare` or `|period`.

In an exported file the numbers are there and follow a click; a link to another dashboard is plain text.

## Story pages

A story is a dashboard that reads like a document. Make one with **+** beside **Dashboards** › **Story**, or turn a
dashboard into one with its **⋯** menu › **Layout** › **Story**.

- The tiles run down one column in the story's order. Text keeps to a reading width; charts may go wider.
- A tile's **⋯** menu sets its **Width** (full or half) and its **Height** (small, medium, large). Two half-width
  tiles share a row on a wide screen and stack on a phone. A text is as tall as its words.
- **+** between two tiles adds a tile there. Drag the handle on the left of a tile to move it.
- **Layout** › **Grid** turns the story back into a dashboard: every tile is where it was on the grid.
- An exported file shows the story as a column. **Export** › **PDF** prints it on A4 pages, cut between tiles: never
  through a chart.

## Sites and drill-through

A workspace can be a site: its dashboards become pages with a navigation. Open **Site…** under the dashboards in the
sidebar and switch on **Make this workspace a site**. It is off unless you switch it on.

- **Title**, **Navigation** (a top bar or a sidebar), the **Home page**, and a **Footer** line.
- **Groups:** add a group, name it, and pick a group for each page. Pages in no group come first. A page that is not
  **In the navigation** is still reached by a link in a text, or by a click that opens it.
- **Preview site** shows the site as an export will draw it. On a phone the navigation is a **Pages** button.
- **Export:** **Export** › **Whole site (one file)** makes one HTML file with every page, its navigation, the links
  between pages and the drill-throughs; it works offline and keeps its place in the address, so back and forward
  work. **Whole site (folder, ZIP)** makes a folder for any web host: `index.html` is the home page and every other
  page has its own file. A drill-through opens its page for the first 50 values of the clicked field; past them the
  page opens without the filter and says so. Tenun warns when the site is over 25 MB. From a terminal:
  `tenun snapshot <workspace.tenun> --site --html site.html` (or `--zip site.zip`).
- **Drill-through:** a chart's **⋯** menu › **On click** › **Open** a page. A click on a bar, a slice, a point, a place
  on a map or a table row then opens that page, keeping only the clicked value. The page says where you came from
  ("From Coffee overview: Store: Senja Menteng") and has a **Back** button. The click no longer filters its own page.

## Advanced

| Type | Use it when | Fields |
|---|---|---|
| **Heat map** | You look for hot spots across two fields. | Rows, columns and one value. Colour goes from light to dark. The number is in each cell. |
| **Funnel** | You follow steps where people drop out. | One value and a stage field. Order by value or by hand. Shows the % kept at each step. |
| **Box plot** | You compare how values are spread in each group. | One number field and a group field. Shows the median, quartiles, whiskers and outliers. |
| **Bullet chart** | You compare an actual value with a target. | One value, a group field, and a **Target**. Up to three bands. It replaces a gauge. |
| **Compare two values** | You compare two values or two periods per group. | Two values, or one value and a date field with two periods. Two values draw as a dumbbell. Two dates draw as a slope chart. |
| **Sankey** | You follow a flow through two or three stages. | Two or three category fields and one value. Each stage shows up to 25 nodes, the rest go into Other. |
| **Calendar heat map** | You look for patterns by weekday and week. | A date field and one value. Shows the last 12 months that have data. |
| **Cohort grid** | You ask how many customers come back. | The entity (for example `customer_id`), the date of the event and a grain. Each row is the group that started in one period. |
| **Gantt chart** | You show tasks over time. | A label field, a start date and an end date. Shown only when a table has two date columns. Up to 100 rows. |

### More Advanced options

- **Small multiples:** one small chart for each value of a field. They share one scale, or each has its own.
- **Several values, separate panels:** values with different units get their own plots on one time axis.
- **Indexed lines:** every line starts at 100, so you compare growth and not size.
- **Bars plus a line:** allowed only when the values share a unit.
- **Bubble size:** a third value on a scatter chart sets the size of the dot.
- **Trend column in a table:** a small line for each row, for the last 12 periods.
- **Pivot with groups that open and close,** with a subtotal for each group.
- **Standard deviation and variance** as values.
- **Manual sort order** for the groups.
- **Reference bands:** a shaded range on the axis.
- **Drill down:** see below.
- **Which tiles a click filters:** see below.
- **Saved views:** see below.
- **Your year and your week:** see below.

## Which tiles a click filters

A click on a bar filters every other tile of the page, and so does a filter tile. With Advanced on, a tile's **⋯**
menu has **Filters other tiles**: every tile (as always), no tile, or the tiles you tick. A filter tile's choice
follows the same setting. **Keep only** and **Exclude** still filter every tile. Exported and published files do
the same.

## A filter for every page

A filter tile can filter every page of the workspace (Advanced): select it and tick **Apply to every page**. Pick
Java on one page, and every other page shows Java too, with a chip "Every page: Region is Java". The chip's **×**
clears the choice where it was made.

In an exported site and on tenun.cloud, a list or dropdown filter tile marked this way works the same for its first
50 values. Past them, a page says that the file does not hold that value.

## Your year and your week

A business year can start in another month than January, and a week can start on Saturday or Sunday. With Advanced
on, set both in **Settings › Year and week**. They are saved in the workspace file.

- **A year that starts in April** runs from April 2026 to March 2027 and is called **FY 2027**, by the year it ends.
  Its first quarter is April to June: **Q1 FY 2027**. Charts by year and by quarter, "This quarter", "Year to date",
  a number tile's period and the formulas `FISCALYEAR` and `FISCALQUARTER` all follow it.
- **The week start** moves the weeks of a chart by week and the rows of a calendar heat map.
- Months, days and the parts of a date (a month of the year, a weekday) do not change. `YEAR` and `QUARTER` keep
  their Excel meaning.

## Saved views

A view remembers how a dashboard reads: the clicks, what the filter tiles have chosen, the time grain for every
chart, and how far each chart is drilled down. Use one for the Monday meeting, one for each region, and so on.

- **Make one** (Advanced): set the dashboard the way you want it, open **Views** next to its title, and choose
  **Save as a new view**. Give it a name, such as "Java, this quarter". A dashboard keeps up to 10 views.
- **Open one:** pick it in **Views**. Anyone who reads the dashboard can do this, with or without Advanced.
- **After a change,** the button says **changed**. **Update** saves the change into the view.
- **Open on this view first** makes it the first thing a reader sees.
- **Exported files and tenun.cloud** hold every view in full; a reader picks one at the top of the page. In an
  exported file the address keeps the view, so a link can open on it (`#v=v2`).

## Drill down

A chart over time can go from years to quarters, to months, to days. A chart by region can go to the stores of a
region. You set it up once (Advanced); anyone who reads the dashboard can use it.

- **Set it up:** select a bar, line, area, pie, donut, table or pivot. Under **Drill down**, choose **By date** (when
  its first field is a date by year, quarter, month or week) or a hierarchy.
- **A hierarchy** lists fields from the top down, such as Region › Store › Product. Make one in the **Model** view with
  **Hierarchies**. Each field must reach the one above it through your relationships; Tenun says which pair does not.
- **Use it:** the tile shows a drill button in its corner. With it on, a click on a bar opens the level below for that
  bar. The bar above the chart shows the path ("All years › 2026 › by quarter"). Click a part of it to go back there,
  or press **Up** or Esc. **Next level** shows the level below for every value at once (every quarter of every year).
- **With the drill button off,** a click filters the other tiles, as always. On a drilled chart it filters by the
  level shown, such as the month.
- **Exported files and tenun.cloud** hold one level of drill-down for the first 50 values of each chart, and Next level.
  A click past that says so. Maps do not drill, because a place can be written several ways in the data.

## Forecast

A line or an area over time can show what comes next (Advanced). In the tile's options, tick **Forecast**.

- **What it draws:** a dashed line in the series' colour from the last finished period, and a light band for the
  likely range. The legend and the hover call it **Estimate**. Under "Dec 2026", the hover says "Estimate 18.2 M,
  likely between 16.9 M and 19.5 M".
- **Periods ahead:** 3 months, 2 quarters, 8 weeks or 14 days unless you set another number (up to 36).
- **How:** exponential smoothing with a season and a slowing trend, the method Power BI and Tableau use. Tenun fits it
  on your computer to the periods the chart shows. The same data always gives the same forecast.
- **The season** is a year: 12 months, 4 quarters, 52 weeks; for days it is a week. A chart by year has no season,
  only a trend.
- **When it refuses:** it needs two full seasons and two more periods (26 months, 10 quarters, 106 weeks, 16 days,
  6 years). With fewer, the chart says how many it has.
- **The unfinished period** (this month, so far) is not used to fit. The estimate stands in for it.
- **The range** is where 9 in 10 values would fall if the past errors hold. It widens with each step ahead. Untick
  **Show the likely range** to hide it.
- One value per chart; a chart split into up to 4 lines gets one forecast for each.
- An exported or published page keeps the forecast as it was worked out.

It is an estimate from the past, not knowledge of the future. A price change, a new store or a holiday that moved
will not be in it.

## Unusual values

A line, an area or bars over time can mark the values that stand out (Advanced). In the tile's options, tick **Mark
unusual values**.

- **What it draws:** a ring around the point or the bar: in the bad colour when the value is above the usual, in the
  warning colour when it is below. Under the chart, "3 unusual values" opens a list of them with their words. The hover
  says it too: "About 4 times the usual spread above the 6 months around it (median 267M)".
- **How:** each value is read against the 6 periods around it (3 before and 3 after; more on one side at the ends),
  not counting itself. Their median is the usual value, and the usual spread is their median distance from it (× 1.4826,
  so it reads like a standard deviation). A value at least 3.5 spreads away is marked.
- **How many:** **Fewer** needs 4.5 spreads, **Some** 3.5 and **More** 2.5.
- When the periods around a value are all the same, any value that differs is marked, and the words say so.
- A series needs 8 periods with values. The unfinished period (this month, so far) is never marked.
- Each line of a split chart, and each value of a chart with several, is read on its own.
- An exported or published page keeps the marks.

Unusual is not wrong. A sale, a holiday or a new store can all make a month stand out. The mark says where to look.

## What Tenun does not draw

See [the README](../README.md#what-tenun-does-not-do-on-purpose) for the list and the answer to each.

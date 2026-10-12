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
| **Text** | You want a title or a note. | A short text. It supports headings, bold, italic, lists and links. |
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

## What Tenun does not draw

See [the README](../README.md#what-tenun-does-not-do-on-purpose) for the list and the answer to each.

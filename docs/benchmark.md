# Benchmark

How fast Tenun loads files and answers tiles. Measured with `cargo run --release --example bench`. Each time is the median of 5 runs after one warm-up run, in milliseconds (an Excel load: 3 runs; an export: 3 runs at 1 million rows and 1 run at 10 million). A tile is computed in full, as the app asks for it: a number tile, for example, also computes its comparison and its small chart. An export also computes, for offline use, what every click on every tile does to the other tiles.

Machine: 8 CPU threads, 15 GB of memory, linux.

The data: orders (a date over three years, a store, a customer, a product, an amount and a quantity), 50 stores in 5 regions, customers, and a target for each store and month. Orders link to stores and customers; targets link to stores.

## 100,000 rows

| Step | ms |
|---|---:|
| Load the orders (CSV) | 151 |
| Load the orders (Parquet) | 35.2 |
| Load the orders (Excel) | 502 |
| Suggest relationships | 7.9 |
| Number with a comparison | 24.7 |
| Bar by store, top 10 and Other | 17.1 |
| Line by month | 19.7 |
| Pivot: store by month | 68.4 |
| Measure over two tables, by region | 6.5 |
| Targets by region and month | 1.7 |
| Line by month, filtered by a click on a store | 3.3 |
| Add a calculated column (the table reloads) | 2.4 |
| Bar of the calculated column by region | 5.3 |
| Export a dashboard of six tiles | 2,844 |

The export is 603 KB. Peak memory of the run so far: 171 MB.

## 1,000,000 rows

| Step | ms |
|---|---:|
| Load the orders (CSV) | 1,791 |
| Load the orders (Parquet) | 68.3 |
| Load the orders (Excel) | 6,569 |
| Suggest relationships | 40.2 |
| Number with a comparison | 140 |
| Bar by store, top 10 and Other | 81.6 |
| Line by month | 82.3 |
| Pivot: store by month | 338 |
| Measure over two tables, by region | 28.9 |
| Targets by region and month | 4.0 |
| Line by month, filtered by a click on a store | 9.4 |
| Add a calculated column (the table reloads) | 5.3 |
| Bar of the calculated column by region | 25.5 |
| Export a dashboard of six tiles | 15,385 |

The export is 596 KB. Peak memory of the run so far: 1254 MB.

## 10,000,000 rows

| Step | ms |
|---|---:|
| Load the orders (CSV) | 11,144 |
| Load the orders (Parquet) | 55.3 |
| Suggest relationships | 159 |
| Number with a comparison | 1,104 |
| Bar by store, top 10 and Other | 534 |
| Line by month | 709 |
| Pivot: store by month | 2,162 |
| Measure over two tables, by region | 153 |
| Targets by region and month | 2.9 |
| Line by month, filtered by a click on a store | 38.5 |
| Add a calculated column (the table reloads) | 14.8 |
| Bar of the calculated column by region | 158 |
| Export a dashboard of six tiles | 172,043 |

The export is 616 KB. Peak memory of the run so far: 1338 MB.


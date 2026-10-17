# Benchmark

How fast Tenun loads files and answers tiles. Measured with `cargo run --release --example bench`. Each time is the median of 5 runs after one warm-up run, in milliseconds (an Excel load: 3 runs; an export: 3 runs at 1 million rows and 1 run at 10 million). A tile is computed in full, as the app asks for it: a number tile, for example, also computes its comparison and its small chart. An export also computes, for offline use, what every click on every tile does to the other tiles.

Machine: 8 CPU threads, 15 GB of memory, linux.

The data: orders (a date over three years, a store, a customer, a product, an amount and a quantity), 50 stores in 5 regions, customers, and a target for each store and month. Orders link to stores and customers; targets link to stores.

## 100,000 rows

| Step | ms |
|---|---:|
| Load the orders (CSV) | 183 |
| Load the orders (Parquet) | 42.0 |
| Load the orders (Excel) | 554 |
| Suggest relationships | 8.6 |
| Number with a comparison | 25.3 |
| Bar by store, top 10 and Other | 17.9 |
| Line by month | 15.3 |
| Pivot: store by month | 69.7 |
| Measure over two tables, by region | 6.8 |
| Targets by region and month | 1.6 |
| Line by month, filtered by a click on a store | 3.2 |
| Add a calculated column (the table reloads) | 3.0 |
| Bar of the calculated column by region | 5.2 |
| Export a dashboard of six tiles | 2,628 |

The export is 603 KB. Peak memory of the run so far: 174 MB.

## 1,000,000 rows

| Step | ms |
|---|---:|
| Load the orders (CSV) | 998 |
| Load the orders (Parquet) | 32.7 |
| Load the orders (Excel) | 5,731 |
| Suggest relationships | 23.8 |
| Number with a comparison | 102 |
| Bar by store, top 10 and Other | 56.4 |
| Line by month | 58.3 |
| Pivot: store by month | 246 |
| Measure over two tables, by region | 18.4 |
| Targets by region and month | 2.0 |
| Line by month, filtered by a click on a store | 6.6 |
| Add a calculated column (the table reloads) | 3.9 |
| Bar of the calculated column by region | 18.6 |
| Export a dashboard of six tiles | 14,063 |

The export is 595 KB. Peak memory of the run so far: 1220 MB.

## 10,000,000 rows

| Step | ms |
|---|---:|
| Load the orders (CSV) | 10,163 |
| Load the orders (Parquet) | 51.9 |
| Suggest relationships | 111 |
| Number with a comparison | 764 |
| Bar by store, top 10 and Other | 364 |
| Line by month | 426 |
| Pivot: store by month | 2,015 |
| Measure over two tables, by region | 127 |
| Targets by region and month | 2.1 |
| Line by month, filtered by a click on a store | 28.5 |
| Add a calculated column (the table reloads) | 10.3 |
| Bar of the calculated column by region | 149 |
| Export a dashboard of six tiles | 138,399 |

The export is 617 KB. Peak memory of the run so far: 1221 MB.


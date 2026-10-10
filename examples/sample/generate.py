"""Builds the Tenun sample: a small coffee chain, spread over three sources.

    uv run --with openpyxl examples/sample/generate.py

- coffee.xlsx      Orders + Products (two sheets)
- stores.csv       one row per store
- customers.sqlite one table of loyalty members

The numbers are invented. The seed is fixed, so the output never changes.
"""

import csv
import datetime as dt
import random
import sqlite3
from pathlib import Path

from openpyxl import Workbook
from openpyxl.styles import Font

OUT = Path(__file__).parent
rnd = random.Random(1975)

stores = [
    (1, "Senja Menteng", "Jakarta", "Java", "2021-03-01", 48),
    (2, "Senja Kemang", "Jakarta", "Java", "2022-07-15", 36),
    (3, "Senja Dago", "Bandung", "Java", "2021-11-20", 30),
    (4, "Senja Tunjungan", "Surabaya", "Java", "2023-02-01", 40),
    (5, "Senja Malioboro", "Yogyakarta", "Java", "2022-04-10", 28),
    (6, "Senja Seminyak", "Denpasar", "Bali & Nusa Tenggara", "2023-06-01", 44),
    (7, "Senja Ubud", "Gianyar", "Bali & Nusa Tenggara", "2024-01-15", 24),
    (8, "Senja Kesawan", "Medan", "Sumatra", "2023-09-01", 32),
    (9, "Senja Losari", "Makassar", "Sulawesi", "2024-05-20", 26),
]
weight = {1: 1.6, 2: 1.2, 3: 1.0, 4: 1.1, 5: 0.8, 6: 1.3, 7: 0.7, 8: 0.9, 9: 0.6}

products = [
    ("Kopi Susu", "Coffee", 28),
    ("Americano", "Coffee", 25),
    ("Cappuccino", "Coffee", 32),
    ("Es Kopi Gula Aren", "Coffee", 30),
    ("Matcha Latte", "Tea & Others", 35),
    ("Teh Tarik", "Tea & Others", 22),
    ("Cokelat", "Tea & Others", 30),
    ("Croissant", "Bakery", 26),
    ("Roti Bakar", "Bakery", 24),
    ("Banana Cake", "Bakery", 22),
]
pop = [9, 6, 6, 8, 4, 3, 3, 4, 3, 2]

segments = ["Regular", "Regular", "Regular", "Gold", "Gold", "Student"]
first = ["Ayu", "Budi", "Citra", "Dewi", "Eka", "Fajar", "Gita", "Hadi", "Indah", "Joko",
         "Kartika", "Lestari", "Made", "Nanda", "Oka", "Putri", "Rizky", "Sari", "Tono", "Wulan"]
last = ["Santoso", "Wijaya", "Pratama", "Lestari", "Hidayat", "Saputra", "Nugroho", "Kusuma",
        "Siregar", "Putra", "Halim", "Gunawan"]
customers = []
for cid in range(1, 401):
    joined = dt.date(2021, 1, 1) + dt.timedelta(days=rnd.randint(0, 1300))
    customers.append((cid, f"{rnd.choice(first)} {rnd.choice(last)}", rnd.choice(segments),
                      joined.isoformat(), rnd.choice(stores)[2]))

start, end = dt.date(2025, 1, 1), dt.date(2026, 10, 14)
orders = []
oid = 100000
day = start
while day <= end:
    growth = 1 + (day - start).days / 900
    weekend = 1.35 if day.weekday() >= 5 else 1.0
    season = 1.15 if day.month in (6, 7, 12) else 1.0
    for s in stores:
        if dt.date.fromisoformat(s[4]) > day:
            continue
        n = int(rnd.gauss(2.2, 0.6) * weight[s[0]] * growth * weekend * season)
        for _ in range(max(n, 0)):
            oid += 1
            p = rnd.choices(products, weights=pop)[0]
            q = rnd.choices([1, 2, 3], weights=[7, 2, 1])[0]
            member = rnd.random() < 0.55
            orders.append((oid, day, s[0], rnd.randint(1, 400) if member else None,
                           p[0], q, p[2] * q))
    day += dt.timedelta(days=1)

wb = Workbook()
ws = wb.active
ws.title = "Orders"
ws.append(["order_id", "date", "store_id", "customer_id", "product", "quantity", "amount"])
for o in orders:
    ws.append(list(o))
for c in ws[1]:
    c.font = Font(bold=True)
for cell in ws["B"][1:]:
    cell.number_format = "yyyy-mm-dd"
ps = wb.create_sheet("Products")
ps.append(["product", "category", "price"])
for p in products:
    ps.append(list(p))
for c in ps[1]:
    c.font = Font(bold=True)
wb.save(OUT / "coffee.xlsx")

with open(OUT / "stores.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["id", "name", "city", "region", "opened", "seats"])
    w.writerows(stores)

db = OUT / "customers.sqlite"
db.unlink(missing_ok=True)
con = sqlite3.connect(db)
con.execute("CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT, segment TEXT, joined DATE, home_city TEXT)")
con.executemany("INSERT INTO customers VALUES (?, ?, ?, ?, ?)", customers)
con.commit()
con.execute("VACUUM")
con.close()
print(f"{len(orders)} orders, {len(customers)} customers, {len(stores)} stores")

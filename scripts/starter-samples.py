#!/usr/bin/env python3
"""Write the sample CSV files of the starter dashboards (crates/tenun-core/starters/).

    python3 scripts/starter-samples.py

The data is made up. A fixed seed makes the same files every time, so the tests that count them
by hand keep their numbers. Uses the Python standard library only.
"""

import csv
import datetime
import io
import random
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "crates" / "tenun-core" / "starters"
r = random.Random(7)


def write(name, header, rows):
    s = io.StringIO()
    w = csv.writer(s, lineterminator="\n")
    w.writerow(header)
    w.writerows(rows)
    (OUT / name).write_text(s.getvalue())
    print(name, len(rows))


def days(start, end, workdays=False):
    d = start
    while d <= end:
        if not workdays or d.weekday() < 5:
            yield d
        d += datetime.timedelta(days=1)


# Sales: one row per sale.
products = [("Kopi Gayo 250 g", 85000), ("Teh Melati 100 g", 22000), ("Gula Aren 500 g", 38000),
            ("Keripik Singkong", 15000), ("Sambal Bawang", 27000), ("Kecap Manis 600 ml", 24000),
            ("Madu Hutan 350 g", 96000), ("Beras Pandan Wangi 5 kg", 78000), ("Kopi Toraja 250 g", 92000),
            ("Rendang Kaleng", 55000)]
cities = ["Jakarta", "Bandung", "Surabaya", "Medan", "Makassar", "Yogyakarta"]
rows = []
all_days = list(days(datetime.date(2026, 1, 1), datetime.date(2026, 6, 30)))
for i in range(420):
    d = r.choice(all_days)
    p, price = r.choices(products, weights=[9, 6, 5, 8, 6, 7, 3, 4, 3, 2])[0]
    c = r.choices(cities, weights=[10, 6, 7, 4, 3, 4])[0]
    q = r.randint(1, 12)
    rows.append((d.isoformat(), p, c, q, q * price))
rows.sort()
write("sales.csv", ["date", "product", "city", "qty", "amount"], rows)

# Budget against actual: one row per month, unit and account.
units = {"Keuangan": 0.6, "Pemasaran": 0.9, "Operasional": 1.4, "SDM": 0.5, "TI": 0.8}
accounts = {"Gaji": (120e6, 0.02), "Perjalanan": (18e6, 0.25), "Perlengkapan": (9e6, 0.2),
            "Sewa": (25e6, 0.0), "Pelatihan": (7e6, 0.4), "Listrik dan Air": (6e6, 0.12)}
rows = []
for m in range(1, 13):
    for u, size in units.items():
        for a, (base, spread) in accounts.items():
            budget = round(base * size / 1e5) * 1e5
            actual = round(budget * (1 + r.uniform(-spread, spread * 0.8)) / 1e3) * 1e3
            if a == "Pelatihan" and m in (1, 2):
                actual = 0
            rows.append((datetime.date(2026, m, 1).isoformat(), u, a, int(budget), int(actual)))
write("budget.csv", ["month", "unit", "account", "budget", "actual"], rows)

# Stock: one row per item and warehouse, counted on one day.
items = {"Minuman": ["Air Mineral 600 ml", "Teh Botol 350 ml", "Kopi Sachet", "Susu UHT 1 l", "Jus Jeruk 250 ml", "Sirup Melon"],
         "Makanan": ["Mi Instan Goreng", "Mi Instan Kuah", "Biskuit Kelapa", "Roti Tawar", "Wafer Cokelat", "Kacang Atom", "Abon Sapi"],
         "Bahan pokok": ["Beras 5 kg", "Minyak Goreng 2 l", "Gula Pasir 1 kg", "Tepung Terigu 1 kg", "Telur 1 kg", "Garam 250 g"],
         "Kebersihan": ["Sabun Mandi", "Sampo Sachet", "Pasta Gigi", "Deterjen 800 g", "Sabun Cuci Piring", "Tisu Gulung"]}
cost = {}
for cat, names in items.items():
    for n in names:
        cost[n] = r.choice([3500, 4500, 6000, 9500, 12000, 15500, 18000, 24000, 32000, 68000])
warehouses = ["Gudang Cikarang", "Gudang Sidoarjo", "Gudang Deli Serdang", "Gudang Gowa"]
rows = []
for w in warehouses:
    for cat, names in items.items():
        for n in names:
            q = max(0, int(r.gauss(220, 140)))
            if r.random() < 0.08:
                q = r.randint(0, 6)
            rows.append((n, cat, w, q, q * cost[n]))
write("stock.csv", ["item", "category", "warehouse", "stock", "value"], rows)

# Attendance: one row per person per working day.
people = {"Keuangan": ["Ayu Lestari", "Bima Saputra", "Citra Dewi", "Dimas Prasetyo"],
          "Pemasaran": ["Eka Putri", "Fajar Nugroho", "Gita Maharani", "Hendra Wijaya", "Intan Permata"],
          "Operasional": ["Joko Santoso", "Kartika Sari", "Lukman Hakim", "Maya Anggraini", "Nanda Pratama"],
          "TI": ["Oki Firmansyah", "Putu Ardana", "Rina Kusuma", "Sigit Ramadhan"]}
rows = []
for d in days(datetime.date(2026, 8, 3), datetime.date(2026, 9, 4), workdays=True):
    if d == datetime.date(2026, 8, 17):
        continue  # a holiday
    for unit, names in people.items():
        for n in names:
            s = r.choices(["Hadir", "Terlambat", "Izin", "Sakit", "Cuti", "Alpa"], weights=[84, 6, 3, 3, 3, 1])[0]
            rows.append((d.isoformat(), n, unit, s))
write("attendance.csv", ["date", "name", "department", "status"], rows)

# Survey: one row per answer, a score from 1 to 5.
questions = {"Keramahan staf": 4.3, "Kecepatan layanan": 3.6, "Kebersihan tempat": 4.0,
             "Harga sesuai kualitas": 3.4, "Kualitas produk": 4.1, "Akan merekomendasikan": 3.9}
outlets = {"Jakarta": 0.1, "Bandung": 0.2, "Surabaya": -0.1, "Medan": -0.3, "Makassar": 0.0}
rows = []
n = 0
for o, shift in outlets.items():
    for k in range(12):
        n += 1
        rid = f"R{n:03d}"
        d = datetime.date(2026, 7, 1) + datetime.timedelta(days=r.randint(0, 91))
        for q, mean in questions.items():
            s = min(5, max(1, round(r.gauss(mean + shift, 0.8))))
            rows.append((rid, d.isoformat(), o, q, s))
write("survey.csv", ["respondent", "date", "outlet", "question", "score"], rows)

#!/usr/bin/env python3
"""Synthèse des runs longues (banc `longue`) : par variante × scène × seed.

Usage : DIR=out/longue/<chantier> python3 scripts/analyse_longue.py <variante>…
Les CSV sont nommés <variante>_<froid|tempere>_<seed>.csv."""
import csv, sys, statistics as st, os
D = os.environ.get("DIR", "out/longue/mar6")
variants = sys.argv[1:] or ["main", "sans-ancre"]
def load(path):
    with open(path) as f:
        return list(csv.DictReader(f))
def num(r, k):
    try: return float(r[k])
    except: return float("nan")
print(f"{'run':<26}{'an':>5}{'pop':>5}{'morts':>6}{'faim':>5}{'soif':>5}{'clans':>6}{'foyer→départ km':>16}{'min inter km':>13}{'chemin km/an':>13}{'huttes':>7}{'têtes/an':>9}{'marche h':>9}")
for v in variants:
    for sc in ["froid", "tempere"]:
        for s in ["42", "7", "1337", "2024"]:
            p = f"{D}/{v}_{sc}_{s}.csv"
            if not os.path.exists(p): continue
            rows = load(p)
            if not rows: continue
            last = rows[-1]
            yrs = num(last, "an")
            tail = rows[-360:]
            fd = st.median([num(r, "foyer_depart_km") for r in tail if r["foyer_depart_km"] not in ("NaN", "nan")] or [float("nan")])
            inter = [num(r, "inter_clans_km") for r in tail if r["inter_clans_km"] not in ("NaN", "nan")]
            path = sum(num(r, "foyers_km_jour") for r in rows) / max(yrs, 1e-9)
            heads = sum(num(r, "chasse_tetes") for r in rows) / max(yrs, 1e-9)
            walk = st.mean(num(r, "marche_h_adulte") for r in rows)
            print(f"{v+' '+sc+' '+s:<26}{yrs:>5.1f}{last['pop']:>5}{last['morts']:>6}{last.get('morts_Starvation','?'):>5}{last.get('morts_Dehydration','?'):>5}{last['clans']:>6}{fd:>16.1f}{(min(inter) if inter else float('nan')):>13.1f}{path:>13.1f}{last['huttes_chef']:>7}{heads:>9.0f}{walk:>9.2f}")

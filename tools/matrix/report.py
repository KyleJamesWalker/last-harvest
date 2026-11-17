#!/usr/bin/env python3
"""Turn the matrix CSVs from tools/matrix/run.py into one report page.

Usage: uv run --no-project --with numpy tools/matrix/report.py [--out DIR] [--findings FILE]

Writes DIR/results.json and DIR/report.html (DIR defaults to target/matrix). --findings puts an HTML fragment
of conclusions at the top of the page. Tiers not run yet show as empty.
"""
import argparse
import csv
import json
import math
import sys
from collections import defaultdict
from itertools import combinations
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run import ROOT, powerups  # noqa: E402

ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
ap.add_argument("--out", type=Path, default=ROOT / "target/matrix")
ap.add_argument("--findings", type=Path)
args = ap.parse_args()
OUT = args.out
W, P, TOP, ZONE = powerups()
NIGHT = 600.0
SETTINGS = {}


def load(name):
    p = OUT / f"{name}.csv"
    if not p.exists():
        return None
    lines = p.read_text().splitlines()
    SETTINGS[name] = next((l.lstrip("# ") for l in lines if l.startswith("#")), "the defaults of an older sandbox")
    rows = list(csv.DictReader(l for l in lines if not l.startswith("#")))
    alls, per = {}, defaultdict(dict)
    for r in rows:
        for k in ("kills", "kills_sd", "useful", "raw", "overkill", "dps", "fell_at", "spawned", "capped", "level_reached", "hearts", "survival", "censored", "seeds"):
            r[k] = float(r[k]) if r.get(k) not in (None, "") else math.nan
        if r["weapon"] == "all":
            alls[r["build"]] = r
        else:
            per[r["build"]][r["weapon"]] = r
    return alls, per


def fell(r):
    """Mean time to the first fall, a night that never fell counting as its length; older CSVs only have fell_at."""
    if not math.isnan(r.get("survival", math.nan)):
        return r["survival"]
    return NIGHT if math.isnan(r["fell_at"]) else r["fell_at"]


def never_fell(r):
    if not math.isnan(r.get("censored", math.nan)):
        return r["censored"] / r["seeds"]
    return float(fell(r) >= NIGHT)


def slim(r):
    return {"kills": round(r["kills"]), "sd": round(r["kills_sd"]), "dps": round(r["dps"], 1), "fell": round(fell(r)),
            "overkill": round(r["overkill"], 3), "capped": round(r["capped"], 3), "ceiling": r["ceiling"] == "true",
            "level": round(r["level_reached"], 1), "spawned": round(r["spawned"])}


res = {}

# Tier 1: each weapon alone, curves per setting.
for name in ("t1_singles_normal", "t1_singles_eight", "t1_singles_pressure"):
    d = load(name)
    if not d:
        continue
    alls, _ = d
    curves = {w: {} for w in W}
    for b, r in alls.items():
        w, lvl = b.split(":")
        curves[w][lvl] = slim(r)
    res[name] = curves


def pair_table(name, at_top):
    d = load(name)
    if not d:
        return None
    alls, _ = d
    single = {b.split(":")[0]: alls[b] for b in alls if "," not in b}
    out = []
    for a, b in combinations(W, 2):
        la, lb = (TOP[a], TOP[b]) if at_top else (4, 4)
        r = alls.get(f"{a}:{la},{b}:{lb}") or alls.get(f"{b}:{lb},{a}:{la}")
        if r is None:
            continue
        ka, kb, kab = single[a]["kills"], single[b]["kills"], r["kills"]
        fa, fb, fab = fell(single[a]), fell(single[b]), fell(r)
        out.append({"a": a, "b": b, "kills": round(kab), "best": round(max(ka, kb)), "sum": round(ka + kb),
                    "lift": round(kab / max(ka, kb), 3), "share_of_sum": round(kab / (ka + kb), 3),
                    "fell": round(fab), "fell_best": round(max(fa, fb)), "fell_gain": round(fab - max(fa, fb)),
                    "zones": "+".join(sorted({ZONE[a], ZONE[b]})) if ZONE[a] != ZONE[b] else ZONE[a] + " x2"})
    # Synergy: residual of kills against a fit on the two singles, so strong pairs aren't ranked only for being strong.
    X = np.array([[1, p["best"], p["sum"] - p["best"]] for p in out], float)
    y = np.array([p["kills"] for p in out], float)
    coef, *_ = np.linalg.lstsq(X, y, rcond=None)
    for p, pred in zip(out, X @ coef):
        p["synergy"] = round(p["kills"] - pred)
    zones = defaultdict(list)
    for p in out:
        zones[p["zones"]].append(p["synergy"])
    return {"pairs": out, "fit": [round(c, 3) for c in coef],
            "zones": {z: {"mean": round(sum(v) / len(v)), "n": len(v)} for z, v in sorted(zones.items())},
            "singles": {w: slim(single[w]) for w in single}}


for name, at_top in (("t2_pairs_pressure", True), ("t2_pairs4_normal", False)):
    t = pair_table(name, at_top)
    if t:
        res[name] = t

d = load("t3_passives_pressure")
if d:
    alls, _ = d
    single = {b.split(":")[0]: alls[b] for b in alls if "," not in b}
    cells = []
    for w in W:
        for p in P:
            r = alls.get(f"{w}:{TOP[w]},{p}:{TOP[p]}")
            if r is None:
                continue
            cells.append({"w": w, "p": p, "kills": round(r["kills"]), "base": round(single[w]["kills"]),
                          "gain": round(r["kills"] - single[w]["kills"]),
                          "gain_pct": round(100 * (r["kills"] / single[w]["kills"] - 1), 1),
                          "fell_gain": round(fell(r) - fell(single[w]))})
    by_p = defaultdict(list)
    for c in cells:
        by_p[c["p"]].append(c["gain_pct"])
    res["t3_passives_pressure"] = {"cells": cells,
                                   "passive_mean_pct": {p: round(sum(v) / len(v), 1) for p, v in by_p.items()}}

d = load("t4_triples_pressure")
if d:
    alls, _ = d
    pairs = {}
    if "t2_pairs_pressure" in res:
        pairs = {frozenset((p["a"], p["b"])): p for p in res["t2_pairs_pressure"]["pairs"]}
    tri = []
    for b, r in alls.items():
        ws = [x.split(":")[0] for x in b.split(",")]
        best_pair = max((pairs[frozenset(c)]["kills"] for c in combinations(ws, 2) if frozenset(c) in pairs), default=0)
        tri.append({"ws": ws, "kills": round(r["kills"]), "fell": round(fell(r)), "ceiling": r["ceiling"] == "true",
                    "best_pair": best_pair, "over_pair": round(r["kills"] - best_pair),
                    "zones": len({ZONE[w] for w in ws})})
    tri.sort(key=lambda t: (-t["fell"], -t["kills"]))
    by_zones = defaultdict(list)
    for t in tri:
        by_zones[t["zones"]].append(t["fell"])
    res["t4_triples_pressure"] = {"top_survival": tri[:25], "top_kills": sorted(tri, key=lambda t: -t["kills"])[:25],
                                  "worst": sorted(tri, key=lambda t: (t["fell"], t["kills"]))[:10],
                                  "by_zone_count": {k: round(sum(v) / len(v)) for k, v in sorted(by_zones.items())},
                                  "n": len(tri), "at_ceiling": sum(t["ceiling"] for t in tri)}
    # Weapon appearances in the top 10% of triples by survival.
    top = tri[: max(1, len(tri) // 10)]
    cnt = defaultdict(int)
    for t in top:
        for w in t["ws"]:
            cnt[w] += 1
    res["t4_triples_pressure"]["top10_share"] = {w: round(cnt[w] / len(top), 3) for w in W}

d = load("t5_random_pressure")
if d:
    alls, per = d
    feats = W + P
    idx = {f: i for i, f in enumerate(feats)}
    inter = [(a, b) for a, b in combinations(W, 2)] + [(w, p) for w in W for p in P]
    rows, yk, yf, nf = [], [], [], []
    shares = defaultdict(list)
    for b, r in alls.items():
        lv = {}
        for part in b.split(","):
            name, l = part.split(":")
            top = TOP[name]
            lv[name] = min(float(l), top + 1) / top
        x = [lv.get(f, 0.0) for f in feats] + [lv.get(a, 0.0) * lv.get(c, 0.0) for a, c in inter]
        rows.append(x)
        yk.append(r["kills"])
        yf.append(fell(r))
        nf.append(never_fell(r))
        tot = sum(v["useful"] for v in per[b].values()) or 1
        for w, v in per[b].items():
            if w in idx and w in W:
                shares[w].append((lv.get(w, 0), v["useful"] / tot))
    X = np.array(rows)
    X = np.hstack([np.ones((len(X), 1)), X])

    def ridge(y, lam):
        y = np.array(y)
        n = X.shape[1]
        reg = lam * np.eye(n)
        reg[0, 0] = 0
        beta = np.linalg.solve(X.T @ X + reg, X.T @ y)
        pred = X @ beta
        r2 = 1 - ((y - pred) ** 2).sum() / ((y - y.mean()) ** 2).sum()
        return beta, r2

    out = {"n": len(rows)}
    for label, y in (("kills", yk), ("fell", yf)):
        beta, r2 = ridge(y, 5.0)
        main = {f: round(beta[1 + i], 1) for i, f in enumerate(feats)}
        ints = [{"a": a, "b": c, "coef": round(beta[1 + len(feats) + j], 1)} for j, (a, c) in enumerate(inter)]
        ints.sort(key=lambda t: -abs(t["coef"]))
        out[label] = {"r2": round(r2, 3), "main": main, "top_interactions": ints[:30]}
    out["useful_share"] = {w: round(sum(s for _, s in v) / len(v), 3) for w, v in shares.items() if v}
    out["mean_kills"] = round(float(np.mean(yk)))
    out["mean_fell"] = round(float(np.mean(yf)))
    out["never_fell"] = round(float(np.mean(nf)), 3)
    res["t5_random_pressure"] = out

res["weapons"], res["passives"], res["settings"] = W, P, SETTINGS
(OUT / "results.json").write_text(json.dumps(res, indent=1))
findings = args.findings.read_text() if args.findings else "<p>No conclusions written for this run.</p>"
page = (Path(__file__).resolve().parent / "report.html").read_text()
page = page.replace("/*DATA*/null", json.dumps(res, separators=(",", ":"))).replace("<!--FINDINGS-->", findings)
(OUT / "report.html").write_text(page)
print("wrote", OUT / "report.html")

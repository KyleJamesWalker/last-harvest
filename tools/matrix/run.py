#!/usr/bin/env python3
"""Run the sandbox balance matrix, tier by tier, into CSV files.

Usage: tools/matrix/run.py [--out DIR] [--random N] [--player WORDS] [TIER ...]

Tiers (all by default; name a prefix such as t2 to run fewer):
  t1_singles_normal     each weapon alone at levels 1..top and its breaker, 20 seeds
  t1_singles_eight      the same with the figure-eight walk, 10 seeds
  t1_singles_pressure   the same under the pressure pack, 10 seeds
  t2_pairs_pressure     every pair of weapons at top, under pressure, 10 seeds
  t2_pairs4_normal      every pair at level 4 on normal rules, 10 seeds
  t3_passives_pressure  every weapon at top with every passive at top, under pressure, 8 seeds
  t4_triples_pressure   every set of three weapons at top, under pressure, 3 seeds
  t5_random_pressure    N random builds of 6 weapons and 6 passives at random levels, under pressure, 1 seed each

The pressure tiers lay tools/matrix/pressure over the game's data, which needs the `[spawn]` section of tuning.txt.
A finished tier's CSV is kept and skipped on the next run; delete it to run that tier again.

--player sets how the scripted player plays every tier, as comma-separated words: a walk (circle, eight, avoid), a
pick (none, random, held, first:a+b), nothrow for no grenades and locked for a new player's unlocks. Its tiers go
in a folder of their own under DIR, named for the words, such as target/matrix/avoid-held. The eight tier keeps its
figure eight.
"""
import argparse
import json
import itertools
import random
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
from manual_items import sections  # noqa: E402


def powerups():
    weapons, passives, top, zone = [], [], {}, {}
    for pid, f in sections(ROOT / "crates/game/powerups.txt"):
        kind = f.get("kind")
        if kind == "weapon":
            weapons.append(pid)
        elif kind == "passive":
            passives.append(pid)
        top[pid] = int(f.get("max", "1"))
        zone[pid] = {"map": "far"}.get(f.get("zone", ""), f.get("zone", ""))
    return weapons, passives, top, zone


def tiers(weapons, passives, top, random_builds):
    rng = random.Random(7)
    levels = lambda w: [str(n) for n in range(1, top[w] + 1)] + ["break"]
    singles = [f"{w}:{l}" for w in weapons for l in levels(w)]
    yield "t1_singles_normal", singles, 20, False, ()
    yield "t1_singles_eight", singles, 10, False, ("--walk", "eight")
    yield "t1_singles_pressure", singles, 10, True, ()
    alone = [f"{w}:top" for w in weapons]
    yield "t2_pairs_pressure", [f"{a}:top,{b}:top" for a, b in itertools.combinations(weapons, 2)] + alone, 10, True, ()
    yield "t2_pairs4_normal", [f"{a}:4,{b}:4" for a, b in itertools.combinations(weapons, 2)] + [f"{w}:4" for w in weapons], 10, False, ()
    yield "t3_passives_pressure", [f"{w}:top,{p}:top" for w in weapons for p in passives] + alone, 8, True, ()
    yield "t4_triples_pressure", [",".join(f"{w}:top" for w in t) for t in itertools.combinations(weapons, 3)], 3, True, ()
    full = []
    for _ in range(random_builds):
        ws, ps = rng.sample(weapons, 6), rng.sample(passives, 6)
        full.append(",".join([f"{w}:{rng.choice(levels(w))}" for w in ws] + [f"{p}:{rng.randint(1, top[p])}" for p in ps]))
    yield "t5_random_pressure", full, 1, True, ()


WALKS = ("circle", "eight", "avoid")
PICKS = ("none", "random", "held")


def player_flags(words):
    flags = []
    for w in filter(None, (w.strip() for w in words.split(","))):
        if w in WALKS:
            flags += ["--walk", w]
        elif w in PICKS or w.startswith("first:"):
            flags += ["--pick", w]
        elif w == "nothrow":
            flags += ["--no-throw"]
        elif w == "locked":
            flags += ["--locked"]
        else:
            sys.exit(f"--player: {w} is not a walk, a pick, nothrow or locked")
    return flags


def run(sandbox, out, tier, builds, seeds, extra, chunk=120):
    t0, parts = time.time(), []
    for i in range(0, len(builds), chunk):
        part = out.with_suffix(f".part{i // chunk}.csv")
        cmd = [str(sandbox), "--seeds", str(seeds), "--seed", str(1 + i), "--csv", str(part), *extra, *builds[i:i + chunk]]
        subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        parts.append(part)
        print(f"{tier}: {min(i + chunk, len(builds))}/{len(builds)} builds, {time.time() - t0:.0f}s", flush=True)
    with out.open("w") as f:
        for n, part in enumerate(parts):
            lines = part.read_text().splitlines(keepends=True)
            f.writelines(lines if n == 0 else [l for l in lines if not l.startswith("#")][1:])
            part.unlink()


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=Path, default=ROOT / "target/matrix")
    ap.add_argument("--random", type=int, default=5000, help="builds in the random tier (5000)")
    ap.add_argument("--player", default="", help="how the scripted player plays, such as avoid,held,nothrow")
    ap.add_argument("tiers", nargs="*")
    a = ap.parse_args()
    player = player_flags(a.player)
    if player:
        a.out = a.out / a.player.replace(",", "-").replace(":", "_").replace("+", "_")
    subprocess.run(["cargo", "build", "-q", "--release", "-p", "last-harvest", "--bin", "last-harvest-sandbox", "-p", "last-harvest-pack"], cwd=ROOT, check=True)
    meta = subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT, check=True, capture_output=True, text=True)
    bin_dir = Path(json.loads(meta.stdout)["target_directory"]) / "release"
    sandbox = bin_dir / "last-harvest-sandbox"
    mods = a.out / "mods"
    mods.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(bin_dir / "last-harvest-pack"), "mod", str(ROOT / "tools/matrix/pressure"), str(mods / "pressure.pak")], check=True, stdout=subprocess.DEVNULL)
    spawn = "[spawn]" in (ROOT / "crates/game/tuning.txt").read_text()
    weapons, passives, top, _ = powerups()
    for tier, builds, seeds, pressure, extra in tiers(weapons, passives, top, a.random):
        if a.tiers and not any(tier.startswith(t) for t in a.tiers):
            continue
        out = a.out / f"{tier}.csv"
        if out.exists():
            print(f"{tier}: kept from an earlier run")
            continue
        if pressure and not spawn:
            print(f"{tier}: skipped, tuning.txt has no [spawn] section for the pressure pack")
            continue
        run(sandbox, out, tier, builds, seeds, (*player, *extra, *(("--mods", str(mods)) if pressure else ())))
    print(f"done; tools/matrix/report.py --out {a.out} makes the report", flush=True)


if __name__ == "__main__":
    main()

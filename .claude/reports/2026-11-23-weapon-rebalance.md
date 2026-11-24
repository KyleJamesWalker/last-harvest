# Weapon rebalance (ROADMAP §24), 2026-11-23

**Bottom line.** Three weapons are dead under every test: the shotgun, the hunting rifle and the post maul. Each got a data-only buff that leaves it below the median weapon, and those are the only game data changes on this branch. No passive is clearly dead; several can't be judged by a scripted player. Nothing strong was touched: the pistol, the four crowd-killers (flare, molotov, tacks, scarecrow), the knife and the dog are flagged below with option packs. For #204's bigger start pool, `[levelup] held_weight 2.5` restores the first-night dawn rate (34% → 67%, against 64% today) and keeps the variety #204 buys; it is a pack here, not applied.

| Change | Status | Level-8 useful DPS, circle / avoid | Level-8 first fall, circle / avoid (s) |
|---|---|---|---|
| Shotgun: cooldown 1.5..1.0 → 1.2..0.7, range 90 → 110, pierce 1 → 1 1 1 2 2 2 3 3 | applied (c65d9d5) | 33 → 73 / 41 → 100 | 72 → 83 / 105 → 166 |
| Hunting rifle: cooldown 2.0..1.2 → 1.5..0.75 | applied (ada8f55) | 35 → 45 / 36 → 52 | 43 → 47 / 67 → 69 |
| Post maul: cooldown 2.4..1.6 → 1.8..1.0, crack width 10..14 → 16..24, second crack at level 3 | applied (573d6a2) | 55 → 88 / 29 → 67 | 46 → 59 / 75 → 101 |
| `held_weight` 1 → 2.5 (with #204's pool) | option, pack | first-night dawn 34% → 67% | |
| Holster, pitchfork: quicker | option, packs | 66 → 98, 78 → 89 | |
| Shotgun, rifle, maul: a stronger step each | option, packs | 79, 58, 100 | |
| Flare, scarecrow: weaker low levels, same top | option, packs | level 1: 154 → 96, 134 → 100 | |
| Pistol: `flat` and `pb30_out40` (§26), rebuilt on this data | option, packs | 196 → 162, 139 | 318 → 277, 257 |

For scale, the median weapon does about 145 useful DPS at level 8 on the circle walk; the knife does 147 and the pistol 196.

## Method

- `last-harvest-sandbox` built from this branch (`dev-0c176e5` for the baseline, `dev-573d6a2` after the buffs), normal difficulty, 72×54 generated farm. About 24,000 nights. Cells from the level-4 pairs, pressure pairs, co-op and passive batches were replayed at the end from a second build and matched exactly.
- Every variant ran as a mod pack (`--mods`) on the same binary and the same seeds as its baseline, so differences are paired. After each buff was written into `powerups.txt` the rebuilt binary reproduced the pack's numbers to the last digit.
- **Useful DPS** is the weapon's own damage capped at each zombie's remaining health, per second held. **First fall** is the undying player's first time at 0 HP, in seconds; a night with no fall counts 600. **Kills** are the whole night's, grenades included.

| Run | Builds | Player | Seeds |
|---|---|---|---|
| Singles, circle | every weapon at levels 1..8 and its breaker | circle walk, turnaround every 6 s, grenade every 12 s, no picks, undying | 12 |
| Singles, avoid | levels 1, 4, 8, breaker | `--walk avoid --no-throw` | 12 |
| Singles, no grenades | levels 4, 8 | circle, `--no-throw` | 12 |
| Shotgun first | `pistol,knife --pick first:shotgun --locked --mortal --walk avoid`, before and after | Normal | 300 |
| Pairs, level 4 | all 171 pairs and each weapon alone | circle | 10 |
| Pairs, top, pressure | all 171 pairs at level 8 | circle, `tools/matrix/pressure` (spawns to 25/s, cap 600) | 10 |
| Co-op | each weapon at 4 and 8; pistol:4 + knife:4 + each weapon at 4 | `--players 2`, circle | 10 |
| Kit + weapon, solo | pistol:4 + knife:4 + each weapon at 4 | circle | 12 |
| Passives, kit | pistol:4 + knife:4 + each passive at top | circle undying; and `--mortal --walk avoid --pick held` | 30 |
| Passives, pressure | each weapon at top × each passive at top (323 builds) | circle, pressure | 6 |
| Picker | `pistol,knife`, every powerup unlocked | `--mortal --walk avoid`, `--pick random` and `--pick held` | 400 |
| Picker, one powerup first | `--pick first:ID` for each of the 36 | `--mortal --walk avoid` | 80 |
| First night | `pistol,knife --locked --pick held --mortal --walk avoid`, each `held_weight` and pool | Normal and Easy | 300 |

**What counts as dead** (fixed before looking at candidates): bottom five on level-8 useful DPS on both walks, no better at level 4, a small share of any pair's kills, and the same picture with two players. A weapon that is weak on one walk only (the lantern, every behind weapon on the circle) does not qualify. A buff that would carry a weapon past the median is a judgment call, not a clear buff.

## Every weapon (before the buffs)

| Weapon | Useful DPS, level 1..8 then breaker (circle) | Overkill L8 | First fall L4 / L8 / brk, circle (s) | Avoid walk: DPS L4 / L8, first fall L8 (s) | 2 players L8: kills, first fall | As a level-4 partner: Δkills, Δfall, pairs it hurts | Pressure L8: kills, as partner Δkills | Random picker takes it | `first:` Δfall (s) |
|---|---|---|---|---|---|---|---|---|---|
| pistol | 16 32 44 63 118 130 171 196 221 | 19% | 165 / 318 / 471 | 66 / 211, 477 | 4775, 326 | +565, +123, 0 | 5495, +2052 | 20% | +2 |
| shotgun | 11 13 15 18 19 25 25 33 45 | 68% | 57 / 72 / 82 | 21 / 41, 105 | 1349, 60 | +178, +21, 2 | 1654, +361 | 16% | -9 |
| rifle | 11 12 16 20 21 22 25 35 47 | 76% | 42 / 43 / 47 | 16 / 36, 67 | 1525, 35 | +154, +7, 3 | 1762, +290 | 15% | -15 |
| lantern | 13 24 29 33 36 46 60 72 93 | 17% | 43 / 60 / 65 | 106 / 101, 109 | 2692, 46 | +229, +11, 2 | 3626, +923 | 13% | -5 |
| burner | 21 26 41 43 62 63 70 93 90 | 8% | 43 / 44 / 52 | 52 / 102, 61 | 2718, 35 | +356, +2, 0 | 3630, +1089 | 14% | -11 |
| pitchfork | 14 21 19 38 40 37 54 78 80 | 63% | 48 / 48 / 67 | 53 / 49, 73 | 2328, 36 | +278, +4, 2 | 3313, +865 | 16% | -17 |
| maul | 14 14 15 30 38 44 46 55 90 | 78% | 43 / 46 / 59 | 26 / 29, 75 | 2226, 34 | +225, +3, 2 | 3107, +809 | 15% | -19 |
| knife | 54 69 71 85 116 129 141 147 178 | 44% | 83 / 142 / 258 | 155 / 210, 254 | 4838, 124 | +779, +34, 0 | 7551, +2695 | 18% | -5 |
| scythe | 49 60 65 81 93 121 135 145 164 | 58% | 53 / 83 / 130 | 124 / 218, 136 | 4440, 69 | +732, +12, 0 | 6337, +2385 | 17% | -6 |
| crows | 18 27 44 56 61 81 88 101 117 | 19% | 42 / 50 / 51 | 68 / 138, 102 | 3496, 35 | +400, +5, 3 | 4703, +1473 | 18% | -9 |
| zapper | 13 16 26 27 42 53 75 104 159 | 32% | 61 / 165 / 251 | 28 / 118, 228 | 3074, 133 | +319, +28, 0 | 3557, +1116 | 16% | +7 |
| dog | 17 37 41 64 96 117 162 182 202 | 42% | 127 / 229 / 309 | 64 / 201, 274 | 4589, 223 | +674, +81, 0 | 5715, +2215 | 17% | +13 |
| tacks | 65 71 159 165 196 199 207 210 215 | 15% | 50 / 60 / 75 | 204 / 220, 143 | 5886, 45 | +1412, +7, 0 | 8906, +3763 | 15% | -2 |
| gas | 71 83 130 120 148 154 174 184 200 | 9% | 45 / 51 / 54 | 149 / 210, 100 | 5274, 36 | +1014, +4, 0 | 8249, +3361 | 13% | -7 |
| holster | 23 29 31 36 40 53 56 66 90 | 65% | 51 / 59 / 60 | 38 / 73, 108 | 2244, 50 | +366, +12, 0 | 2520, +758 | 16% | +1 |
| scarecrow | 134 131 144 154 184 183 201 204 212 | 77% | 54 / 68 / 81 | 200 / 219, 98 | 5781, 51 | +1380, +9, 0 | 8562, +3596 | 15% | -2 |
| molotov | 100 110 138 180 195 198 207 212 220 | 9% | 47 / 80 / 103 | 200 / 221, 133 | 5850, 51 | +1578, +7, 0 | 8869, +3756 | 14% | -2 |
| flare | 154 168 183 192 200 205 209 214 219 | 6% | 45 / 55 / 81 | 213 / 218, 116 | 5836, 42 | +1606, +10, 0 | 8845, +3731 | 14% | +11 |
| drone | 15 20 25 34 122 145 162 194 219 | 24% | 71 / 117 / 130 | 33 / 198, 171 | 5129, 92 | +381, +29, 1 | 5568, +2294 | 16% | -4 |

"As a level-4 partner" is the mean change it makes to each of the other 18 weapons' night (kills and first fall) when added at level 4, and how many of those pairs kill less than the other weapon alone. "Random picker takes it" is the share of 400 mortal nights in which a random pick took its card at least once (every powerup unlocked). "`first:`" is the change in survival when the picker takes that powerup first whenever it is offered, against the plain held picker (199 ±9 s); its standard error is about ±12 s and no powerup moves it by more than 20 s, so it separates nothing.

Without the timed grenade the weak weapons' own numbers barely move (level-8 useful DPS: shotgun 30, rifle 30, maul 55), but their nights lose a third of their kills (shotgun:8 971 → 607): the grenade was doing much of the work.

The three buffed weapons now, same seeds: shotgun 14 18 19 31 35 44 61 73 130; rifle 14 14 20 25 30 30 38 45 79; maul 21 27 44 48 61 56 79 88 159.

## Every passive

| Passive (at top) | With pistol:4 + knife:4, undying circle: kills, first fall (s) | Same kit, mortal, avoid, held picker: first fall (s), dawn | With each weapon at top under pressure: mean Δkills, mean Δfall (s), best partner | Random picker takes it | `first:` Δfall (s) |
|---|---|---|---|---|---|
| (none) | 2305, 193 ±4 | 472 ±23, 33% | | | |
| hollow_points | 2589 (+12%), 219 ±7 | 539 ±21, 73% | +10%, +7, burner +41% | 18% | -17 |
| quick_hands | 2410 (+5%), 236 ±7 | 553 ±21, 77% | +14%, +15, burner +38% | 19% | +4 |
| running_shoes | 2303 (-0%), 175 ±5 | 433 ±25, 30% | -11%, -7, scythe +6% | 15% | -11 |
| canned_peaches | 2292 (-1%), 224 ±5 | 550 ±17, 67% | +2%, +21, pitchfork +18% | 15% | -11 |
| deep_pockets | 2314 (+0%), 198 ±5 | 560 ±17, 83% | +4%, +0, burner +29% | 19% | -12 |
| work_jacket | 2289 (-1%), 203 ±5 | 512 ±19, 47% | +3%, +12, pitchfork +17% | 18% | -14 |
| garden_tea | 2287 (-1%), 214 ±5 | 527 ±19, 60% | +2%, +9, pitchfork +24% | 19% | -17 |
| sharp_eye | 2380 (+3%), 205 ±6 | 533 ±24, 70% | +6%, +3, pitchfork +27% | 18% | -14 |
| bigger_blast | 2724 (+18%), 223 ±7 | 459 ±25, 43% | +28%, +9, shotgun +94% | 17% | -16 |
| long_barrel | 2338 (+1%), 204 ±6 | 558 ±16, 67% | +10%, +4, burner +54% | 16% | -20 |
| lucky_penny | 2305 (+0%), 193 ±4 | 536 ±18, 53% | +2%, +0, pitchfork +15% | 15% | -19 |
| pawn_shop | 2383 (+3%), 197 ±4 | 491 ±26, 47% | +3%, +2, burner +15% | 17% | -3 |
| heavy_rounds | 2686 (+17%), 239 ±8 | 552 ±14, 60% | +13%, +4, holster +65% | 15% | -3 |
| brighter_lantern | 2336 (+1%), 184 ±5 | 494 ±21, 43% | +3%, +3, lantern +30% | 14% | -11 |
| vaccine | 2302 (-0%), 247 ±6 | 550 ±16, 70% | +2%, +29, pitchfork +28% | 14% | +4 |
| grenade_belt | 2338 (+1%), 188 ±7 | 527 ±19, 53% | +9%, -2, pitchfork +34% | 21% | -18 |
| fuel_drums | 2565 (+11%), 200 ±6 | 393 ±27, 27% | +30%, +2, shotgun +109% | 18% | -18 |

The kit alone reaches dawn in 33% of the mortal nights, so most passives at top help a held-picking player. The pressure columns are the mean over all 19 weapons of adding the passive at top to that weapon at top.

- **Pulling weight everywhere:** quick hands, hollow points, heavy rounds, bigger blast, canned peaches, vaccine, and grave pull for a player who takes cards (the best dawn rate, 83%, through faster levels).
- **Not measurable with this player, not dead:** running shoes (the script doesn't use speed to escape, and kills drop 11% under pressure, perhaps because the walker leaves its targets behind), rabbit's foot and blood price (they need a player who takes cards; with the held picker they are mid-table), brighter lantern, grenade belt (the timer throws one grenade every 12 s whatever the belt holds).
- **Worth a look by hand:** fuel drums. It adds 30% kills under pressure but lowers the mortal walker's survival (393 ±27 s against 472 ±23, dawn 27% against 33%); the walker may be standing in its own bigger barrel blasts. Not changed.

No passive was changed.

## Outliers

**Weak (dead by the rule): shotgun, rifle, maul.** Level-8 useful DPS 33, 35 and 55 on the circle and 41, 36 and 29 on the avoid walk, 68–78% overkill, 6–9% of the kills when added to pistol + knife, and with two players adding the shotgun or the maul to pistol + knife lowered the team's kills (4,212 → 4,156 and 3,921).

**Weak, but not dead:** the holster (66 / 73 DPS, yet it helps an avoiding player survive), the pitchfork (78 / 49), the lantern (72 on the circle but 101 on the avoid walk, where its slow counts) and the burner (93 / 102, no survival). Options below for the first two; the lantern and burner are left as support weapons.

**Strong, flagged and left alone:**
- **Pistol.** Still the survival weapon: first fall 318 s alone at level 8 against 142 for the knife, and +123 s to the average level-4 partner. §26 is still open; its two leading options are rebuilt below on today's data.
- **Flare, molotov, tacks, scarecrow.** All four sit near the kill ceiling from level 4 (2,700–3,100 of about 3,250 kills a night) and the flare and scarecrow nearly from level 1 (154 and 134 useful DPS at level 1, more than most weapons at level 6). Their upper levels add almost nothing (flare 1.4× from level 1 to 8), so the first card is the whole weapon. None keeps a walking player alive (first fall 55–80 s at level 8).
- **Knife and dog.** The best all-round pairs after the pistol; the dog is fixed (182 DPS at level 8, 229 s first fall) and is now the second-best survival weapon.
- **Zapper.** Mid-table DPS but the third-best first fall (165 s at level 8, 251 s at the breaker).
- **Drone.** 34 DPS at level 4, 122 at level 5 when its second drone and pierce arrive: the steepest single step in the game.
- **Breakers.** The biggest jumps past level 8 on first fall: pistol 318 → 471 s, knife 142 → 258 s, zapper 165 → 251 s.

## Proposals

### Applied: clear buffs to dead weapons

Each is one commit with the numbers in its body. All three attack overkill through rate, reach and coverage rather than damage.

**Shotgun** (c65d9d5)

| Variant | Useful DPS L1 / L4 / L8 / brk (circle) | First fall L4 / L8 / brk (s) | Night kills L8 | Useful DPS (avoid) | First fall (avoid, s) | With pistol:4 + knife:4: kills, first fall (s) |
|---|---|---|---|---|---|---|
| before | 11 / 18 / 33 / 45 | 57 / 72 / 82 | 971 | 12 / 21 / 41 / 58 | 85 / 105 / 132 | 2496, 240 ±12 |
| **applied** | 14 / 31 / 73 / 130 | 69 / 83 / 102 | 1532 | 16 / 43 / 100 / 174 | 96 / 166 / 229 | 2484, 235 ±10 |
| cooldown only | 13 / 23 / 43 / 64 | 65 / 84 / 87 | 1110 | 17 / 28 / 59 / 88 | 88 / 120 / 150 | 2604, 226 ±12 |
| `shotgun-strong` pack | 21 / 35 / 79 / 126 | 79 / 97 / 110 | 1611 | 27 / 49 / 108 / 179 | 107 / 213 / 381 | 2521, 265 ±11 |

With two players its level-8 kills go 1,349 → 2619; under pressure alone 1,654 → 2830, with the pistol 5,829 → 6611 (first fall 238 → 287 s). It is a start weapon, so the first night was checked two ways on a fresh save (300 seeds each). The held picker, which mostly stacks the pistol and knife and ends holding the shotgun in 28% of nights, is unchanged: dawn 64% → 63%. A picker that takes the shotgun whenever it is offered (`--pick first:shotgun`, holding it in 99% of nights) goes from dawn 6% to 17% and survival 229 ±7 → 307 ±9 s. So the buff helps a shotgun run a good deal, and a shotgun-first run is still far behind stacking the pistol and knife (64%). ROADMAP §20 calls pistol + shotgun a strong first-run pair in play; this needs the owner's confirmation on the device.

**Hunting rifle** (ada8f55)

| Variant | Useful DPS L1 / L4 / L8 / brk (circle) | First fall L4 / L8 / brk (s) | Night kills L8 | Useful DPS (avoid) | First fall (avoid, s) | With pistol:4 + knife:4: kills, first fall (s) |
|---|---|---|---|---|---|---|
| before | 11 / 20 / 35 / 47 | 42 / 43 / 47 | 892 | 12 / 16 / 36 / 37 | 69 / 67 / 72 | 2514, 207 ±6 |
| **applied** | 14 / 25 / 45 / 79 | 43 / 47 / 54 | 1088 | 16 / 25 / 52 / 65 | 68 / 69 / 78 | 2377, 227 ±9 |
| `rifle-fast` pack | 14 / 32 / 58 / 84 | 44 / 52 / 57 | 1268 | 14 / 24 / 60 / 95 | 61 / 77 / 77 | 2254, 220 ±7 |
| applied + `spread 24` | 11 / 16 / 42 / 71 | 44 / 48 / 55 | 1042 | 14 / 18 / 57 / 52 | 71 / 73 / 87 | 2406, 206 ±6 |

The rifle stays near the bottom. Its real limit is that it fires only straight down the heading at a zombie within 5° of it, which no number fixes; a wider trigger (`spread 24`) fires more often and misses more. Cooldown was kept at 0.75 s or slower so the bolt sound (0.5 s after the shot) still fits. One cost shows in three places: with a knife beside it, the faster rifle lowers the pair's kills about 5–9% (pistol:4 + knife:4 + rifle:4 2,514 → 2,377 solo and 4,487 → 4094 with two players; knife:8 + rifle:8 under pressure 7,371 → 6698) while first fall goes up (207 → 227 s, 167 → 185 s): its knockback shoves zombies out of the knife's ring. A reshape (aim at the nearest zombie in view, or a wider round) is the real fix and is a design call.

**Post maul** (573d6a2)

| Variant | Useful DPS L1 / L4 / L8 / brk (circle) | First fall L4 / L8 / brk (s) | Night kills L8 | Useful DPS (avoid) | First fall (avoid, s) | With pistol:4 + knife:4: kills, first fall (s) |
|---|---|---|---|---|---|---|
| before | 14 / 30 / 55 / 90 | 43 / 46 / 59 | 1285 | 28 / 26 / 29 / 62 | 64 / 75 / 96 | 2460, 199 ±7 |
| cooldown only | 16 / 42 / 67 / 134 | 46 / 51 / 72 | 1411 | 28 / 33 / 51 / 110 | 64 / 79 / 106 | 2391, 201 ±6 |
| **applied** | 21 / 48 / 88 / 159 | 45 / 59 / 88 | 1711 | 25 / 24 / 67 / 140 | 70 / 101 / 177 | 2472, 199 ±5 |
| `maul-fast` pack | 26 / 53 / 100 / 160 | 52 / 66 / 88 | 1896 | 32 / 45 / 90 / 140 | 77 / 101 / 177 | 2526, 213 ±10 |

With two players, pistol:4 + knife:4 + maul:4 goes from 3,921 kills (below the kit alone, 4,212) to 4340; under pressure alone 3,107 → 4242.

### Options: judgment calls, not applied

Each has a pack in `.claude/reports/2026-11-23-weapon-rebalance/`, built from this branch's data (573d6a2).

**1. `held_weight` for #204's start pool** (`held-weight-2.5.pak`, `held-weight-2.pak`)

First night on a fresh save, Normal, held picker, mortal, 300 seeds. "#204 pool" is today's data with crows, tacks and molotov set to `unlock start`, as a pack; it reproduces #204's 64% → 34%.

| Setup | Dawn reached | Survival (s) | Distinct weapon sets | Highest powerup level at the end |
|---|---|---|---|---|
| main, `held_weight 1` | 64% ±3 | 511 ±8 | 2 | 7.9 |
| this branch (buffed shotgun), `held_weight 1` | 63% ±3 | 511 ±8 | 2 | 7.9 |
| main, `held_weight 2` | 71% ±3 | 532 ±8 | 2 | 8.1 |
| #204 pool, `held_weight 1` | 34% ±3 | 394 ±11 | 14 | 6.7 |
| #204 pool, 1.5 | 49% ±3 | 453 ±10 | 14 | 7.4 |
| #204 pool, 2 | 59% ±3 | 495 ±9 | 12 | 7.8 |
| #204 pool, 2.5 | 67% ±3 | 513 ±8 | 14 | 7.9 |
| #204 pool, 3 | 72% ±3 | 533 ±8 | 12 | 8.0 |
| #204 pool, 4 | 73% ±3 | 531 ±8 | 12 | 8.0 |
| #204 pool, 1, `reroll 4` `banish 4` | 58% ±3 | 496 ±9 | 13 | 7.8 |

| Setup | Dawn reached | Survival (s) | Distinct weapon sets | Highest powerup level at the end |
|---|---|---|---|---|
| Easy: main, 1 | 99% ±1 | 598 ±1 | 2 | 8.2 |
| Easy: #204 pool, 1 | 90% ±2 | 570 ±5 | 16 | 7.9 |
| Easy: #204 pool, 2 | 98% ±1 | 596 ±2 | 14 | 8.2 |
| Easy: #204 pool, 3 | 99% ±1 | 599 ±1 | 13 | 8.2 |
| Normal, random picker: #204 pool, 1 | 0% ±0 | 169 ±3 | 16 | 2.7 |
| Normal, random picker: #204 pool, 3 | 1% ±1 | 189 ±5 | 16 | 3.5 |

Recommendation: **`held_weight 2.5` if #204 merges** (67% against today's 64%; 2 gives 59%). The number of distinct weapon sets stays at 12–14 of #204's 14 and the random picker still ends on 16 sets, so the variety is kept; what changes is that a player who wants to stack a weapon can. Above 3 nothing more is gained. More rerolls and banishes (4 each) do about as much as weight 2 (58%). On Easy any value from 2 restores 98–99%.

A side finding: with every powerup unlocked the same held picker almost never sees dawn, because 36 powerups dilute the draw far more than #204's 11.

| Setup | Dawn reached | Survival (s) | Distinct weapon sets | Highest powerup level at the end |
|---|---|---|---|---|
| every powerup unlocked, 1 | 1% ±0 | 202 ±4 | 253 | 3.2 |
| every powerup unlocked, 2 | 7% ±1 | 258 ±7 | 202 | 4.5 |
| every powerup unlocked, 3 | 15% ±2 | 313 ±8 | 174 | 5.5 |
| every powerup unlocked, 4 | 24% ±2 | 364 ±9 | 161 | 6.2 |

So a veteran save plays a much harder game than a fresh one at `held_weight 1`, and a higher weight helps there too (2.5 was not run on the full pool). A weight that grows with the pool would be a code change; noted for §20.

**2. Holster and pitchfork** (`holster-quick.pak`, `pitchfork-quick.pak`)

| Variant | Useful DPS L1 / L4 / L8 / brk (circle) | First fall L4 / L8 / brk (s) | Night kills L8 | Useful DPS (avoid) | First fall (avoid, s) | With pistol:4 + knife:4: kills, first fall (s) |
|---|---|---|---|---|---|---|
| before | 23 / 36 / 66 / 90 | 51 / 59 / 60 | 1528 | 23 / 38 / 73 / 100 | 82 / 108 / 125 | 2397, 202 ±7 |
| `holster-quick` pack | 30 / 49 / 98 / 141 | 56 / 67 / 79 | 1905 | 30 / 51 / 109 / 167 | 98 / 138 / 167 | 2500, 194 ±4 |

| Variant | Useful DPS L1 / L4 / L8 / brk (circle) | First fall L4 / L8 / brk (s) | Night kills L8 | Useful DPS (avoid) | First fall (avoid, s) | With pistol:4 + knife:4: kills, first fall (s) |
|---|---|---|---|---|---|---|
| before | 14 / 38 / 78 / 80 | 48 / 48 / 67 | 1642 | 25 / 53 / 49 / 68 | 63 / 73 / 84 | 2420, 188 ±4 |
| `pitchfork-quick` pack | 22 / 55 / 89 / 119 | 49 / 61 / 71 | 1843 | 23 / 34 / 56 / 96 | 74 / 89 / 104 | 2360, 202 ±7 |

The holster option (cooldown 1.6..1.0 → 1.3..0.7, range 80 → 100) is the back-shotgun's share of the shotgun buff. The pitchfork option (cooldown 1.2..0.8 → 1.0..0.6, width 8 → 12) is mixed on the avoid walk at level 4. Neither weapon is dead, so neither is applied.

**3. A stronger step for the three buffed weapons** (`shotgun-strong.pak`, `rifle-fast.pak`, `maul-fast.pak`)

In the tables above. `shotgun-strong` (cooldown 1.0..0.55, range 120, pierce 2, spread 50) makes the breaker shotgun a real avoid-walk build (first fall 381 s). `rifle-fast` (cooldown 1.2..0.5) overlaps its own bolt sound at the top levels.

**4. Flare and scarecrow: a ramp instead of a level-1 peak** (`flare-ramp.pak`, `scarecrow-ramp.pak`)

| Variant | Useful DPS L1 / L4 / L8 / brk (circle) | First fall L4 / L8 / brk (s) | Night kills L8 | Useful DPS (avoid) | First fall (avoid, s) | With pistol:4 + knife:4: kills, first fall (s) |
|---|---|---|---|---|---|---|
| today | 154 / 192 / 214 / 219 | 45 / 55 / 81 | 3251 | 169 / 213 / 218 / 223 | 86 / 116 / 168 |  |
| `flare-ramp` pack | 96 / 181 / 214 / 219 | 48 / 55 / 81 | 3251 | 110 / 195 / 218 / 223 | 77 / 116 / 195 |  |

| Variant | Useful DPS L1 / L4 / L8 / brk (circle) | First fall L4 / L8 / brk (s) | Night kills L8 | Useful DPS (avoid) | First fall (avoid, s) | With pistol:4 + knife:4: kills, first fall (s) |
|---|---|---|---|---|---|---|
| today | 134 / 154 / 204 / 212 | 54 / 68 / 81 | 3205 | 151 / 200 / 219 / 223 | 88 / 98 / 89 |  |
| `scarecrow-ramp` pack | 100 / 147 / 204 / 212 | 53 / 68 / 81 | 3205 | 107 / 194 / 219 / 223 | 93 / 98 / 89 |  |

Flare: cooldown 9..5 → 14 12 10 9 8 7 6 5, area 40..60 → 30..60. Scarecrow: cooldown 12..5 → 16 14 12 10 8 7 6 5. Both keep level 8 and the breaker exactly and give the levels something to add. This is a nerf to two strong finds, offered only because their level-ups are close to empty today; §27 says the scarecrow feels great as it is.

**5. Pistol** (`pistol-flat.pak`, `pistol-pb30_out40.pak`)

| Variant | Useful DPS L1 / L4 / L8 / brk (circle) | First fall L4 / L8 / brk (s) | Night kills L8 | Useful DPS (avoid) | First fall (avoid, s) | With pistol:4 + knife:4: kills, first fall (s) |
|---|---|---|---|---|---|---|
| today | 16 / 63 / 196 / 221 | 165 / 318 / 471 | 3083 | 17 / 66 / 211 / 232 | 195 / 477 / 585 |  |
| `pistol-flat` pack | 16 / 54 / 162 / 216 | 145 / 277 / 397 | 2719 | 17 / 56 / 178 / 229 | 178 / 413 / 578 |  |
| `pistol-pb30_out40` pack | 16 / 48 / 139 / 203 | 137 / 257 / 401 | 2464 | 17 / 54 / 181 / 220 | 172 / 391 / 580 |  |

The 2026-11-22 report's two leading options, rebuilt because its packs carry a whole older `powerups.txt` and would undo the dog's `fetches` and `charge` and tonight's buffs.

### Loading a pack

Copy one `.pak` into the game's `data/mods/` folder on the device, one `powerups.txt` pack at a time (each replaces the whole file; the two `held-weight` packs hold only a `tuning.txt` line and combine with any other). In the sandbox: `--mods <folder holding the pack>`. Every pack but the `held-weight` ones goes stale when `powerups.txt` changes on main (the nine new weapons in #201–#205 will do that), so rebuild then: copy `crates/game/powerups.txt` into a folder, make the edit named above, and run `cargo run -p last-harvest-pack --features packer -- mod <folder> name.pak`. The exact lines:

| Pack | Section | Lines |
|---|---|---|
| `holster-quick` | `[holster]` | `cooldown 1.3 1.2 1.15 1.1 1.0 0.9 0.8 0.7`; `range 100` |
| `pitchfork-quick` | `[pitchfork]` | `cooldown 1.0 0.95 0.9 0.85 0.8 0.75 0.7 0.6`; `area 12` |
| `shotgun-strong` | `[shotgun]` | `cooldown 1.0 0.95 0.9 0.85 0.8 0.7 0.65 0.55`; `range 120`; `pierce 2`; `spread 50` |
| `rifle-fast` | `[rifle]` | `cooldown 1.2 1.1 1.0 0.9 0.8 0.7 0.6 0.5` |
| `maul-fast` | `[maul]` | `cooldown 1.5 1.4 1.3 1.2 1.1 1.0 0.9 0.8` |
| `flare-ramp` | `[flare]` | `cooldown 14 12 10 9 8 7 6 5`; `area 30 34 38 42 46 50 56 60` |
| `scarecrow-ramp` | `[scarecrow]` | `cooldown 16 14 12 10 8 7 6 5` |
| `pistol-flat` | `[pistol]` | `count 1 2 3 3 4 4 5 5` |
| `pistol-pb30_out40` | `[pistol]`; `tuning.txt` | `outside 40`; `[weapons] point_blank 30` |
| `held-weight-2.5`, `held-weight-2` | `tuning.txt` | `[levelup] held_weight 2.5` (or `2`) |

## Notes and limits

- **The nine new weapons** (#201, #202, #203, #205) are not measured here. The Nail gun, Grain shovel and Varmint rifle land in the same ahead zone as the three buffed weapons and should be compared against the new numbers, not the old ones. Each new start or early unlock dilutes the draw further, which strengthens the case for a higher `held_weight`.
- **One scripted style per run.** Nobody aims. The circle walker flatters all-round and far weapons; the avoid walker flatters behind weapons and dies at about 60–110 s with most single weapons. A real player who faces the crowd gets more from the shotgun, rifle and maul than either walker.
- **The picker rows are weak evidence.** With every powerup unlocked the mortal walker dies near 200 s whatever it takes, and a random pick takes every powerup about equally (13–21%), so pick rate shows no dead card by itself.
- **Co-op DPS for burning weapons looks wrong.** With `--players 2` the first player's burner shows 27 useful DPS at level 8 (93 solo) and gas 47 (184 solo) while the team's kills rise, so burn damage is probably credited to another seat or to no weapon in the tally. Co-op rows here use kills and first fall only. Not investigated.
- First fall is censored at 600 s; only pistol breaker cells come near it.
- Raw results (JSON per night) were left out of the repo; every run is one sandbox command from the Method table.

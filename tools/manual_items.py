#!/usr/bin/env python3
"""Fill the manual's weapon, passive and zombie tables from the game's data files.

Usage: tools/manual_items.py [docs/guides/GAME_MANUAL.md]

Each `<!-- items: weapons -->`, `<!-- items: passives -->` or `<!-- items: enemies -->` line is followed by a table
read from crates/game/powerups.txt or enemies.txt, up to a `<!-- /items -->` line. Run it after changing either
file; --check exits 1 when the manual is out of date instead of writing it.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MARK = re.compile(r'^<!-- items: (\w+) -->$')
END = '<!-- /items -->'


def sections(path):
    """Each [id] section of a data file, in order, as its id and a dict of key to value."""
    out = []
    for raw in path.read_text().splitlines():
        line = raw.split('#', 1)[0].strip()
        if not line:
            continue
        if line.startswith('[') and line.endswith(']'):
            out.append((line[1:-1], {}))
        elif out:
            key, _, value = line.partition(' ')
            out[-1][1][key] = value.strip()
    return out


def unlock(fields):
    words = fields.get('unlock', 'start').split()
    kind, n = words[0], words[1] if len(words) > 1 else ''
    text = {
        'start': 'From the start',
        'kills': f'{n} kills over every run',
        'night': f'Survive {int(n) // 60}:{int(n) % 60:02d} of one night' if n else '',
        'level': f'Level {n} in one run',
        'nights': f'{n} night{"s" if n != "1" else ""} played',
        'dawns': f'{n} dawn{"s" if n != "1" else ""} survived',
    }.get(kind, ' '.join(words))
    extra = []
    if float(fields.get('after', '0') or 0) > 0:
        extra.append(f'from minute {fields["after"]}')
    if 'chest' in fields:
        extra.append("only in a leader's chest")
    return text + (', then found ' + ' and '.join(extra) if extra and kind != 'start' else '')


def cell(text):
    return text.replace('|', '\\|')


def powerups(kind):
    rows = [(i, f) for i, f in sections(ROOT / 'crates/game/powerups.txt') if f.get('kind') == kind]
    if kind == 'weapon':
        head = '| Weapon | Zone | Unlocks | Levels | On its card |\n|---|---|---|---|---|'
        body = [f"| {cell(f.get('name', i))} | {f.get('zone', '').capitalize()} | {unlock(f)} | {f.get('max', '1')} | {cell(f.get('blurb', ''))} |" for i, f in rows]
    else:
        head = '| Passive | Unlocks | Levels | On its card |\n|---|---|---|---|'
        body = [f"| {cell(f.get('name', i))} | {unlock(f)} | {f.get('max', '1')} | {cell(f.get('blurb', ''))} |" for i, f in rows]
    return '\n'.join([head, *body])


def enemies():
    head = '| Zombie | Health | Speed | Bite | Money | Comes from minute | What to expect |\n|---|---|---|---|---|---|---|'
    body = []
    for i, f in sections(ROOT / 'crates/game/enemies.txt'):
        body.append(f"| {cell(f.get('name', i))} | {f.get('hp', '')} | {f.get('speed', '')} | {f.get('damage', '')} | {f.get('xp', '')} | {f.get('after', '0')} | {cell(f.get('about', ''))} |")
    return '\n'.join([head, *body])


TABLES = {'weapons': lambda: powerups('weapon'), 'passives': lambda: powerups('passive'), 'enemies': enemies}


def fill(text):
    lines, out, i = text.splitlines(), [], 0
    while i < len(lines):
        out.append(lines[i])
        m = MARK.match(lines[i])
        i += 1
        if not m:
            continue
        if m[1] not in TABLES:
            sys.exit(f'no table called {m[1]}; there are {", ".join(TABLES)}')
        out += ['', TABLES[m[1]](), '', END]
        rest = lines[i:]
        if END in rest and not any(MARK.match(l) or l.startswith('#') for l in rest[: rest.index(END)]):
            i += rest.index(END) + 1
    return '\n'.join(out) + '\n'


def main():
    args = [a for a in sys.argv[1:] if a != '--check']
    path = Path(args[0]) if args else ROOT / 'docs/guides/GAME_MANUAL.md'
    text = path.read_text()
    filled = fill(text)
    if '--check' in sys.argv:
        sys.exit(0 if filled == text else f'{path} is out of date: run tools/manual_items.py')
    path.write_text(filled)


if __name__ == '__main__':
    main()

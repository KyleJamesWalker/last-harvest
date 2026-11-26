#!/usr/bin/env bash
# Usage: scripts/prune-target.sh [hours]. Deletes the debug .o files and incremental caches older than hours (6)
# from the shared target/; macOS debug builds keep their debug info in those .o files and cargo never removes them.
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/lib/target.sh
hours=${1:-6}
mins=$((hours * 60))
before=$(du -sk "$bin" | cut -f1)
objs=0
for deps in "$bin"/debug/deps "$bin"/*/debug/deps; do
  [ -d "$deps" ] || continue
  n=$(find "$deps" -maxdepth 1 -name '*.o' -mmin +"$mins" -print -delete | wc -l)
  objs=$((objs + n))
done
caches=0
for inc in "$bin"/debug/incremental "$bin"/*/debug/incremental; do
  [ -d "$inc" ] || continue
  for dir in "$inc"/*/; do
    [ -n "$(find "$dir" -mmin -"$mins" -print -quit)" ] && continue
    rm -rf "$dir"
    caches=$((caches + 1))
  done
done
after=$(du -sk "$bin" | cut -f1)
echo "removed $objs object files and $caches incremental caches older than ${hours}h: $(( (before - after) / 1024 )) MB freed, $bin is $((after / 1024)) MB"

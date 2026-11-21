# shellcheck shell=bash
# Sourced by the build scripts: $bin is where cargo writes binaries, which a worktree may share with the main
# checkout (scripts/share-target.sh). Each checkout's own outputs (packs, packages, web builds) stay in its target/.
bin=$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')

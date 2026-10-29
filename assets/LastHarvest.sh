#!/bin/bash
# Stock menu launcher: the frontend exits before this runs and restarts after.
# Before the game starts it fetches a missing data/game_data.pak and updates the game from the latest release.

repo=KyleJamesWalker/last-harvest
api=${LAST_HARVEST_API:-https://api.github.com/repos/$repo/releases}
downloads=${LAST_HARVEST_DOWNLOADS:-https://github.com/$repo/releases/download}
pak=data/game_data.pak
work=.update

say() { echo "update: $*"; }

# This device's build in a release, as named in its tarball.
platform() {
    [ -n "$LAST_HARVEST_PLATFORM" ] && { echo "$LAST_HARVEST_PLATFORM"; return; }
    case "$(uname -s)-$(uname -m)" in
        Linux-aarch64) echo aarch64-linux ;;
    esac
}

# fetch <url> <file> [api]: an API call gives up sooner than a download.
fetch() {
    if command -v curl >/dev/null; then
        if [ -n "$3" ]; then
            curl -fsSL --connect-timeout 4 --max-time 15 -o "$2" "$1"
        else
            curl -fsSL --connect-timeout 4 --speed-limit 1024 --speed-time 30 -o "$2" "$1"
        fi
    # The RG DS Plus has no curl, and its busybox wget can't do https.
    elif command -v python3 >/dev/null; then
        python3 - "$1" "$2" "$3" 2>/dev/null <<'EOF'
import shutil, sys, urllib.request
url, out, api = sys.argv[1:4]
with urllib.request.urlopen(url, timeout=5 if api else 30) as r, open(out, "wb") as f:
    shutil.copyfileobj(r, f)
EOF
    elif command -v wget >/dev/null; then
        wget -q -T "$([ -n "$3" ] && echo 5 || echo 30)" -t 1 -O "$2" "$1"
    else
        say "no curl or wget"
        return 1
    fi
}

sha256() {
    if command -v sha256sum >/dev/null; then
        sha256sum "$1"
    elif command -v shasum >/dev/null; then
        shasum -a 256 "$1"
    else
        openssl dgst -sha256 -r "$1"
    fi | { read -r sum _ && echo "$sum"; }
}

# Reads a release's JSON into lines of "tag <tag>" and "<asset> <sha256>", object by object, so key order and
# nesting don't matter.
index() {
    awk '
        { text = text $0 "\n" }
        END {
            n = length(text); depth = 0; key = ""
            for (i = 1; i <= n; i++) {
                c = substr(text, i, 1)
                if (c == "\"") {
                    s = ""
                    for (i++; i <= n; i++) {
                        c = substr(text, i, 1)
                        if (c == "\\") { s = s c substr(text, i + 1, 1); i++; continue }
                        if (c == "\"") break
                        s = s c
                    }
                    j = i + 1
                    while (substr(text, j, 1) ~ /[ \t\r\n]/) j++
                    if (substr(text, j, 1) == ":") { key = s; continue }
                    if (key == "name") name[depth] = s
                    if (key == "digest") digest[depth] = s
                    if (key == "tag_name" && depth == 1) print "tag " s
                    key = ""
                } else if (c == "{") {
                    depth++; name[depth] = ""; digest[depth] = ""; key = ""
                } else if (c == "}") {
                    if (name[depth] != "" && digest[depth] ~ /^sha256:[0-9a-f]+$/) print name[depth] " " substr(digest[depth], 8)
                    depth--
                } else if (c == "[" || c == ",") {
                    key = ""
                }
            }
        }' "$1"
}

# release <latest | tags/TAG>: the release's index in $work/index, or nothing when offline.
release() {
    rm -f "$work/release.json" "$work/index"
    if ! { fetch "$api/$1" "$work/release.json" api && index "$work/release.json" > "$work/index" && grep -q '^tag ' "$work/index"; }; then
        say "can't reach $api/$1"
        offline=1
        return 1
    fi
}

field() { awk -v k="$1" '$1 == k { print $2; exit }' "$work/index"; }

# get <url> <file> <sha256>: downloads beside the file, keeping it only when complete and as the release says.
get() {
    [ -n "$3" ] || { say "the release gives no sha256 for $1"; return 1; }
    rm -f "$2.part"
    if ! fetch "$1" "$2.part"; then
        say "couldn't download $1"
        rm -f "$2.part"
        return 1
    fi
    if [ "$(sha256 "$2.part")" != "$3" ]; then
        say "$1 doesn't match its sha256; keeping what's here"
        rm -f "$2.part"
        return 1
    fi
    mv -f "$2.part" "$2"
}

# newer <a> <b>: whether release a comes after release b.
newer() {
    local a=${1#v} b=${2#v} i x y
    local -a as bs
    IFS=. read -r -a as <<< "${a%%-*}"
    IFS=. read -r -a bs <<< "${b%%-*}"
    for i in 0 1 2; do
        x=${as[i]:-0} y=${bs[i]:-0}
        case "$x$y" in *[!0-9]*) return 1 ;; esac
        ((10#$x > 10#$y)) && return 0
        ((10#$x < 10#$y)) && return 1
    done
    [ "$a" = "${a%%-*}" ] && [ "$b" != "${b%%-*}" ]
}

# The pack the release has, unless the one here is the same.
sync_pack() {
    local want
    want=$(field game_data.pak)
    [ -n "$want" ] || { say "release $1 has no game_data.pak"; return 1; }
    [ -f "$pak" ] && [ "$(sha256 "$pak")" = "$want" ] && return 0
    pack_tried=1
    say "fetching release $1's game_data.pak"
    get "$downloads/$1/game_data.pak" "$pak" "$want" && say "game_data.pak is release $1's"
}

# Swaps in the release's binary, data README, launcher and icon from its tarball for this platform.
update_game() {
    local tag=$1 plat name dir
    plat=$(platform)
    [ -n "$plat" ] || { say "no build for $(uname -s) $(uname -m)"; return 1; }
    name="last-harvest-$tag-$plat.tar.gz"
    say "updating $version to $tag"
    get "$downloads/$tag/$name" "$work/$name" "$(field "$name")" || return 1
    dir="$work/files"
    rm -rf "$dir"
    if ! { mkdir -p "$dir" && tar -xzf "$work/$name" -C "$dir"; }; then
        say "couldn't unpack $name"
        return 1
    fi
    rm -f "$work/$name"
    if [ "$("$dir/last-harvest/last-harvest" --version 2>/dev/null)" != "$tag" ]; then
        say "the $tag binary doesn't run here; keeping $version"
        return 1
    fi
    mv -f "$dir/last-harvest/last-harvest" last-harvest || return 1
    [ -f "$dir/last-harvest/data/README.txt" ] && mv -f "$dir/last-harvest/data/README.txt" data/README.txt
    [ -f "$dir/Imgs/LastHarvest.png" ] && [ -d ../Imgs ] && mv -f "$dir/Imgs/LastHarvest.png" ../Imgs/LastHarvest.png
    # bash reads a running script as it goes, so a new launcher replaces this file rather than writing into it.
    if [ -f "$dir/LastHarvest.sh" ] && ! cmp -s "$dir/LastHarvest.sh" "$launcher"; then
        chmod +x "$dir/LastHarvest.sh" && mv -f "$dir/LastHarvest.sh" "$launcher" && relaunch=1
    fi
    version=$tag
    say "updated to $tag"
}

update() {
    if [ -e data/no-updates ]; then
        say "off: data/no-updates is here"
        return
    fi
    case "$version" in
        v[0-9]*) ;;
        *) say "$version isn't a release, so it isn't updated"; return ;;
    esac
    release latest || return
    local tag
    tag=$(field tag)
    case "$tag" in
        v[0-9]*) case "$tag" in *[!A-Za-z0-9.-]*) say "odd release tag $tag"; return ;; esac ;;
        *) say "odd release tag $tag"; return ;;
    esac
    if newer "$tag" "$version"; then
        update_game "$tag"
    fi
    [ "$version" = "$tag" ] && sync_pack "$tag"
}

# A release's game needs that release's pack.
fetch_missing_pack() {
    [ -f "$pak" ] && return
    case "$version" in v[0-9]*) ;; *) return ;; esac
    [ -z "$offline$pack_tried" ] || return
    [ -f "$work/index" ] && [ "$(field tag)" = "$version" ] || release "tags/$version" || return
    sync_pack "$version"
}

main() {
    launcher="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
    cd "$(dirname "$0")/last-harvest" || exit 1
    # shellcheck source=/dev/null
    [ -f /etc/profile.d/myenv.sh ] && . /etc/profile.d/myenv.sh
    if [ -n "$LAST_HARVEST_RELAUNCHED" ]; then
        exec >> log.txt 2>&1
    else
        exec > log.txt 2>&1
    fi
    mkdir -p "$work" data
    rm -f "$pak.part"
    version=$(./last-harvest --version 2>/dev/null)
    [ -n "$LAST_HARVEST_RELAUNCHED" ] || update
    fetch_missing_pack
    rm -rf "$work"
    if [ -n "$relaunch" ]; then
        say "starting the new launcher"
        LAST_HARVEST_RELAUNCHED=1 exec bash "$launcher"
    fi
    ./last-harvest
}

main "$@"
exit

#!/usr/bin/env bash
# Re-creates reference/ (third-party sources used to write docs/) at the commits the docs were written against.
# Usage: scripts/fetch-references.sh            # pinned commits
#        scripts/fetch-references.sh --latest   # default branches (then update the table in docs/README.md)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REF="$ROOT/reference"
LATEST="${1:-}"
mkdir -p "$REF"

# name | url | pinned commit
REPOS=(
  "GW2TacO|https://github.com/BoyC/GW2TacO.git|70871b99f0d535805de977c7624a6307080c54ff"
  "Pathing|https://github.com/blish-hud/Pathing.git|bd14f74ddf5a405e79d7e92c7933b883df10f623"
  "TmfLib|https://github.com/dlamkins/TmfLib.git|966b9de628b2ed8dc651bb49e6b08cc4fc312f25"
  "TaimiHUD|https://github.com/TaimiHUD/TaimiHUD.git|9045ace36505ae8650c87754099ae773177f4ae9"
  "Nexus-API|https://github.com/RaidcoreGG/Nexus-API.git|9b2c53df86c00db6495642bfcff2d0611bd957ef"
  "GW2-Compass|https://github.com/RaidcoreGG/GW2-Compass.git|d596f72f39f83b91ae825bb45f86c96c2d013185"
  "gw2-mumble-cpp|https://github.com/RaidcoreGG/gw2-mumble-cpp.git|621a7949a44c00173ed9f95f8ade423432037c9d"
  "gw2pathing-wiki|https://github.com/dlamkins/gw2pathing-wiki.git|c3383bef461c08e2f8ddb96d66ff7ae588534355"
  "gw2pathing-libdef|https://github.com/dlamkins/gw2pathing-libdef.git|b3a9a2eb013831556d3b68c4e180b641e5269d85"
)

for entry in "${REPOS[@]}"; do
  IFS='|' read -r name url sha <<<"$entry"
  dir="$REF/$name"
  if [[ -d "$dir/.git" ]]; then
    echo "== $name: exists, skipping (delete it to re-fetch)"
    continue
  fi
  echo "== $name"
  if [[ "$LATEST" == "--latest" ]]; then
    git clone -q --depth 1 "$url" "$dir"
  else
    git init -q "$dir"
    git -C "$dir" remote add origin "$url"
    git -C "$dir" fetch -q --depth 1 origin "$sha"
    git -C "$dir" checkout -q FETCH_HEAD
  fi
  echo "   $(git -C "$dir" rev-parse HEAD)"
done

# Two small files from Blish HUD core (the full repo is ~70 MB).
BLISH_SHA="54ec00e46905bbe33a71caf1770dda00b63f21c4"
[[ "$LATEST" == "--latest" ]] && BLISH_SHA="dev"
mkdir -p "$REF/blishhud-core"
for f in ColorUtil.cs WorldUtil.cs; do
  curl -sfL "https://raw.githubusercontent.com/blish-hud/Blish-HUD/$BLISH_SHA/Blish%20HUD/_Utils/$f" -o "$REF/blishhud-core/$f"
done
echo "== blishhud-core: ColorUtil.cs, WorldUtil.cs @ $BLISH_SHA"

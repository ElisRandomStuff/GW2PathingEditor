# Pathing Editor — Research Notes

Working notes for building a **Nexus addon that creates and edits GW2 marker packs (TacO / Blish HUD Pathing /
TaimiHUD format) in game** with an intuitive UI. They are written to be used *instead of* re-reading the reference
code. Each claim names its source file so it can be re-checked.

| Doc | Contents |
|-|-|
| [01-marker-pack-format.md](01-marker-pack-format.md) | Containers, XML structure, per-overlay parser rules, categories & inheritance, coordinates/units, GUIDs, `.trl` binary format, routes, value encoding, `bh-` prefix, state files, **safe writer profile** |
| [02-attribute-reference.md](02-attribute-reference.md) | Every attribute: element, type, default, TacO/Pathing/TaimiHUD support, quirks. Behaviour ids, filter values, script attrs, TacO editor field mapping |
| [03-lua-scripting.md](03-lua-scripting.md) | Pathing Lua runtime (NeoLua), script attributes, full global/type API, pack layout |
| [04-nexus-addon-api.md](04-nexus-addon-api.md) | Nexus API v6 (every member), old↔new names, render passes, ImGui 1.80, addon skeleton, MumbleLink layout & identity |
| [05-rendering-and-math.md](05-rendering-and-math.md) | Camera/projection, marker sizing & fading, trail meshes, minimap/world-map transforms, picking, TacO's move gizmo, occlusion options, constants |
| [06-existing-editors-and-workflows.md](06-existing-editors-and-workflows.md) | TacO's in-game editor in detail (hotkeys, saved bits, undo, recorder), community workflow pain points, derived requirements |
| [07-reference-code-map.md](07-reference-code-map.md) | Where each topic lives in the reference repos |

## Reference sources

Cloned into `reference/` (git-ignored, ~120 MB). Recreate with `scripts/fetch-references.sh`.

| Repo | Commit | What it is |
|-|-|-|
| [BoyC/GW2TacO](https://github.com/BoyC/GW2TacO) | `70871b9` | Original overlay + in-game marker editor (C++) |
| [blish-hud/Pathing](https://github.com/blish-hud/Pathing) | `bd14f74` | Blish HUD pathing module v1.14.3 (C#) |
| [dlamkins/TmfLib](https://github.com/dlamkins/TmfLib) | `966b9de` | Pack reader/writer used by Pathing |
| [TaimiHUD/TaimiHUD](https://github.com/TaimiHUD/TaimiHUD) | `9045ace` | Nexus/ArcDPS addon with pathing (Rust) v0.4.1 |
| [RaidcoreGG/Nexus-API](https://github.com/RaidcoreGG/Nexus-API) | `9b2c53d` | `Nexus.h`, API v6 |
| [RaidcoreGG/GW2-Compass](https://github.com/RaidcoreGG/GW2-Compass) | `d596f72` | Small official Nexus example addon (older API names, ImGui 1.80) |
| [RaidcoreGG/gw2-mumble-cpp](https://github.com/RaidcoreGG/gw2-mumble-cpp) | `621a794` | `Mumble.h` structs |
| [dlamkins/gw2pathing-wiki](https://github.com/dlamkins/gw2pathing-wiki) | `c3383be` | Source of gw2pathing.com |
| [dlamkins/gw2pathing-libdef](https://github.com/dlamkins/gw2pathing-libdef) | `b3a9a2e` | Lua API type stubs |
| blish-hud/Blish-HUD (2 files) | `54ec00e` | `ColorUtil.cs`, `WorldUtil.cs` only |

Notes were compiled on 2026-11-22.

## Top findings that affect design

1. **There is no in-game pack/category authoring anywhere.** TacO's editor can only drop and edit markers into one
   global `poidata.xml` with pre-existing categories. Everything else is done by hand in text editors and Excel.
   That's the gap to fill (06 §1.5, §5).
2. **The three readers disagree in subtle ways.** Element-name case, category-before-use order (TacO), snapshot vs
   dynamic inheritance, colour format (`AARRGGBB` without `#` is the only form all three read; 6-digit colours are
   invisible in TacO), `toggleCategory` vs `toggle`, `infoRange` vs `triggerRange`, duplicate GUIDs (TacO keeps only
   one), int16 limits in TacO. The exporter must follow the **safe writer profile** (01 §14).
3. **Saved-bits model:** store per item only the attributes explicitly set, and show inherited values. That
   preserves the pack's inheritance structure and matches TacO's own editor (06 §1.3).
4. **Rendering path for an MVP:** project markers with DirectXMath and draw them with the ImGui background draw list
   (like GW2-Compass). Move to D3D11 in `RT_PreRender` (like TaimiHUD) for trails and polish. World-depth occlusion
   (hooking the game's depth buffer) is optional and fragile (05 §7).
5. **Placement is from MumbleLink:** the character's feet (`AvatarPosition`) and the camera. There's no way to
   raycast the world without the game's depth, so "click to place" has to work from existing geometry (trails,
   markers) or offsets.

## Decisions (2026-11-23)

- **Rust** with [`nexus-rs`](https://github.com/zerthox/nexus-rs) (`0.12.0`, ImGui 1.80 via `arcdps-imgui`),
  cross-compiled from Linux to `x86_64-pc-windows-gnu` with MinGW-w64. Rust-specific notes: 04 §7. The C++
  samples in these notes stay as reference.
- Signature `-0x24dc02d4` (random negative). Local development only for now; GitHub / Raidcore hosting later.
- Packs live in `<GW2>/addons/PathingEditor/`. Users manage the files themselves (no publishing to Blish/TaimiHUD
  folders from the addon for now).
- **No GW2 API calls from the addon.** The editor only lets authors *set* API-driven attributes (`achievementId`,
  `achievementBit`, ...); the overlays do the API calls.
- The addon **previews the pack itself** (world, minimap, world map: 05 §9), drawn from the editor's data, rather
  than relying on an overlay to reload it: TaimiHUD can't be told to (06 §3).
- Icons and textures come from `<GW2>/addons/PathingEditor/images/`, which the user fills.
- **No hard-coded keyboard shortcuts** in the windows: they clashed with players' own bindings. Actions are buttons;
  the few that need a key are Nexus keybinds, unbound by default. Ctrl/Shift+click selection is fine.
- Undo/redo covers markers and trail points (snapshots); category edits and whole-trail creation/deletion aren't
  undoable yet.

## Open questions

- Should the addon also *display* other installed packs (to learn their categories, as TacO does), or only the pack
  being edited?
- Scope of Lua support: edit `script-*` attributes and attach `.lua` files only (no execution) is the assumption.

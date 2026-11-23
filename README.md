# Pathing Editor

A [Raidcore Nexus](https://raidcore.gg/Nexus) addon for Guild Wars 2 to create and edit marker packs (TacO / Blish
HUD Pathing / TaimiHUD format) in game.

**Status:** early. It captures markers, records trails, and edits them in a pack tree (categories, positions,
icons, textures, conditions, script hooks).

## Install

1. Install [Raidcore Nexus](https://raidcore.gg/Nexus).
2. Download `pathing_editor.dll` from the [releases](https://github.com/ElisRandomStuff/GW2PathingEditor/releases)
   and put it in `<Guild Wars 2>/addons/`.
3. In game, Nexus loads it: open it with **Alt+Shift+P** or its pin icon in the Nexus bar.

## Use

- **Alt+Shift+P** or the pin icon in the Nexus bar opens the window.
- The window shows your map and position (your character's feet, in metres, the same space as `xpos/ypos/zpos`).
- **Copy as \<POI\>** gives a ready-to-paste line for where you stand. **Copy xpos/ypos/zpos** copies just the
  position, to move a marker of an existing pack.
- **Pack editor** opens the pack window.
- **Show in world** (on by default) draws the pack's markers and trails as you edit them, in the world, on the
  minimap and on the world map: icons, heights, sizes, fades, textures and visibility, inherited from the
  categories like the overlays do. It's drawn over the world (no occlusion, like TacO), but not over the minimap.
  Turn it off if another overlay shows the same pack.

Keybinds (Nexus' keybind settings): *Show/hide window* (Alt+Shift+P), *Show/hide pack editor*, *Capture
marker* and *Trail*, the last three unbound by default. The windows have no keyboard shortcuts of their own, so
they never clash with yours.

### Markers tab

- Set a **category** (`type`, e.g. `mypack.chests`), then press **Capture marker**. Better: bind *Capture marker*
  to a spare key in Nexus' keybind settings and capture while running around.
  ```xml
  <POI MapID="50" xpos="-194.279" ypos="1.376" zpos="-161.135" type="mypack.chests" GUID="xM5D+V2CUEGt0qIpbEik4g=="/>
  ```
- **Raise markers by** (off by default) adds an offset, 1.5 m by default, to `ypos`. The overlays already draw icons
  `heightOffset` (default 1.5 m) above `ypos`, so with both, icons float about 3 m up.
- **Copy all** copies every marker.

### Trails tab

- **Points: every N m** (automatic) or **placed by hand** (manual).
- Automatic: set a category, then **Start recording** and move: a point is added every N metres (1 m by default).
  - **Pause** / **Resume**. Resuming more than 5 m from the trail's last point starts a new section (no line is
    drawn across the gap). Map changes and teleports (waypoints, portals, respawns) pause the recording by
    themselves.
  - **Add point here** adds a point whatever the spacing.
- Manual: **Start trail here** places the first point, then **Place point** at each next one. **New section** makes
  the next point start a disconnected piece.
- Bind the *Trail* keybind to do it with the window closed: it starts and pauses (automatic), or starts and places
  a point per press (manual). A small indicator shows while a trail is open.
- **Undo point** removes the last point (hold to remove several). **Stop** finishes the trail.
- **Copy** gives the `<Trail>` line. Copy its `.trl` file into the pack too, at the same relative path:
  ```xml
  <Trail type="mypack.trails" trailData="trails/map50_1.trl" GUID="..." fadeNear="5000" fadeFar="5100"/>
  ```

### Pack editor

The pack as a tree: categories, with their markers and trails inside. *This map only* hides what's on other maps.
Select an item to edit it on the right:

- **Category:** display name, icon (`iconFile`, inherited by its markers), where it shows, *Add marker here*,
  *Start a trail here*, *Select its markers*, *Use for captured markers / new trails*, new subcategory, delete
  (when empty).
- **Marker:** position (drag to nudge, Ctrl+click a box to type), *Move to my position*, category, icon, where it shows.
- **Several markers** (Ctrl+click to add or remove one, Shift+click for a range, or *Select its markers* on a
  category): move them all (drag), *Move them here* (keeps their layout), category, icon, where they show, copy,
  delete.
- **Trail:** category, texture, where it shows, and *Record more*: *Continue the line of* a section you pick
  (the one ending closest to you by default) or *New section here*. Below, its **points**: click (Ctrl/Shift+click
  for several) to drag or type a position, *Move to my position*, *Add a point after it, here*, move several
  together, copy or delete them. The selected trail's points are marked in the world, the selected ones in blue.
- **Where it shows:** *in the world*, *on the map*, *on the minimap* (`inGameVisibility`, `mapVisibility`,
  `miniMapVisibility`; all on by default, inherited from the categories).
- **Conditions** (categories, markers, trails): when the overlays show it. Festivals, mounts, professions, races and
  map types as checkboxes; specializations, achievement (+ bit), schedule (+ duration) and raid encounters as text.
  Values inherited from the categories show greyed in the box. Unknown names, bad numbers, a schedule without a
  duration and the like are flagged in yellow. The editor doesn't evaluate them: that's the overlays' job (with the
  player's API key for achievements and raids).
- **Scripts (Blish HUD only)**: the `script-once`, `-tick`, `-focus`, `-trigger` and `-filter` attributes, e.g.
  `MyPack_OnTrigger(3,'chest')`. Each is checked against the `.lua` files in the addon folder, the way Pathing
  calls it: Pathing starts at `pack.lua` and loads the others with `Pack:Require("...")`. You see where the function
  is (`Calls MyPack_OnTrigger(marker, isAutoTriggered, 3, "chest") in data/scripts/main.lua:12`), or why Pathing
  won't call it: missing, `local`, in a file nothing requires, a `Table.Function` name, an empty argument (Pathing
  drops the whole attribute), spaces that change an argument's type, or a different number of parameters. The
  *Whole pack* view (nothing selected) lists the `.lua` files and every script attribute, click one to select it.
  Write and check the Lua itself with a Lua editor; *Rescan scripts* picks up your changes.
- **Choose...** picks an image from `images/`, with previews. Put your PNGs there (subfolders work), lowercase
  names without spaces for the best compatibility.
- **Undo / Redo** (buttons): adding, moving, deleting, pasting markers and trail points, their category, icon,
  visibility, conditions and scripts, and recording more of a trail. Not undoable: category edits, pasting or deleting whole trails.
- **Copy / Paste / Paste here** (buttons) go through the clipboard as text:
  - Markers and trails as `<POI>` / `<Trail>` lines, so you can paste lines from another pack's file, or paste
    ours into one. Pasted items get new GUIDs, and go into the selected category if one is.
  - A `<Trail>` line only names its points' file (`trailData`). Put that `.trl` file in `trails/` (or at the
    path `trailData` says, from the addon folder) before pasting; the pasted trail gets its own copy of it, so the
    file you put there is never changed. Copy its texture into `images/` and pick it again if you want it.
  - Trail points as `x, y, z` lines, pasted after the selected point of the selected trail.
  - *Paste here* moves what's pasted so the first item is where you stand.
- Attributes the editor doesn't handle yet are kept as they are and listed under the item.
- **Export...** writes the pack to `export/<name>.zip` or `.taco` (the same file, two names): the XML at the root,
  then the images, `.trl` files and `.lua` scripts it uses, at their paths. Files the pack uses but the folder
  lacks are listed in the status. Ready to drop into TacO's, Blish HUD's or TaimiHUD's marker folder.
- **Import...** opens a `.zip` / `.taco` pack you put in `import/`, **in place of** the one you're editing. All its
  XML files are merged into `captures.xml`, and its other files are unpacked at their paths. The current pack is
  first backed up to `export/backup-<date>.zip` (every file of the folder but `import/`, `export/` and
  `settings.ini`); import that backup to get it back. Paths outside the pack (absolute, `..`) are skipped.

### Seeing the pack in TaimiHUD

TaimiHUD can't be told to reload one pack (docs/06 §3). To avoid copying files, link its pathing folder to this
one once (Windows command prompt, adjust the GW2 path):

```bat
mklink /J "C:\Games\Guild Wars 2\addons\TaimiHUD\pathing\PathingEditor" "C:\Games\Guild Wars 2\addons\PathingEditor"
```

Then TaimiHUD's **Reload data** button (Data sources tab) shows the current state. Turn *Show in world* off
meanwhile, or you'll see everything twice.

### Files

All in `<GW2>/addons/PathingEditor/`:

- `captures.xml`: categories, markers and trails. It's a complete small pack, so together with `trails/` and
  `images/` you can drop the folder's content into an overlay's marker folder to preview it.
- `trails/*.trl`: the trails' points.
- `images/`: your icons and textures. Paths in the XML (`images/chest.png`, `trails/map50_1.trl`) are relative to
  the addon folder, the way they are relative to a pack's root.
- `settings.ini`: the window's options.
- `import/`: packs (`.zip`, `.taco`) to import. `export/`: exports and the backups made before an import. Neither
  is part of the pack.

## Build (from Linux)

Needs rustup with the Windows target, and MinGW-w64:

```sh
rustup target add x86_64-pc-windows-gnu      # Arch: pacman -S rustup mingw-w64-gcc
cargo test                                    # unit tests, on the host
scripts/build.sh                              # → target/x86_64-pc-windows-gnu/release/pathing_editor.dll
scripts/build.sh --install "/path/to/Guild Wars 2"   # also copies it to <GW2>/addons/
```

Research notes on the pack format, Nexus and MumbleLink are in [`docs/`](docs/README.md).

## Code layout

Everything outside `src/addon/` is platform-independent and unit-tested on Linux:

| File | What it does |
|-|-|
| `pack.rs` | The pack: categories, POIs, trails; reading and writing `captures.xml` |
| `trail.rs` | Trails: points, sections, `.trl` files |
| `editor.rs` | Editor state and every action (capture, edit, record, paste, undo/redo, saving) |
| `view.rs` | How things look in game: camera and map projection, styles inherited from categories |
| `clipboard.rs` | Copy/paste text: `<POI>`/`<Trail>` lines, `x, y, z` lines |
| `settings.rs`, `images.rs` | `settings.ini`; the PNGs of `images/` |
| `conditions.rs` | The filter attributes (festival, mount, profession, schedule, ...): known values and checks |
| `archive.rs` | `.zip` / `.taco`: export, folder backup, import in place of the pack |
| `scripts.rs` | `script-*` attributes: split like Pathing, checked against the functions of the pack's `.lua` files |
| `attrs.rs`, `namespace.rs`, `files.rs`, `geometry.rs`, `format.rs`, `guid.rs` | Small helpers |

`src/addon/` is the Nexus side (Windows only): `mod.rs` (loading, keybinds, MumbleLink), `ui.rs` (main window),
`pack_window/` (pack editor: tree, details, attribute editors, conditions and scripts, import/export), `preview.rs` (drawing in the world and on the maps),
`thumbnails.rs` (textures for images).

## Credits

Pathing Editor builds on the work of the Guild Wars 2 overlay community. None of their code is included here (the
`reference/` clones are git-ignored): the notes in `docs/` describe it, with sources, and the addon reimplements
the file formats and formulas these projects established.

- [GW2TacO](https://github.com/BoyC/GW2TacO) by BoyC (CC BY-NC 4.0): the marker pack format, its in-game editor
  and trail recorder, and the minimap and world map placement the preview follows.
- [Blish HUD](https://github.com/blish-hud/Blish-HUD) and its [Pathing](https://github.com/blish-hud/Pathing)
  module (MIT), [TmfLib](https://github.com/dlamkins/TmfLib) (CPOL), the
  [Pathing wiki](https://github.com/dlamkins/gw2pathing-wiki) and
  [Lua definitions](https://github.com/dlamkins/gw2pathing-libdef), by Dade Lamkins and the Blish HUD team: how
  packs are read and drawn, and the attribute and Lua documentation.
- [TaimiHUD](https://github.com/TaimiHUD/TaimiHUD) by kittywitch (MIT): pack loading and map trails in a Nexus
  addon.
- [Raidcore Nexus](https://raidcore.gg/Nexus), with [Nexus-API](https://github.com/RaidcoreGG/Nexus-API),
  [GW2-Compass](https://github.com/RaidcoreGG/GW2-Compass) and
  [gw2-mumble-cpp](https://github.com/RaidcoreGG/gw2-mumble-cpp) (MIT): the addon loader, its API and examples,
  and the MumbleLink layout.
- [nexus-rs](https://github.com/zerthox/nexus-rs) and [gw2-mumble-rs](https://github.com/zerthox/gw2-mumble-rs)
  by Zerthox (MIT): the Rust bindings this addon is built on. With them come
  [Dear ImGui](https://github.com/ocornut/imgui) (MIT) through
  [arcdps-imgui](https://github.com/gw2scratch/imgui-rs) (MIT / Apache-2.0), and
  [roxmltree](https://github.com/RazrFalcon/roxmltree), [getrandom](https://github.com/rust-random/getrandom),
  [zip](https://github.com/zip-rs/zip2) and [flate2](https://github.com/rust-lang/flate2-rs) with
  [miniz_oxide](https://github.com/Frommi/miniz_oxide) (MIT / Apache-2.0, and others for their dependencies: see
  `THIRD-PARTY-NOTICES.txt`).

Guild Wars 2 is a trademark of ArenaNet, LLC and NCSOFT Corporation. This project isn't affiliated with or
endorsed by ArenaNet, NCSOFT or Raidcore.

## License

[MIT](LICENSE). The DLL also contains the libraries listed above, under their own licences:
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt) has their notices and ships with every release (regenerate it
with `scripts/third_party_notices.py` when dependencies change).

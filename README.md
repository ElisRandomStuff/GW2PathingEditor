# Pathing Editor

A [Raidcore Nexus](https://raidcore.gg/Nexus) addon for Guild Wars 2 to create and edit marker packs (TacO / Blish
HUD Pathing / TaimiHUD format) in game.

**Status:** early. It captures markers, records trails, and edits them in a pack tree (categories, positions,
icons, textures).

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
- **Choose...** picks an image from `images/`, with previews. Put your PNGs there (subfolders work), lowercase
  names without spaces for the best compatibility.
- **Undo / Redo** (buttons): adding, moving, deleting, pasting markers and trail points, their category, icon and
  visibility, and recording more of a trail. Not undoable: category edits, pasting or deleting whole trails.
- **Copy / Paste / Paste here** (buttons) go through the clipboard as text:
  - Markers and trails as `<POI>` / `<Trail>` lines, so you can paste lines from another pack's file, or paste
    ours into one. Pasted items get new GUIDs, and go into the selected category if one is.
  - A `<Trail>` line only names its points' file (`trailData`). Put that `.trl` file in `trails/` (or at the
    path `trailData` says, from the addon folder) before pasting; the pasted trail gets its own copy of it, so the
    file you put there is never changed. Copy its texture into `images/` and pick it again if you want it.
  - Trail points as `x, y, z` lines, pasted after the selected point of the selected trail.
  - *Paste here* moves what's pasted so the first item is where you stand.
- Attributes the editor doesn't handle yet are kept as they are and listed under the item.

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
| `attrs.rs`, `namespace.rs`, `files.rs`, `geometry.rs`, `format.rs`, `guid.rs` | Small helpers |

`src/addon/` is the Nexus side (Windows only): `mod.rs` (loading, keybinds, MumbleLink), `ui.rs` (main window),
`pack_window/` (pack editor: tree, details, attribute editors), `preview.rs` (drawing in the world and on the maps),
`thumbnails.rs` (textures for images).

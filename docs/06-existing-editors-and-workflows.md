# 06 — Existing Editors & Authoring Workflows

What exists today for *creating* packs, how it works internally, and what it lacks. This is the baseline our addon
must beat. Sources: TacO `MarkerEditor.{h,cpp}`, `GW2TacOMenu.{h,cpp}`, `gw2tactical.cpp` (CreateNewPOI, DeletePOI,
UpdatePOI), `TrailLogger.cpp`, `UI.xml`. The wiki guides `docs/guides/sywmamp/*` ("So you want to make a marker
pack", by Metallis) and the Marker Pack Academy video list. TaimiHUD `src/render/edit_marker_window.rs`.

---

## 1. TacO: the only in-game editor

### 1.1 Hotkeys (`TacOKeyAction` in `GW2TacOMenu.h`)

| Action | Effect |
|-|-|
| `AddPOI` | New marker at the **character's feet**, category = config `defaultcategory0` |
| `AddDefaultPOI_1..4` | Same, with `defaultcategory1..4`. Pack makers bind these to spare keys (numpad) and pre-assign a category to each |
| `RemovePOI` | Deletes the first own marker whose `triggerRange` contains the character |
| `ActivatePOI` | "Interact": triggers behaviours, `toggleCategory`, non-auto `copy` for POIs in range |
| `StartTrailRec` / `PauseTrailRec` / `ResumeTrailAndCreateNewSection` / `DeleteLastTrailSegment` | Trail recorder |
| `Toggle_marker_editor` | Show the editor window |
| `DeleteSelectedMarker`, `DeleteSelectedTrailSegment` (vertex), `CopySelectedMarker` (duplicate +1 m up) | Selection ops |
| `UndoEdit` / `RedoEdit` | Undo stack |

`CreateNewPOI` guard: it refuses to drop a marker if a marker **of the same category** already exists within that
marker's `triggerRange` (`testRange=true`). That prevents accidental double drops.

### 1.2 Editor window (`UI.xml` template `markereditor`)
- Header label: `EDITED MARKER: <guid>` in green for own markers, `EXTERNAL MARKER` in red for pack markers.
  External items are **not editable** unless the config `EnableExternalEditing` is set.
- Buttons: **Edit category** (pick from a category context menu, then edit that category's defaults), **change
  marker type** (assign a category), **delete**, **default1..5** (choose the category bound to each AddDefaultPOI
  key; slot 5 / `defaultcategory4` is used for new trails), and the trail tools: start/stop recording, pause, new
  section, delete last segment, save trail (Win32 *Save As* `.trl` → creates a `<Trail>`), load trail (imports a
  `.trl` into the recorder), new trail, prev/next vertex.
- Attribute panel: 33 fields in 5 groups (02 §10). Every field has a **"saved" checkbox**:
  - unchecked = inherited from the category (row drawn at 50 % opacity).
  - editing a value auto-checks it.
  - unchecking resets the value to the category's.
- Selection:
  - click a marker icon in the world or on the minimap (hit-test against the screen rects recorded during draw).
  - click a trail ribbon to select its nearest vertex; **Ctrl+click** inserts a vertex.
  - click empty space to deselect.
- The 3D move gizmo appears on the selection (05 §6). Trigger and info ranges are drawn as circles.

### 1.3 Data model & undo (`MarkerDOM`)
- Command pattern. Two parallel arrays `actionBuffer[]` and `undoBuffer[]` plus `bufferIndex`. A new action crops
  everything after the index (classic redo truncation). `Redo()` runs `actionBuffer[i++]`, `Undo()` runs
  `undoBuffer[--i]`.
- Actions: `AddMarkerFull`, `MoveMarker`, `AddTrailFull`, `MoveTrailVertex`, `AddTrailVertex`, `DeleteTrailVertex`,
  `DeleteMarkerTrail`, `SetMarkerTrailCategory`, `ChangeMarkerTrailAttrib` (full `MarkerTypeData` snapshot
  before/after), `ChangeCategoryAttrib`. **`AddCategory` / `DeleteCategory` are declared but not implemented.**
- `MarkerTypeData` = all attribute values **plus a `saveBits` bitfield** (one bit per attribute) meaning
  "explicitly set here". `Write()` emits only saved attributes. That's how inheritance survives a round trip.
- `ChangeCategoryAttrib` → `PropagateCategoryParameter` pushes the new value down the subtree into every category,
  marker and trail that has **not** saved that parameter. TacO needs this because its inheritance is a snapshot
  (01 §4.2).
- Persistence: **every action calls `ExportPOIS()`, rewriting the whole `poidata.xml`** (all non-external markers,
  trails and routes, plus the categories they use: `needsExport` / `forceExport`). Trail vertex edits rewrite the
  `.trl` file in place (`GW2Trail::Export`).
- "Export my map markers": a *Save As* of the current map's own markers to a chosen XML.

### 1.4 Trail recorder (`GW2TrailDisplay`)
- While recording and not paused, each Mumble update appends `charPos` if it moved ≥ 30 gu since the last point.
  A map change discards the recording.
- Pause → "start new section" appends the `(0,0,0)` separator.
- "Delete last segment" pops the last point.
- The live trail renders in light blue (`0.2, 0.7, 1.0, 0.8`).
- Save writes `.trl` v0 and creates a `<Trail>` with `trailData` = path relative to the TacO dir,
  `fadeNear=5000`, `fadeFar=5100`, category `defaultcategory4`.

### 1.5 Gaps and pain points (TacO editor + community workflow)
1. **Cannot create, rename, move or delete categories in game.** TacO only "knows" categories from loaded packs or
   `categorydata.xml`. The guide's workflow is: load an old version of your pack so TacO learns the categories,
   drop markers, then hand-edit XML.
2. **No pack concept.** Output is one global `poidata.xml`. Authors copy it out, restructure categories by hand,
   zip it, and rename it to `.taco`.
3. Heavy **bulk editing happens outside the game**: Find & Replace to stamp `type=` / `achievementId=` onto
   hundreds of lines, Excel "text to columns" to fill `tip-name="Token N"` and descriptions, Notepad++ regex
   cleanups (`the-extra-mile.md`).
4. No multi-select, no bulk attribute set, no auto-numbering, no achievement/bit lookup (authors use gw2treasures;
   the bit order differs from the wiki and the in-game UI).
5. No texture browser. `iconFile` is typed as a path. Missing files silently show the default icon.
6. Testing loop: copy XML into Blish `markers/`, copy icons into `markers/data`, press "Reload Markers". TacO doesn't
   live-reload hand edits.
7. Behaviour state hides markers while editing (behaviour 3 markers vanish after you walk through them).
8. Win32 file dialogs inside a fullscreen game. Trail files are saved relative to the TacO folder.
9. Pack markers are read-only by default, so you can't fix an existing pack in place.
10. No validation: TacO vs Blish compatibility, invalid colours, duplicate GUIDs, categories used before they're
    defined, int16 overflows (01 §10).

## 2. Blish HUD Pathing
No authoring UI. Useful dev affordances:
- **Reload Marker Packs** (menu + keybind), plus `advanced.yaml: marker-load-paths` to load a working folder
  (e.g. TacO's POIs dir, or **our addon's output dir**) next to the normal `markers/`.
- `edittag` attribute → debug render.
- Lua dev console with a watch tree.
- Category menu with "Smart categories" (hides those with no markers on the current map), separators and tooltips.

## 3. TaimiHUD
Has an in-game **editor for squad/commander-marker sets** (JSON, `src/marker/format.rs`,
`src/render/edit_marker_window.rs`). That's *not* pathing packs, but it's the same Nexus + ImGui stack. UX ideas
worth copying: "take current position" buttons, "take squad markers" (capture the 8 placed squad markers as
positions), "set map id from current map", validation messages listed before save (e.g. "no positions").
Its pathing side is display-only: pack sources come from a configurable list (GitHub releases, `sources.toml`) and
there are category toggles.

**Reloading packs** (checked 2026-11-23, TaimiHUD `9045ace`):
- Every entry of `addons/TaimiHUD/pathing/` is a pack: `.taco`/`.zip` files, and folders as loose packs
  (`DirectoryLoader`) (`settings_struct.rs` `read_source_dir`, `controller/pathing/mod.rs` `load_all_inner`).
- There's no file watching and no per-pack reload. The only reload is `PathingEvent::ReloadAll`: unload every pack,
  load them all again. It's triggered by the **Reload data** button (Data sources tab, which also reloads timers and
  markers) or after a source update. There's no keybind or Nexus event for it, so **another addon can't trigger it**.
- → A directory junction `addons/TaimiHUD/pathing/<name>` → `addons/PathingEditor` makes TaimiHUD read our files in
  place (`captures.xml`, `trails/`, `images/`), but a refresh still takes a *Reload data* click. Real auto-refresh
  would need a change in TaimiHUD (e.g. a Nexus event that triggers `ReloadAll`, or a per-pack reload).
- Our own answer is the in-addon world preview (05 §9), drawn from the editor's data every frame.

## 4. Community authoring conventions (from the guides)
- Organise images under `Data/` (e.g. `Data/Icons`, `Data/Trails`). Define `iconFile` once on a category: "iconFile
  once, 2243 markers changed".
- Specific beats general: put the shared value on the category and override it on the one exception.
- A huge distance ring (Blish *Distance Rings* module, 3250 gu) helps judge coverage and label visibility while
  marking.
- Tooling people use: TacO (dropping), Blish + Pathing (testing), VSCode/N++, Excel, GitHub (hosting, releases; the
  TacO and Pathing pack repos update from release URLs).
- Pack metadata lives outside the pack: TacO's `MarkerPacks.json` entries
  `{name, id, filename, versionurl, versionsearchstring, versionterminator, downloadurl, enabledbydefault}`.
  The Blish pack repo is a separate service.

## 5. Requirements this implies for our editor
- In-game **pack management**: create a pack, edit pack metadata, add/rename/move/delete categories (with
  namespace-rename refactoring of all `type=` references), separators, DisplayName, defaulttoggle.
- **Placement:** drop at feet (hotkeys bound to "slot" categories, with TacO's same-category duplicate guard),
  nudge with the gizmo, numeric fields, "snap to current position". Dropping at the mouse cursor in the world is
  *not* possible without the game's depth buffer (05 §7). Without it, placement is limited to feet, camera-relative
  offsets, or picking a point on an existing trail or marker.
- **Inheritance UI** with saved bits (override toggles, inherited values greyed out, "where does this value come
  from" hint).
- **Undo/redo** for *every* operation, including category ops and bulk edits.
- **Bulk tools:** multi-select (box or list), set an attribute on the selection, auto-number `tip-name`, assign
  `achievementBit` sequentially, fill descriptions from `/v2/achievements` bits, re-category.
- **Trail tools:** record / pause / new section / undo-last, vertex select/move/insert/delete, simplify, reverse,
  split/merge sections, assign a texture.
- **Preview:** render the pack being edited (ignoring behaviour state on request), toggle Blish-vs-TacO semantics
  where they differ.
- **I/O:** import existing `.taco`/`.zip`/folders (keep unknown attributes and Routes), export through the "safe
  writer profile" (01 §14), optional Blish-optimized export, write straight into Blish `markers/` or the TaimiHUD
  pathing dir for instant testing.
- **Linting:** compatibility badges per attribute, colour format, GUID uniqueness, missing files, int16 overflows,
  info/trigger range mismatch, trails without fade.

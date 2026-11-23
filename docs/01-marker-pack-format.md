# 01 — Marker Pack Format (TacO / Blish HUD Pathing / TaimiHUD)

The "TacO marker format" is an informal standard. There is no formal spec: the behaviour is defined by what the
three readers actually do. This document compiles the gw2pathing.com wiki with the real parsing code of:

- **TacO** (`reference/GW2TacO`, C++, `MarkerPack.cpp`, `gw2tactical.cpp`, `TrailLogger.cpp`). The original. It ships
  the only existing in-game editor.
- **Blish HUD Pathing** (`reference/Pathing`, C#), which parses through **TmfLib** (`reference/TmfLib`).
- **TaimiHUD** (`reference/TaimiHUD`, Rust, `pathing/pack/`). A Nexus/ArcDPS addon.

Attribute-by-attribute details are in [02-attribute-reference.md](02-attribute-reference.md). This file covers
containers, structure, parsing rules, coordinates, binary formats and the rules our writer must follow.

---

## 1. Terminology

| Term | Meaning |
|-|-|
| Marker pack | A `.taco`/`.zip` archive (or loose folder) of XML + textures + `.trl` (+ `.lua` for Pathing). |
| Category | `<MarkerCategory>`. A node in a tree. Users toggle categories on and off. Categories carry default attributes that children inherit. |
| Namespace / type | Dot-joined lowercase path of category names from the root, e.g. `mypack.jp.chalice`. Markers and trails reference it in `type="..."`. |
| POI / marker | `<POI>`. A billboarded icon at a 3D position. |
| Trail | `<Trail>`. A ribbon mesh built from a `.trl` point list. |
| Route | `<Route>`. TacO-only ordered POI sequence. Ignored by Pathing. |
| Pathable | Blish's term for marker *or* trail. |
| Behavior / filter / modifier | Pathing's terms for attribute-driven logic: hide after interaction, filter by mount, copy to clipboard, etc. |

---

## 2. Containers

| Format | TacO | Pathing | TaimiHUD |
|-|-|-|-|
| `.taco` (zip renamed) | ✔ | ✔ | ✔ |
| `.zip` | ✔ | ✔ | ✔ |
| Loose folder of XML | ✔ (`POIs/**/*.xml`) | ✔ (the `markers/` root dir is one "unpacked" pack) | ? |

- **Every `.xml` file in the archive is loaded**, in any folder. TacO filters by suffix `.xml` (case-insensitive) on
  zip entry names, in archive order. TmfLib loads all `*.xml` entries.
- All resource paths (`iconFile`, `texture`, `trailData`, Lua `Pack:Require`) are **relative to the archive root**,
  not to the XML file.
- Path normalisation:
  - **TacO:** icons get `\` → `/`. Zip lookup uses miniz `mz_zip_reader_locate_file(..., flags=0)`, which is
    case-insensitive. If the file is not in the pack's zip, TacO also tries the category's zip, then loose files
    relative to the TacO dir and `POIs\`. Trail `.trl` paths are **not** slash-normalised.
  - **TmfLib:** `\` → `/`, `//` → `/`, lowercased, trimmed, leading `.` stripped. Fully case-insensitive.
  - Real packs do contain `iconFile="Data\Auric_Basin.png"` (backslashes). Readers must tolerate them; writers
    should not emit them.
- **Textures:** TacO decodes **marker icons as PNG only** (`DecompressPNG` in `GetMapIcon`). Trail textures go
  through `D3DX11CreateTextureFromMemory` (png/jpg/dds/bmp...) with a PNG fallback. TaimiHUD:
  png/jpeg/tga/tiff/bmp/dds. Pathing: its texture manager (PNG is always safe). → **Write PNG.**
- **Lua:** Pathing only. The entry point is `pack.lua` at the archive root. See [03-lua-scripting.md](03-lua-scripting.md).
- Packs are meant to stay zipped. The Blish docs warn users not to extract them.

### 2.1 Blish "optimized" / manifested pack layout (TmfLib `Pack.cs`, `PackWriter.cs`)

If the archive contains `_markerCategories.xml`, TmfLib treats it as a *manifested pack*. It then loads only:

1. `_markerCategories.xml`: the whole category tree, written as `<overlaydata><markercategory .../>...`.
2. `map{mapId}.xml` (e.g. `map1206.xml`) for the current map only: `<overlaydata><pois>...</pois></overlaydata>`.

`PackWriter` writes that layout. Element names come out lowercase, POIs are grouped by MapId and sorted by category
namespace, and resource paths are **lowercased** inside the archive. Trails get a `mapid` attribute (from the `.trl`
header). `SkipMapId=true` drops `mapid` and breaks TacO. The Pathing repo states that "all packs are now optimized
prior to being downloaded" from its pack repo.

→ An optional "export optimized for Blish" mode is cheap to add later. The default export must be a plain TacO
layout.

---

## 3. Install / data locations

| Overlay | Packs | User-created / state files |
|-|-|-|
| TacO | `<TacO>/POIs/` (recursive `*.xml`, `*.zip`, `*.taco`), auto-downloads into `POIs/Online/` | `poidata.xml` (all markers made in TacO's editor, `external=false`), `categorydata.xml` (base categories), `activationdata.xml` (behavior state) |
| Blish Pathing | `Documents/Guild Wars 2/addons/blishhud/markers/` (+ extra dirs via `advanced.yaml: marker-load-paths`) | `markers/data/states/timers.txt` (`guid,timestamp`), `markers/data/states/category_preferences.txt`, `markers/data/user/{advanced,populate,ignore}.yaml`, `markers/data/kv/` (Lua `Storage`) |
| TaimiHUD | `<GW2>/addons/TaimiHUD/pathing/` (dir chosen from addon dir + `"pathing"`, see `src/settings/source/sources.rs`; fallback addon dir `addons/Taimi`). Verify in game. | its own settings in the addon dir |

For loose markers in Blish's `markers/` root, all relative resource paths resolve against `markers/` itself. The
wiki FAQ says to copy icons into `markers/data/...`.

---

## 4. XML document structure

```xml
<OverlayData>
  <!-- category tree: any depth, any number of roots -->
  <MarkerCategory name="mypack" DisplayName="My Pack">
    <MarkerCategory name="jps" DisplayName="Jumping Puzzles" iconFile="data/jp.png" heightOffset="2">
      <MarkerCategory name="hdr" DisplayName="— Core Tyria —" IsSeparator="1"/>
      <MarkerCategory name="chalice" DisplayName="Chalice of Tears" behavior="1"/>
    </MarkerCategory>
  </MarkerCategory>

  <POIs>
    <POI MapID="50" xpos="-194.279" ypos="1.37639" zpos="-161.135"
         type="mypack.jps.chalice" GUID="xM5D+V2CUEGt0qIpbEik4g=="/>
    <Trail type="mypack.jps.chalice" GUID="NdG3CddnEEO+xNrGN7lGIA=="
           trailData="data/chalice.trl" texture="data/trail.png"/>
    <Route Name="Floor is lava" BackwardDirection="0" MapID="1043"
           resetposx="-562.167" resetposy="326.338" resetposz="515.736" resetrange="0.1">
      <POI .../>   <!-- TacO only -->
    </Route>
  </POIs>
</OverlayData>
```

### 4.1 Parser behaviour per overlay

| Rule | TacO (RapidXML wrapper `Bedrock/UtilLib/XMLNode.cpp`) | Pathing (TmfLib NanoXml) | TaimiHUD |
|-|-|-|-|
| Root element | Must be exactly `OverlayData` | Not checked. Uses the root node's `markercategory` / `pois` children | — |
| Element names | **Case-sensitive**: `OverlayData`, `MarkerCategory`, `POIs`, `POI`, `Trail`, `Route` | Lowercased, so case-insensitive | case-insensitive (Rust `eq_ignore_ascii_case`) |
| Attribute names | **Case-insensitive** (ASCII), via `first_attribute(name, len, case_sensitive=false)` | Lowercased, so case-insensitive | case-insensitive |
| `bh-` prefix | Not understood. The attribute is ignored as unknown | Stripped (`VenderPrefixes=["bh-"]`), on element *and* attribute names | Stripped from attribute names |
| Entities | Standard XML + numeric (`&#xA;`) decoded | Forgiving parser | standard |
| Duplicate attribute names | first match wins | last wins. Two names differing only in case may **throw** (`AttributeCollection` groups case-sensitively and then inserts into a case-insensitive keyed collection) | — |
| Unknown attributes | ignored | kept in the collection (used for inheritance and Lua) | ignored |

- `<?xml ...?>` declaration and comments: **TacO** parses with RapidXML `parse<0>`, which skips both, then looks
  up `OverlayData` among the document's children (`XMLDocument.cpp`, `MarkerPack.cpp`). TacO's own exporter writes
  no declaration. **NanoXml** skips declarations and `<!-- -->` comments between elements. A comment *before the
  root* is skipped up to the first `>`, so a `>` inside it would break parsing (`NanoXmlBase.SkipSpaces`,
  `NanoXmlDocument` ctor). → We write a declaration and no comments.

> The wiki says "TacO attribute names are case-sensitive". That was true for older MSXML-based TacO builds. The
> current BoyC source is case-insensitive for attributes, but element names are still case-sensitive.
> **Writers should still use TacO's canonical casing** (see 02) so old TacO builds work.

### 4.2 Multiple files, merging, order dependence

- All XML documents of all packs merge into **one global category tree** in TacO (`CategoryMap`) and in Pathing
  (`SharedPackCollection`). A `MarkerCategory` with an existing namespace *extends/overrides* the existing node. This
  lets packs reference other packs' categories (`toggle`/`show`/`hide`).
- **TacO resolves `type` → category at the moment each POI is parsed** (`ImportPOI` → `GetCategory`). If the
  category is declared in a later file, the POI gets **no category**: it never hides with the category toggle and gets
  none of the inherited attributes. Within a document, TacO parses categories (`RecursiveImportPOIType`) before
  `<POIs>`.
- **TacO's inheritance is a snapshot:** a new category node gets `data = copy(parent.data)` when it is created, and a
  POI gets `copy(category.data)` when it is parsed. Re-declaring a parent later does not update children that already
  exist.
- **Pathing's inheritance is dynamic** (`IAggregatesAttributes`: lookup walks POI → category → parent ... at
  population time) and order-independent. Unknown `type` namespaces are auto-created
  (`GetOrAddCategoryFromNamespace`).
- → **Writer rule:** emit the full category tree first in every XML file that has POIs (or at least in a file that
  sorts first in the zip, and keep that file first in the zip's entry order). Simplest: one XML per pack containing
  categories then POIs.

---

## 5. Categories (`<MarkerCategory>`)

### 5.1 Names and namespaces

- `name` is **required**. Nodes without it are skipped (TacO and TmfLib).
- **TacO:** every char that is not `isalnum` and not `.` becomes `_`. Then the name is **split on `.`**, so
  `name="a.b"` creates nested `a` → `b`. The namespace is lowercased for lookup. DisplayName defaults to the original
  (non-lowercased) segment.
- **TmfLib:** splits on `.` (`GetOrAddCategoryFromNamespace`). Each segment: every char that is not
  `char.IsLetterOrDigit` becomes `_`. Lookup is case-insensitive. `DisplayName` defaults to `Name`.
- `char.IsLetterOrDigit` is Unicode-aware but TacO's `isalnum` is not. Non-ASCII names therefore produce
  **different namespaces** in the two overlays.
- → **Writer rule:** generate names matching `[a-z0-9_]+`. Put human-readable text in `DisplayName`. Avoid `.` in
  `name`; nest elements instead.

### 5.2 Category-only attributes

| Attribute | Type | Default | Notes |
|-|-|-|-|
| `name` | string | — | required, see above |
| `DisplayName` | string | = name | shown in menus |
| `IsSeparator` | bool | 0 | rendered as a header. TacO: no toggle, children not shown as a submenu |
| `defaulttoggle` | bool | 1 | initial enabled state. User choice persists afterwards (TacO config `CategoryVisible_<ns>`, Pathing `category_preferences.txt`). **Not inherited** by children in TacO |
| `ishidden` | bool | 0 | Pathing + TaimiHUD only: hides the category from the menu |
| `forceExport` | int | 0 | TacO internal: forces the category into `poidata.xml` |

Any other marker/trail attribute placed on a category becomes a **default for everything below it** (sub-categories,
markers, trails). This is the main tool for keeping packs small: one `iconFile` on a category can serve 2000+
markers.

### 5.3 Visibility

A category is visible iff it is toggled on **and** every ancestor is toggled on. Markers or trails whose category is
hidden are not drawn. (TacO: `GW2TacticalCategory::IsVisible`, cached.)

Menu niceties: TacO hides categories with no markers on the current map (`containedMapIds`), and hides ones whose
achievement is completed. Pathing has "Smart categories" doing the same.

---

## 6. Markers (`<POI>`)

Minimum useful marker: `MapID`, `xpos`, `ypos`, `zpos`, `type`, `GUID`. Everything else can be inherited from the
category.

### 6.1 Coordinate system and units

- Positions use **MumbleLink avatar space for the current map**, in **metres**. The space is **left-handed** with
  **Y up** (DirectX convention):
  - `+X` = east, `+Z` = north, `+Y` = up. (Derived from TacO's minimap transform: map-x = world x, map-y = world z,
    then a Y flip because continent Y grows southward.)
- The game's own units are **inches**: 1 m = 39.37 game units (`WorldUtil.METER_TO_INCH = 39.37`, TacO
  `x / 0.0254`). The GW2 API `map_rect` is in inches, same origin and axes.
- Attribute units are therefore mixed:
  - **metres:** `xpos/ypos/zpos`, `heightOffset`, `triggerRange`, `infoRange`, `bounce-height`.
  - **game units (inches):** `fadeNear`, `fadeFar`.
  - **pixels:** `minSize`, `maxSize`, `mapDisplaySize`.
- TacO's "place marker" uses the **character position** (`fAvatarPosition`, i.e. the feet). `heightOffset`
  (default 1.5 m) lifts the icon above that.
- **Blish swaps Y/Z internally** (XNA is Z-up): `Position = (xpos, zpos, ypos)`. Lua sees the swapped space, hence the
  wiki's "Z and Y are swapped" warnings. Files always use the TacO order.

### 6.2 GUID

- Format: **base64 of 16 raw bytes** (24 chars ending `==`), e.g. `xM5D+V2CUEGt0qIpbEik4g==`.
- Byte layout is the Windows `GUID` struct / .NET `Guid.ToByteArray()`: Data1 u32 LE, Data2 u16 LE, Data3 u16 LE,
  Data4[8]. Only the round-trip matters, so treat it as 16 opaque bytes and generate them randomly (UUID v4).
- Missing or invalid:
  - **TacO** (`LoadGUID`): if the decoded size ≠ 16 it calls `CoCreateGuid`, a **new random GUID every load**, so
    behaviour state is never persisted.
  - **Pathing:** if the value isn't 4n chars ending `==`, it uses **MD5(value)** as the GUID (deterministic, so
    readable ids like `GUID="chest-1"` "work" in Pathing only). Missing → random.
- **Duplicates:** TacO stores POIs in `POISet[mapID][guid]`, so **two markers with the same GUID on one map overwrite
  each other** in TacO, and only the last one survives. Pathing allows duplicates on purpose to share behaviour state
  (the `invertbehavior` trick).
- → **Writer rule:** always emit a fresh random GUID per marker/trail and preserve existing GUIDs on edit. Only
  share a GUID when the user explicitly links markers, and warn that this breaks TacO.

### 6.3 Map ID

`MapID` is the GW2 API map id (`/v2/maps`), read from MumbleLink. Instances and story maps have their own ids.
TmfLib also lets `mapid` be **inherited from a category** (aggregated lookup). TacO does not.

---

## 7. Trails (`<Trail>`)

```xml
<Trail type="mypack.jps.chalice" GUID="..." trailData="data/chalice.trl"
       texture="data/arrow.png" animSpeed="1" trailScale="1" color="FFFFFFFF"
       fadeNear="5000" fadeFar="5100"/>
```

- `trailData` (required) points at a `.trl` file. **The map id comes from the `.trl` header**, not from XML. TacO
  and TmfLib read the header. TmfLib injects it as a `mapid` attribute.
- `texture` is the trail texture, repeated along the length. Default: TacO `Data/trail.png`, Pathing a built-in
  texture.
- TacO's editor creates new trails with `fadeNear=5000 fadeFar=5100`. From reading TacO's trail pixel shader,
  `farFade = saturate(1-(z-near)/(far-near))` with the defaults `-1/-1` looks like it divides by zero. **Unverified**,
  but **always write explicit `fadeNear`/`fadeFar` on trails** (or their category).
- Pathing's built-in trail defaults are `fadeNear=700`, `fadeFar=900` (`TrailPopulationDefaults`). TacO's are `-1`
  (disabled).

---

## 8. `.trl` binary trail format (v0)

All little-endian. Writers: TacO `GW2Trail::SaveToFile`. Readers: TacO `GW2Trail::Import`, TmfLib `TrlFileReader`.

```
offset  size        field
0       int32       version   (always 0)
4       int32       mapId     (GW2 API map id)
8       N × 12      points: float32 x, float32 y, float32 z   (same space as xpos/ypos/zpos: metres, Y up)
```

- File size = `8 + 12·N`. No point count field; readers use `(len - 8) / 12`.
- **Point `(0,0,0)` = section break.** It ends the current segment and starts a new disconnected one. TacO's recorder
  writes it on "resume trail and create new section". TmfLib yields a new `TrailSection`, and TacO's mesh builder
  resets its `lastPos`.
- TacO ignores `version`. TmfLib returns **no points** if `version != 0`.
- TmfLib reads `x, z, y` and builds `Vector3(x, y, z)`, which is just its Y/Z swap. The file order is (x, up, north).
- A segment needs ≥ 2 points to render. TacO's vertex editor refuses to delete below 2 points.
- TacO's recorder appends a point only after moving **≥ 30 game units (~0.76 m)** from the last one
  (`DoTrailLogging`). Pathing resamples rendered trails to ≤ 20 units between points
  (`PostProcessing_SetTrailResolution`).
- Python reference:
  ```python
  import struct
  def read_trl(b):
      ver, map_id = struct.unpack_from('<ii', b, 0)
      pts = [struct.unpack_from('<fff', b, 8+12*i) for i in range((len(b)-8)//12)]
      sections, cur = [], []
      for p in pts:
          if p == (0.0, 0.0, 0.0):
              if cur: sections.append(cur); cur = []
          else: cur.append(p)
      if cur: sections.append(cur)
      return ver, map_id, sections
  def write_trl(map_id, sections):
      out = bytearray(struct.pack('<ii', 0, map_id))
      for i, sec in enumerate(sections):
          if i: out += struct.pack('<fff', 0, 0, 0)
          for p in sec: out += struct.pack('<fff', *p)
      return bytes(out)
  ```

---

## 9. Routes (`<Route>`): TacO only

```xml
<Route Name="..." BackwardDirection="0|1" MapID="1043"
       resetposx=".." resetposy=".." resetposz=".." resetrange="..">
  <POI .../> <POI .../> ...
</Route>
```

Only the active POI of the route is drawn. Reaching it (distance < its `triggerRange`) advances the index (backwards
if `BackwardDirection=1`, wrapping). Entering `resetrange` around `resetpos` resets to index 0. On map change or
teleport (> 2000 units), it picks the closest route POI. **TmfLib ignores `<Route>` entirely**, so these POIs don't
exist in Blish. → Don't author routes. Read them so we don't lose data.

---

## 10. Value encoding

| Type | TacO reader | Pathing reader | TaimiHUD | **Writer rule** |
|-|-|-|-|-|
| bool | `_stscanf("%d")`, nonzero = true. `"true"` fails to parse and leaves an **uninitialised** local | `int > 0` or `"true"` (ci) | `true`/`false`/int | write `0` / `1` |
| int | `%d` from a **20-char buffer** | `int.TryParse(Any, Invariant)` | `parse::<i32>` | plain decimal |
| float | `%g` from a **20-char buffer** (longer strings truncated) | `float.TryParse(Any, Invariant)` | `f32::from_str` | `.` decimal point, no exponent, ≤ 19 chars. TacO itself *writes* `%g` (6 significant digits). We should keep ~3 decimals for metres |
| color | `"%x"` scan into ARGB: **8 hex digits `AARRGGBB`, no `#`**. A `#` makes the scan fail (white). **6 digits give alpha = 0, so markers are invisible in TacO** (trails ignore color alpha in TacO) | `TrimStart('#')`; 6 digits get `FF` alpha; parses `AARRGGBB` | trims `#`; 6 digits get opaque; `AARRGGBB` | **write 8-digit `AARRGGBB` without `#`**, e.g. `FFFF8000`. Read all forms |
| string list | n/a (TacO `festival`: comma split, lowercased) | split on `,`, trimmed | split on `,`, trimmed | `a,b,c` with no spaces |
| file | path relative to the archive root | idem | idem | forward slashes, lowercase |
| text | XML-escaped. `&#xA;` for newlines (`info`) | idem | idem | escape `< > & ' "` and newlines |

TacO also stores several numeric attributes as **int16** (`TS16`), so values above **32767 overflow** in TacO:
`resetLength`, `minSize`, `maxSize`, `achievementId`, `achievementBit`. (TacO also keeps string-table indices as
int16, capping it at about 32k unique strings.) Pathing uses float/int32.

---

## 11. The `bh-` prefix (Pathing / TaimiHUD)

Pathing strips a leading `bh-` from **element and attribute names**. TacO doesn't recognise the prefixed name and
ignores it. This gives per-overlay overrides:

```xml
<MarkerCategory name="overlay" displayname="You're using TacO!" bh-displayname="You're using Blish HUD's Pathing module!"/>
```

- The prefixed attribute must come **after** the plain one. Pathing lowercases both to the same key and the last one
  wins.
- Anything that inherits can still override it with its own attribute, prefixed or not.

---

## 12. Behavior state persistence (for preview / "reset my state" features)

- **TacO** `activationdata.xml`:
  `<OverlayData><Activations><POIActivation lut1="" lut2="" instance="" GUID=""/>...`. `lut1`/`lut2` are the
  low/high int32 halves of a 64-bit `time_t`. `instance` holds the shard id (behavior 6), a hash of the character
  name (7), or both XORed (8).
- **Pathing** `markers/data/states/timers.txt`: `guid,timestamp` lines. The per-character variant XORs the GUID with
  a character-name GUID.
- An editor should offer to **ignore behaviour filtering** while editing (TacO's editor does not, and that is
  confusing).

---

## 13. Compatibility matrix (structure level)

| Feature | TacO | Pathing | TaimiHUD |
|-|-|-|-|
| `.taco`/`.zip`, multiple XML | ✔ | ✔ | ✔ |
| Case-insensitive element names | ✘ | ✔ | ✔ |
| Order-independent categories | ✘ | ✔ | ✔ (merges) |
| Dynamic inheritance | ✘ (snapshot) | ✔ | ✔ (`merge`) |
| `<Route>` | ✔ | ✘ | ✘ |
| `bh-` prefix | ✘ | ✔ | ✔ |
| Lua `pack.lua` | ✘ | ✔ | ✘ |
| Manifested pack (`_markerCategories.xml`) | ✘ (loads it like any XML, fine) | ✔ | ? |
| Non-PNG textures | ✘ | ? | ✔ |

---

## 14. "Safe writer profile": rules for our exporter

All three overlays read the *same* format. They only disagree at the edges (above). Packs that work everywhere are
written in the common subset, and overlay-specific attributes degrade gracefully because every reader ignores
attributes it doesn't know. This profile is that subset.

1. Root `<OverlayData>`. Elements exactly `MarkerCategory`, `POIs`, `POI`, `Trail`. Never write `Route`.
2. Use canonical TacO attribute casing (`MapID`, `xpos`, `iconFile`, `GUID`, `DisplayName`, `IsSeparator`,
   `defaulttoggle`, `toggleCategory`, `copy-message`, ... see 02).
3. Category `name` must match `[a-z0-9_]+`. Human text goes in `DisplayName`.
4. Every XML that contains POIs also contains (or is preceded in zip order by) the full category tree for the
   namespaces it uses.
5. A unique random base64 GUID per marker/trail. Preserve GUIDs on round-trip.
6. Colors as 8-digit `AARRGGBB`, no `#`.
7. Bools as `0`/`1`. Floats with `.`, ≤ 19 chars, no exponent. Ints within int16 for the TacO int16 fields (or
   warn).
8. Resource paths: forward slashes, lowercase, relative to the archive root, no `./`. PNG only.
9. Trails: write `.trl` v0 with the correct mapId header. Always set `fadeNear`/`fadeFar` on trails or their
   category.
10. Write only explicitly-set attributes ("saved bits", see 06) to keep inheritance intact. Don't flatten inherited
    values onto every marker.
11. Overlay-exclusive attributes (02, "support" column) are allowed but the UI must flag them. Offer `bh-` variants
    when the user wants divergent TacO and Blish behaviour.
12. Keep unknown attributes and elements found on import verbatim (round-trip safety).

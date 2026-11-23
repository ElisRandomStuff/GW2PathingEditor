# 03 — Lua Scripting (Blish HUD Pathing only)

Sources: `reference/gw2pathing-libdef/library/**` (canonical LuaLS type stubs), wiki `docs/lua-scripting/**`,
Pathing `Scripting/**`, `Behavior/Script.cs`, `PackInitiator.cs`.

Only Pathing runs Lua. TacO and TaimiHUD ignore `.lua` files and `script-*` attributes. A pack must therefore still
make sense without scripts. For our editor, scripting support means: **edit or attach `.lua` files to the pack, edit
`script-*` attributes, and validate call syntax.** We will not execute Lua in our addon.

---

## 1. Runtime

- Engine: **NeoLua 1.3.20-beta** (Lua 5.3-ish on the .NET DLR), not PUC-Lua. Created with
  `new Lua(LuaIntegerType.Int32, LuaFloatType.Float)`, so **integers are 32-bit and floats are 32-bit**.
- A single sandboxed global table (`PathingGlobal`) is shared by all packs. Each pack also gets its own `Pack` context
  object. Prefix your globals (e.g. `EP_TriggerFunction`, `ExamplePack = {}`) to avoid collisions.
- Scripts run only if the user enables scripts in Pathing settings (`ScriptsEnabled`). Script attributes are only
  wired when it's on (`StandardMarker_Behaviors.cs`).
- Entry point: **`pack.lua` at the archive root**, loaded per pack after the pack's XML (`PackInitiator.cs:325`).
- Available std libs: `coroutine`, `bit32`, `math`, `string`, `table`, a restricted `os` (`Scripting/Lib/Std/LuaLibraryOS.cs`),
  `pairs/ipairs/mpairs/next/nextKey/select/type/tostring/tonumber/rawget/rawset/rawequal/rawlen/rawmembers/rawarray/getmetatable/setmetatable`,
  `print` (→ `Debug:Print`). **No `io`, no `load`/`dofile`, and `require` is deprecated** (use `Pack:Require`).
- A dev console window (`Scripting/Console/*`) shows prints, warnings, errors, and the `Debug:Watch` tree.
- Call style: `Global:Function(...)` (colon) for the API objects, `obj.Field` (dot) for fields.

## 2. Coordinate gotcha

Lua sees **Blish's Z-up space**: `Mumble.PlayerCharacter.Position` and `marker.Position` have `Z` = height. XML/TacO
uses `ypos` = height. When building attribute tables for `CreateMarker`, map them as:

```lua
local p = Mumble.PlayerCharacter.Position
Pack:CreateMarker({ type = "ep.cat", xpos = p.X, ypos = p.Z, zpos = p.Y, iconFile = "Data/ep/icon.png" })
```

## 3. Script attributes (XML → Lua)

| Attribute | Lua signature | Fires |
|-|-|-|
| `script-once="F(a,b)"` | `F(marker, a, b)` | once after all packs loaded |
| `script-tick="F()"` | `F(marker, gameTime)` | every update (`TICK_FREQUENCY = 0`) |
| `script-focus="F()"` | `F(marker, isFocused)` | enter (`true`) / leave (`false`) `triggerRange` |
| `script-trigger="F()"` | `F(marker, isAutoTriggered)` | interact key in range, or auto-trigger |
| `script-filter="F()"` | `F(marker)` → `boolean` | every frame. **Return `true` to hide** |

Argument literal parsing (`Script.cs` `GetValueFromString`): `nil` → nil; text containing `.` → float (else kept as
string); int → Int32; `true`/`false` (case-insensitive) → bool; otherwise a string with surrounding `'`/`"` trimmed.
The split is on `,` with no quoting, so **commas and parentheses inside string args are not supported**. If a pack
author needs complex args, they should pass a key and look it up in a Lua table.

## 4. Global API (libdef `library/global/*.lua`)

### `Pack` (the current pack's context)
- `Pack:Require(path)`: executes a pack-relative Lua file (`.lua` optional, lowercased, cached per pack). Load order
  = require order. Things referenced by later scripts must be required first.
- `Pack:CreateMarker(attrs?) → Marker`: attribute table with the same names as XML (`type`, `xpos`, `ypos`,
  `zpos`, `iconFile`, `MapVisibility`, `InGameVisibility`, ...). Always set `type`.
- `Pack:CreateTrail(attrs?) → Trail`

### `World` (all loaded packs; `Packs` is a deprecated alias)
- `World.RootCategory: Category`
- `World:CategoryByType(ns) → Category`
- `World:PathableByGuid(b64) → IPathable`, `World:PathablesByGuid(b64) → IPathable[]`
- `World:MarkerByGuid(b64) → Marker`, `World:TrailByGuid(b64) → Trail`
- `World:GetClosestMarker([category]) → Marker`
- `World:GetClosestMarkers([category,] n) → Marker[]`

### `Event`
- `Event:OnTick(fn(gameTime))`: register a per-tick callback (performance-sensitive).

### `Debug`
- `Debug:Print(msg)`, `Debug:Warn(msg)`, `Debug:Error(msg)`
- `Debug:Watch(key, value|table)`, `Debug:ClearWatch(key)`

### `I` (instance factory)
- `I:Vector3(x, y, z)`, `I:Color(r, g, b[, a=255])`, `I:Guid(base64)`
- `I:Marker(pack, attrs?)`
- `I:Texture(assetId)` (GW2 render CDN asset), `I:Texture(pack, path)`

### `Menu` (adds a "Scripts" submenu to Pathing's tray menu)
- `Menu:Add(name, onClick(menu) | nil[, canCheck, checked[, tooltip]]) → Menu`. Call `:Add` on the returned
  menu to nest.
- Fields: `Name`, `OnClick`, `CanCheck`, `Checked`, `Tooltip`. `Menu:Remove(item)`.

### `User`
- `User.SetClipboard(value) → bool` (respects the user's clipboard consent)
- `User.ShowInfo(message) → key`, `User.HideInfo(key)`

### `Storage` (persistent KV, `markers/data/kv/`)
- `Storage:UpsertValue([ns,] name, value: string) → string`
- `Storage:ReadValue([ns,] name) → string?`
- `Storage:DeleteValue([ns,] name)`

### `Pathing`
- `Pathing.Version: string`, `Pathing.IsVersionAtLeast("1.11.2")`. (`PathingVersion` global is deprecated.)

### `Mumble` (Blish `Gw2MumbleService`, Z-up)
- `Mumble.IsAvailable`, `Tick`, `TimeSinceTick`, `CurrentMumbleMapName`
- `Mumble.CurrentMap { Id, IsCompetitiveMode, Type: MumbleMapType }`
- `Mumble.Info { BuildId, IsGameFocused }`
- `Mumble.PlayerCamera { Position, Forward, FieldOfView, NearPlaneRenderDistance, FarPlaneRenderDistance }`
- `Mumble.PlayerCharacter { Position, Forward, Name, Profession, Race, Specialization, CurrentMount, IsCommander, IsInCombat, TeamColorId }`
- `Mumble.UI { CompassRotation, CompassSize, IsCompassRotationEnabled, IsCompassTopRight, IsMapOpen, IsTextInputFocused, MapCenter, MapPosition, MapScale, UISize }`
- Enums:
  - MapType: Unknown −1, Redirect 0, CharacterCreate 1, Pvp 2, Gvg 3, Instance 4, Public 5, Tournament 6,
    Tutorial 7, UserTournament 8, Center/EternalBattlegrounds 9, Blue 10, Green 11, Red 12, FortunesVale 13,
    JumpPuzzle/ObsidianSanctum 14, EdgeOfTheMists 15, PublicMini 16, WvwLounge 18.
  - Mount: None 0, Jackal 1, Griffon 2, Springer 3, Skimmer 4, Raptor 5, RollerBeetle 6, Warclaw 7, Skyscale 8,
    Skiff 9, SiegeTurtle 10.
  - Race: Asura 0, Charr 1, Human 2, Norn 3, Sylvari 4.
  - Profession: Guardian 1 … Revenant 9 (same order as §5 of doc 02).
  - UISize: Small 0, Normal 1, Large 2, Larger 3.

## 5. Types (libdef `library/types/*.lua`)

**Marker** fields (mostly mirror attributes): `Alpha, AutoTrigger, BehaviorFiltered (ro), Behaviors, CanFade,
Category (ro), CullDirection, DistanceToPlayer, DrawOrder, FadeFar, FadeNear, Focused, Guid, HeightOffset,
InGameVisibility, InvertBehavior, MapDisplaySize, MapId (ro), MapVisibility, MaxSize, MiniMapVisibility, MinSize,
Position: Vector3, ResetLength, RotationXyz: Vector3, ScaleOnMapWithZoom, Size, Texture, Tint: Color, TipDescription,
TipName, TriggerRange`.
Methods: `Focus()`, `Unfocus()`, `Interact(isAuto)`, `SetPos(x,y,z | v)`, `SetPosX/Y/Z`, `SetRot(x,y,z | v)` and
`SetRotX/Y/Z` (**radians**, while the XML `rotate` is degrees), `Remove()`, `SetTexture(path | assetId)`,
`GetBehavior(typeName)`.

**Trail** fields: `Alpha, AnimationSpeed, BehaviorFiltered, Behaviors, CanFade, Category, CullDirection, FadeFar,
FadeNear, Guid, InGameVisibility, IsWall, MapId, MapVisibility, MiniMapVisibility, ResetLength, Texture, Tint,
TrailSampleColor (minimap color), TrailScale, TriggerRange`. Methods: `Focus/Unfocus/Interact`, `Remove()`,
`SetTexture(path)`, `SetPoints({Vector3...})`, `GetBehavior(name)`.

**Category**: `DefaultToggle, DisplayName, IsHidden, IsSeparator, LoadedFromPack, Name, Namespace, Parent, Root`.
Methods: `GetOrAddCategoryFromNamespace(ns)`, `IsVisible()`, `Show()`, `Hide()`, `GetMarkers([all])`,
`GetTrails([all])`.

**IPathable**: `Guid, Behaviors, BehaviorFiltered, TriggerRange, DistanceToPlayer, MapId, Category`. Methods
`Focus/Interact/Unfocus`.

**Vector3**: `X, Y, Z`. Operators `+ - * /` (vector and scalar). Methods `Length()`, `Dot(v)`, `Cross(v)`,
`Normalize()`.

**Color**: `R, G, B, A` (0–255), `* number`. **Guid**: `ToBase64()`. **Texture**: `Width, Height`.
**GameTime**: `ElapsedGameTime, TotalGameTime: TimeSpan`. **TimeSpan**: `Days … Milliseconds, Ticks, TotalDays …
TotalMilliseconds`.

## 6. Recommended pack layout for scripts (wiki convention)

```
pack.lua                         -- entry: global table + Pack:Require calls
Data/<PackName>/Scripts/*.lua     -- modules
Data/<PackName>/Markers/*.png
gw2pathing-libdef-main/          -- IntelliSense stubs (dev only; exclude from release)
```

Minimal `pack.lua`:

```lua
ExamplePack = { Version = "1.0.0" }
Pack:Require("Data/ExamplePack/Scripts/TriggerFunction.lua")
Pack:Require("Data/ExamplePack/Scripts/menu.lua")   -- after the functions it references
```

## 7. Editor implications

- Expose `script-*` fields as free text with **syntax validation** matching `SplitFunc`: identifier, optional
  `(...)`, no commas or parens inside string args. Warn if the referenced function name doesn't appear in any
  pack `.lua` (a cheap regex scan for `function Name(` / `Name = function`).
- Ship or offer the libdef files when creating a scripted pack, and keep them out of the exported archive.
- Show a "Blish-only" badge. Scripts are invisible to TacO and TaimiHUD users.

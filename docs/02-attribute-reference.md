# 02 — Attribute Reference

The full attribute catalogue. Sources: the gw2pathing.com wiki (`reference/gw2pathing-wiki/docs/marker-dev/attributes/*.md`),
TacO `MarkerTypeData::Read/Write` (`gw2tactical.cpp` ~L1304–1626), Pathing `Entity/StandardMarker*.cs`,
`Entity/StandardTrail*.cs`, `Behavior/**`, `State/UserResources/Population/*Defaults.cs`, and TaimiHUD
`pathing/pack/src/attributes/mod.rs`.

Legend:
- **On** = which elements accept it: **C** category (inherited by children), **M** marker `<POI>`, **T** `<Trail>`.
- **Support** = **T**acO / **P**athing / tai**M**iHUD. ✔ supported, ✘ ignored, ◐ partial (see notes).
- *Canonical casing* is how TacO writes the attribute, and what we should write. Readers are case-insensitive
  (TacO for attributes only; see 01 §4.1).
- Units: **m** metres, **gu** game units (inches; 39.37 gu = 1 m), **px** pixels, **s** seconds, **min** minutes.

---

## 1. Identity & placement

| Attribute | On | Type | Default | T/P/M | Notes |
|-|-|-|-|-|-|
| `MapID` | M (P: also T, C) | int | — | ✔✔✔ | GW2 API map id. Trails take theirs from the `.trl` header. TmfLib also inherits `mapid` from categories. |
| `xpos` `ypos` `zpos` | M | float m | 0 | ✔✔✔ | MumbleLink space, Y up. See 01 §6.1. |
| `type` | M, T | namespace | — | ✔✔✔ | Category the item belongs to, e.g. `pack.cat.sub`. Case-insensitive. Should always be set. |
| `GUID` | M, T | base64(16 B) | random | ✔✔◐ | Tracks behaviour state. See 01 §6.2. Wiki marks TaimiHUD ✘ (it doesn't persist behaviours). |
| `trailData` | T (C) | file | — | ✔✔✔ | Path to the `.trl`. |

## 2. Marker appearance

| Attribute | On | Type | Default | T/P/M | Notes |
|-|-|-|-|-|-|
| `iconFile` | M, C | file | overlay default icon | ✔✔✔ | PNG. Shared textures are encouraged. |
| `iconSize` | M, C | float | 1.0 | ✔✔✔ | **World size: the quad edge = 2 m × iconSize** in both TacO and Pathing (a quad of half-extent 0.5, scaled ×2). Pathing also multiplies by the user's "Marker Scale". |
| `alpha` | M, T, C | float 0–1 | 1.0 | ✔✔◐ | Clamped. TaimiHUD: markers only. TacO trails: alpha comes from this attribute, not from `color`. |
| `color` | M, T, C | hex | `FFFFFFFF` | ✔✔✔ | Tint multiplier. **Write `AARRGGBB` (8 digits, no `#`)**. 6 digits = alpha 0 in TacO (invisible marker). See 01 §10. |
| `tint` | M, T | hex | — | ✘✔✔ | Blish-only alias, applied *after* `color` (overrides it). |
| `heightOffset` | M, C | float m | 1.5 | ✔✔✔ | Vertical lift of the icon above the position. TacO: added to Y. Pathing: added to Z (its Y/Z swap). |
| `minSize` | M, C | px (T: int16) | 5 | ✔✔✘ | On-screen clamp. Effectively **half the edge length**: the icon is never smaller than 2·minSize px. Same in TacO (`s` = half-extent) and Pathing (`clamp(pixelSizeY, MinSize*4, MaxSize*4)` on an NDC span of 2× pixel size). |
| `maxSize` | M, C | px (T: int16) | 2048 | ✔✔✘ | Icon never larger than 2·maxSize px. |
| `fadeNear` | M, T, C | float gu | −1 | ✔✔✘ | Fade starts. −1 = disabled. TacO markers measure from the **camera**, Pathing from the **player**. Pathing: user "Max View Distance" caps it. |
| `fadeFar` | M, T, C | float gu | −1 | ✔✔✘ | Fully invisible beyond this. Should be > fadeNear. TacO culls markers beyond fadeFar. **Pathing trail defaults: 700/900.** TacO new trails use 5000/5100. |
| `rotate` | M | "x,y,z" degrees | — | ✘✔✘ | Static rotation instead of billboarding. Applied X, then Y, then Z. |
| `rotate-x` `rotate-y` `rotate-z` | M | float degrees | — | ✘✔✘ | Per-axis form. |
| `cull` | M, T | `None`\|`Clockwise`\|`CounterClockwise` | None | ✘✔✘ | Backface culling. Only meaningful with `rotate` on markers. |
| `canfade` | M, T | bool | 1 | ✘✔✘ | 0 = ignore user fade features (fade near character, between camera and character, combat fade). Use sparingly. |
| `occlude` | M | bool | 0 | ✘✔✘ | The marker writes depth and occludes Pathing elements behind it. No texture needed. |
| `title` / `text` | M | string | — | ✘✔✘ | *Undocumented in the wiki.* Billboard text drawn on the marker (`text` is TacO's WvW naming). |
| `title-color` | M | hex | white | ✘✔✘ | *Undocumented.* Color of `title`. |
| `edittag` | M, T | int | — | ✘◐◐ | *Undocumented.* Pathing: any value enables `DebugRender`. TaimiHUD parses it. |

## 3. Map / minimap

| Attribute | On | Type | Default | T/P/M | Notes |
|-|-|-|-|-|-|
| `mapDisplaySize` | M, C (TacO: also T) | px | 20 | ✔✔✔ | Icon size on minimap/world map at zoom scale 1. TacO trails: map line width = mapDisplaySize·0.5. TacO multiplies by the UI-size scale. |
| `scaleOnMapWithZoom` | M, C (TacO: also T) | bool | 1 | ✔✔✘ | size /= mapScale when on. |
| `mapFadeoutScaleLevel` | M, T, C | float | 100 | ✔✘✘ | TacO-only. Map alpha × (1 − clamp((mapScale − level)/2, 0, 1)). |
| `keepOnMapEdge` | M, C | bool | 0 | ✔✘✘ | TacO-only. Clamp the icon to the minimap border instead of hiding it. |
| `miniMapVisibility` | M, T, C | bool | 1 | ✔✔✔ | |
| `mapVisibility` | M, T, C | bool | 1 | ✔✔✔ | Full-screen map. |
| `inGameVisibility` | M, T, C | bool | 1 | ✔✔✔ | 3D world. |
| `tip-name` | M, C | string | category DisplayName | ✘✔◐ | Minimap hover title. `tip-name=""` disables the tooltip entirely (including distance). TaimiHUD: name only. |
| `tip-description` | M, C | string | — | ✘✔✘ | Tooltip body. Categories: shown in the category menu. |

## 4. Triggering & interaction

"Triggered" = the player is within `triggerRange` **and** either presses the interact key or `autoTrigger=1`.
"Focused" (Pathing) = within `triggerRange`.

| Attribute | On | Type | Default | T/P/M | Notes |
|-|-|-|-|-|-|
| `triggerRange` | M, C | float m | 2.0 | ✔✔✘ | Radius for behaviours, copy, toggle, info (Pathing), focus scripts, routes (TacO). |
| `autoTrigger` | M, C | bool | 0 | ◐✔✔ | **TacO only auto-triggers behaviour 4 (`ReappearAfterTimer`) and `copy`.** Other TacO behaviours need the interact key. Pathing: all interactable behaviours. |
| `behavior` | M, C | int | 0 | ✔✔✘ | See §6. |
| `resetLength` | M, C | s (T: int16, P: float) | 0 | ✔✔✔ | Duration for behaviour 4. TacO is whole seconds ≤ 32767. |
| `hasCountdown` | M, C | bool | 0 | ✔✘✘ | TacO-only. With behaviour 4: shows a countdown instead of hiding. With autoTrigger-like retrigger: see `DrawPOI`. |
| `invertbehavior` | M | bool | 0 | ✘✔✘ | Shown only while the behaviour filter would hide it. Pair it with a marker that shares the GUID. |
| `resetguid` | M | base64 list | — | ✘✔✘ | On trigger, clears behaviour state for those GUIDs. |
| `toggleCategory` | M, C | namespace | — | ✔✔✘ | On trigger, flips a category's user toggle (persisted). **TacO only reads `toggleCategory`**, not `toggle`, despite the wiki. |
| `toggle` | M | namespace | — | ✘✔✘ | Pathing alias of `toggleCategory`. Write `toggleCategory`. |
| `show` / `hide` | M | namespace | — | ✘✔✘ | On trigger, force-enable / force-disable a category (persisted). |
| `copy` | M, C (P: separators too) | string | — | ✔✔✔ | Sets the clipboard on trigger (waypoint chat codes `[&BOAHAAA=]`, build codes). Pathing: debounced 8 s (`copy-attribute-recharge-ms`). User consent: Always / Only manual / Never. |
| `copy-message` | M, C | string | `'{0}' copied to clipboard.` | ✔✔✔ | Toast text. `{0}` = copied value (Pathing). |
| `info` | M, C | string | — | ✔✔✘ | Text on screen while in range. `&#xA;` for newlines. |
| `infoRange` | M, C | float m | 2.0 | ✔✘✘ | **TacO uses infoRange. Pathing ignores it and uses `triggerRange`** (InfoModifier = focus). Writer: keep them equal when the user sets info. |
| `bounce` | M | `bounce`\|`rise` | bounce | ✘✔✘ | Animation when triggered. |
| `bounce-height` | M | float m | 2 | ✘✔✘ | |
| `bounce-duration` | M | float s | 1 | ✘✔✘ | |
| `bounce-delay` | M | float s | 0 | ✘✔✘ | |

## 5. Filters (hide unless condition)

| Attribute | On | Type | T/P/M | Values / notes |
|-|-|-|-|-|
| `achievementId` | M, T, C | int (T: int16) | ✔✔✔ | Hides when the achievement is done (needs an API key with `account`+`progression`). |
| `achievementBit` | M, T, C | int | ✔✔✔ | 0-based index into the API `bits` array. Hides when that bit is done. **Don't trust the wiki/in-game order**; use the API / gw2treasures "Index" column. |
| `festival` | M, T, C | list | ◐✔✔ | `halloween, wintersday, superadventurefestival, lunarnewyear, festivalofthefourwinds, dragonbash`. Shown if *any* listed festival is active. TacO's detection is broken (manual override only). |
| `mount` | M, T, C | list | ✘✔✔ | `jackal, griffon, springer, skimmer, raptor, rollerbeetle, warclaw, skyscale, skiff, siegeturtle`. Shown only while riding one of them. |
| `profession` | M, T, C | list | ✘✔✔ | `guardian, warrior, engineer, ranger, thief, elementalist, mesmer, necromancer, revenant`. |
| `race` | M, T, C | list | ✘✔✔ | `asura, charr, human, norn, sylvari`. |
| `specialization` | M, T, C | int list | ✘✔✔ | Spec id **in the third trait slot only** (MumbleLink limitation). Elites: 5 Druid, 7 Daredevil, 18 Berserker, 27 Dragonhunter, 34 Reaper, 40 Chronomancer, 43 Scrapper, 48 Tempest, 52 Herald, 55 Soulbeast, 56 Weaver, 57 Holosmith, 58 Deadeye, 59 Mirage, 60 Scourge, 61 Spellbreaker, 62 Firebrand, 63 Renegade, 64 Harbinger, 65 Willbender, 66 Virtuoso, 67 Catalyst, 68 Bladesworn, 69 Vindicator, 70 Mechanist, 71 Specter, 72 Untamed. Core lines 1–54 (full table in the wiki `specialization.md`). Newer elites (SotO/JW/VoE) aren't listed in the wiki. Get the ids from `/v2/specializations`. |
| `maptype` | M, T, C | list | ✘✔✔ | `unknown, redirect, charactercreate, pvp, gvg, instance, public, tournament, tutorial, usertournament, center\|eternalbattlegrounds, bluehome\|blueborderlands, greenhome\|greenborderlands, redhome\|redborderlands, fortunesvale, jumppuzzle\|obsidiansanctum, edgeofthemists, publicmini, wvwlounge`. Uses the MumbleLink map type. |
| `schedule` | M, T, C | cron string | ✘✔✔ | UTC. 5-field (minute-resolution) or 6-field (with seconds) Cronos syntax (`* , - / L W ? #`). |
| `schedule-duration` | M, T, C | float min | ✘✔✔ | Required with `schedule`. Visible during [start, start+duration). Examples: `0 0 * * 3` + `1440` = all of Wednesday; `20 2/6 * * *` + `15` = Ley-Line Anomaly (Iron Marches). |
| `raid` | M, T, C | list of raid encounter ids | ✘✔✔ | `/v2/raids` ids, e.g. `twisted_castle,xera`. Hides when cleared this week. |
| `script-filter` | M | Lua call | ✘✔✘ | See §7. |

## 6. `behavior` values

| ID | Name | TacO | Pathing | Effect after trigger |
|-|-|-|-|-|
| 0 | AlwaysVisible | ✔ | ✔ | no behaviour (default) |
| 1 | ReappearOnMapChange | ◐ | ✔ | hidden until map change. **The current TacO `POI::IsVisible` has no case for 1**, so it never hides there. |
| 2 | ReappearOnDailyReset | ✔ | ✔ | hidden until 00:00 UTC |
| 3 | OnlyVisibleBeforeActivation | ✔ | ✔ | hidden permanently |
| 4 | ReappearAfterTimer | ✔ | ✔ | hidden for `resetLength` s. TacO: `hasCountdown` shows a timer instead |
| 5 | ReappearOnMapReset | ✘ | ✘ | not implemented anywhere |
| 6 | OncePerInstance | ✔ | ✔ | hidden until a new shard/instance (TacO keys on MumbleLink `shardId`) |
| 7 | OnceDailyPerCharacter | ✔ | ✔ | hidden for this character until daily reset |
| 8 | OncePerInstancePerChar | ✔ | ✘ | TacO-only (not in wiki) |
| 9 | WvWObjective | (internal) | ✘ | TacO-generated WvW objective markers. Never author it |
| 101 | ReappearOnWeeklyReset | ✘ | ✔ | hidden until Monday 07:30 UTC |

State is keyed by GUID (01 §12). TacO's in-game activation key is `ActivatePOI`. It activates **every** POI within
range with behaviour ≠ 0, plus `toggleCategory` and non-auto `copy`.

## 7. Lua script attributes (Pathing only)

| Attribute | Called with | When |
|-|-|-|
| `script-once` | `(marker, ...args)` | once, after packs finish loading |
| `script-tick` | `(marker, gameTime, ...args)` | every update tick |
| `script-focus` | `(marker, isFocused, ...args)` | entering (true) and leaving (false) `triggerRange` |
| `script-trigger` | `(marker, isAutoTriggered, ...args)` | when triggered |
| `script-filter` | `(marker, ...args)` → bool | every frame while otherwise visible. `true` = **hidden** (it's a filter) |

Value syntax: `FuncName(arg1, arg2)` or `FuncName`. The parser (`Behavior/Script.cs: SplitFunc`) splits on the first
`(`, strips the trailing `)`, and splits args on `,`. **Strings can't contain commas or parens.** Arg typing: `nil`;
contains `.` → float; int; `true`/`false`; else string with optional `'`/`"` quotes stripped. Scripts only run if the
user enabled scripts. Full API in [03-lua-scripting.md](03-lua-scripting.md).

## 8. Trail-specific

| Attribute | On | Type | Default | T/P/M | Notes |
|-|-|-|-|-|-|
| `trailData` | T, C | file | — | ✔✔✔ | `.trl` (01 §8). |
| `texture` | T, C | file | overlay default | ✔✔✔ | Repeated along the trail. |
| `animSpeed` | T, C | float | 1 | ✔✔✘ | Texture scroll speed. 0 = static. Pathing caps it by user setting. |
| `trailScale` | T, C | float | 1 | ✔✔✔ | Width multiplier. Base width = **1.016 m (40 gu)**. Pathing: half-width = 20 gu × trailScale. **TacO: in this source trailScale only changes UV repetition** (`uvStretch`), and the mesh width is fixed at 20 gu half-width. |
| `iswall` | T | bool | 0 | ✘✔✔ | Vertical ribbon (boundary walls). |

## 9. Category-only

`name`, `DisplayName`, `IsSeparator`, `defaulttoggle`, `ishidden` (P/M), `forceExport` (TacO internal). See 01 §5.2.

---

## 10. TacO in-editor field names ↔ XML (for UX parity)

The TacO editor (`MarkerEditor.cpp` `typeParameters[]`, UI in `UI.xml`) shows these groups. Each field has a
*"saved"* checkbox meaning "explicitly set on this item" (otherwise inherited):

| Group | TacO UI label → XML attribute |
|-|-|
| Visual appearance | SIZE → `iconSize` · MINIMAP SIZE → `mapDisplaySize` · MINIMAP FADEOUT → `mapFadeoutScaleLevel` · MINIMUM SIZE → `minSize` · MAXIMUM SIZE → `maxSize` · ICON → `iconFile` · SCALE ON MINIMAP → `scaleOnMapWithZoom` · Color → `color` · ALPHA → `alpha` · FADE NEAR/FAR → `fadeNear`/`fadeFar` · HEIGHT → `heightOffset` |
| Behavior | BEHAVIOR (dropdown 0–9) → `behavior` · ACHIEVEMENT ID/BIT → `achievementId`/`achievementBit` · RESET LENGTH → `resetLength` · DEFAULT TOGGLE → `defaulttoggle` · HAS COUNTDOWN → `hasCountdown` · TOGGLE CATEGORY → `toggleCategory` |
| Ranged behavior | AUTO TRIGGER → `autoTrigger` · TRIGGER RANGE → `triggerRange` · INFO RANGE → `infoRange` · INFO TEXT → `info` · COPIED TEXT → `copy` · COPY MESSAGE → `copy-message` |
| Visibility | VISIBLE ON MINIMAP / FULL MAP / IN GAME → `miniMapVisibility`/`mapVisibility`/`inGameVisibility` · KEEP ON MAP EDGE → `keepOnMapEdge` |
| Trails | ANIM SPEED → `animSpeed` · TRAIL SCALE → `trailScale` · TRAIL FILE → `trailData` · TRAIL TEXTURE → `texture` |

Per-field applicability in TacO's editor (`markerData`/`trailData` flags): size, min/max size, icon, height,
behavior, resetlength, hascountdown, togglecategory, autotrigger, triggerrange, inforange, info, copy,
copymessage and keeponmapedge are marker-only. animspeed, trailscale, trailfile and texture are trail-only.
mapDisplaySize, mapFadeout, scaleWithZoom, color, alpha, fades, achievement and the visibility flags apply to both.
defaulttoggle is category-only.

## 11. Suggested UI grouping for our editor

1. **Basics:** category (type), icon/texture, size, color+alpha, height offset.
2. **Visibility:** in-game / minimap / map toggles, fade near/far (show in metres, store gu), min/max size,
   map size, scale-with-zoom.
3. **Interaction:** trigger range (draw it in-world), auto-trigger, behaviour (+ reset length), copy (+ message),
   info (sync infoRange), toggle/show/hide category pickers.
4. **Conditions** (Blish/Taimi badge): achievement (+ bit picker via API), festival, mount, profession, race,
   specialization, map type, schedule (cron builder + duration), raid.
5. **Blish extras:** rotate, cull, canfade, occlude, bounce, invertbehavior, resetguid, tip-name/description,
   title, scripts.
6. Each field: inherited value greyed + "override" toggle (TacO's saved bit) + overlay-support badges.

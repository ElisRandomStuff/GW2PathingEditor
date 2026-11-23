# 07 — Reference Code Map (where to look if the notes aren't enough)

All repos live in `reference/` (git-ignored; recreate with `scripts/fetch-references.sh`). Pinned commits are in
[README.md](README.md). Paths are relative to each repo root.

## GW2TacO (C++, BoyC): `reference/GW2TacO`
| Topic | File / symbol |
|-|-|
| POI / category / type data structs, behaviours enum | `gw2tactical.h` (`MarkerTypeData`, `POI`, `GW2TacticalCategory`, `POIBehavior`) |
| Attribute read/write (names, types, defaults) | `gw2tactical.cpp` `MarkerTypeData::Read` / `Write` (~L1295–1631) |
| Visibility rules (behaviours, achievements, festivals) | `gw2tactical.cpp` `POI::IsVisible` (~L1945) |
| 3D marker drawing, fades, sizes, info text, copy | `gw2tactical.cpp` `DrawPOI`, `InsertPOI`, `OnDraw` |
| Minimap drawing | `gw2tactical.cpp` `DrawPOIMinimap`. `MumbleLink.cpp` `BuildTransformationMatrix`, `GetMinimapRectangle` |
| Create/delete/activate POI | `gw2tactical.cpp` `CreateNewPOI`, `DeletePOI`, `UpdatePOI` |
| Pack loading, zip, categories, routes, export | `MarkerPack.cpp` (`ImportPOIDocument`, `RecursiveImportPOIType`, `ExportPOIS`, `ExportSavedCategories`, activation data) |
| Pack auto-download list | `MarkerPacks.json`, `MarkerPack.cpp` `FetchMarkerPacks` |
| Trails: recorder, `.trl` IO, mesh, shader, hit test | `TrailLogger.cpp` |
| In-game editor, undo, gizmo | `MarkerEditor.cpp` / `.h`, layout in `UI.xml` (template `markereditor`) |
| Hotkey actions | `GW2TacOMenu.h` `TacOKeyAction`, `GW2TacOMenu.cpp` ~L1150–1300 |
| MumbleLink parsing | `MumbleLink.cpp` |
| XML layer (case rules, number parsing) | `Bedrock/UtilLib/XMLNode.cpp`, RapidXML |
| Sample pack | `POIs/TacOMarkers.taco` |
| Default categories | `categorydata.xml` |

## Blish HUD Pathing (C#): `reference/Pathing`
| Topic | File |
|-|-|
| Attribute parsing helpers (bool/int/float/color/guid/cron) | `Utility/AttributeParsingUtil.cs` |
| Default values | `State/UserResources/Population/{Marker,Trail}PopulationDefaults.cs` |
| Per-attribute population (one file per attribute) | `Entity/StandardMarker[...].cs`, `Entity/StandardTrail[...].cs` |
| Behaviour wiring | `Entity/StandardMarker_Behaviors.cs`, `Behavior/Filter/*.cs`, `Behavior/Modifier/*.cs`, `Behavior/Script.cs` |
| Behaviour state files | `State/BehaviorStates.cs` (`timers.txt`), `State/CategoryStates.cs` |
| 3D marker render (billboard, min/max size, opacity) | `Entity/StandardMarker.World.cs` |
| Trail mesh | `Entity/StandardTrail.World.cs`, `StandardTrail_SetTrailResolution.cs`, `_DouglasPeucker.cs`, `_HermiteCurve.cs` |
| Map projection | `Entity/MapProjection.cs`, `State/MapStates.cs`, `Entity/StandardMarker.MiniMap.cs` |
| Pack loading, `bh-` prefix, pack.lua | `PackInitiator.cs`, `SharedPackCollection.cs` |
| Lua engine & sandbox | `Scripting/ScriptEngine.cs`, `Scripting/PathingGlobal.cs`, `Scripting/Lib/*.cs` |
| Settings | `ModuleSettings.cs` |
| Blish core helpers | `reference/blishhud-core/{ColorUtil,WorldUtil}.cs` (fetched from `blish-hud/Blish-HUD@dev`) |

## TmfLib (C#): `reference/TmfLib`
| Topic | File |
|-|-|
| Element/attribute constants, optimized file names | `PackConstImpl.cs` |
| XML → categories / POIs / trails | `Reader/PackReader.cs`, `Builder/*.cs` |
| `.trl` reader | `Reader/TrlFileReader.cs` |
| Category tree, name sanitising, namespaces | `Pathable/PathingCategory.cs` |
| Inheritance lookup | `Prototype/IAggregatesAttributes.cs`, `AttributeCollection.cs` |
| Optimized pack writer | `Writer/PackWriter.cs`, `PackWriterSettings.cs` |
| Zip path normalisation | `Content/AsyncSafeZipArchive.cs` |
| Prefix stripping / lowercasing | `NanoXml/NanoXmlBase.cs` `CleanName` |

## TaimiHUD (Rust, Nexus/ArcDPS addon): `reference/TaimiHUD`
| Topic | File |
|-|-|
| Nexus registration, render callbacks | `src/exports/nexus/mod.rs` (PreRender = 3D, Render = UI, OptionsRender) |
| Space (3D) engine | `src/space/engine.rs`, `src/space/render_list.rs` |
| D3D11 setup, depth/stencil, scissor | `src/space/dx11/{backend,depth_handler,perspective_handler}.rs`, `space/d3d/**` |
| Game-depth capture ("goggles") | `src/space/goggles.rs` |
| Projection, FOV, depth range | `src/render/machine/space.rs` |
| Shaders | `data/shaders/*.hlsl` (`poi`, `trail`, `map`, `mask`, `textured`) |
| Pack parsing & attribute support | `pathing/pack/src/{pack.rs,loader.rs,attributes/mod.rs,category/*,trail/mod.rs}` |
| Coordinates (map ↔ world ↔ continent), calibration | `pathing/meta/src/coords/*.rs`, `pathing/meta/src/ui/{minimap,worldmap}.rs` |
| Pack sources dirs | `src/settings/source/sources.rs` |
| Squad-marker editor UI (ImGui) | `src/render/edit_marker_window.rs`, `src/marker/format.rs` |

## Nexus / MumbleLink
| Topic | File |
|-|-|
| Current API (v6) | `reference/Nexus-API/Nexus.h` |
| Example addon (older API names) | `reference/GW2-Compass/src/entry.cpp`, `Settings.cpp` |
| MumbleLink structs | `reference/gw2-mumble-cpp/Mumble.h` |
| ImGui 1.80 sources | `reference/GW2-Compass/src/imgui/` |

## gw2pathing.com
| Topic | Path |
|-|-|
| Attribute pages | `reference/gw2pathing-wiki/docs/marker-dev/attributes/*.md` |
| Data types | `.../docs/marker-dev/datatypes/*.md` |
| Lua docs & tutorials | `.../docs/lua-scripting/**` |
| Authoring guide (Metallis) | `.../docs/guides/sywmamp/*.md` |
| Blish settings | `.../docs/usage/*.md` |
| Lua type stubs | `reference/gw2pathing-libdef/library/**` |

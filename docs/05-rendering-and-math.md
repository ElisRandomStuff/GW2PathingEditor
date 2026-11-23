# 05 — Rendering, Projection & Picking Math

Everything needed to draw markers and trails in the world, on the minimap and world map, and to pick or drag them
with the mouse. Sources: TacO `gw2tactical.cpp` (DrawPOI, OnDraw, ProjectTacticalPos), `TrailLogger.cpp` (mesh +
shader), `MumbleLink.cpp` (BuildTransformationMatrix, GetMinimapRectangle), `MarkerEditor.cpp` (UberTool gizmo,
HitTest), Pathing `Entity/StandardMarker.World.cs`, `StandardTrail.World.cs`, `MapProjection.cs`,
`State/MapStates.cs`, GW2-Compass `entry.cpp`, TaimiHUD `src/space/**`, `src/render/machine/space.rs`.

Conventions: GW2/TacO world space is **left-handed, Y up, metres** (01 §6.1). Matrices below use DirectXMath
(row-vector `v * M`, LH).

---

## 1. Camera from MumbleLink

```cpp
using namespace DirectX;
XMVECTOR eye   = XMVectorSet(ML->CameraPosition.X, ML->CameraPosition.Y, ML->CameraPosition.Z, 1);
XMVECTOR front = XMVectorSet(ML->CameraFront.X,   ML->CameraFront.Y,   ML->CameraFront.Z,   0);
XMMATRIX view  = XMMatrixLookAtLH(eye, XMVectorAdd(eye, front), XMVectorSet(0, 1, 0, 0)); // up is always +Y
XMMATRIX proj  = XMMatrixPerspectiveFovLH(MLId->FOV /*vertical, rad*/, (float)NL->Width / NL->Height, zNear, zFar);
```

- `CameraTop` is unused (zero). Everyone uses up = (0,1,0).
- Near/far used in the wild: TacO markers 0.01/1000, TacO trails **0.01/150** (trails beyond 150 m get clipped),
  TacO gizmo 0.05/5000, GW2-Compass 1/10000, TaimiHUD ~0.15 m/~213 m scaled by FOV. Without depth testing against
  the game, near/far only matter for clipping and depth precision. Use something like **0.1 / 5000 m**.
- World → screen (GW2-Compass):
  ```cpp
  XMVECTOR s = XMVector3Project(p, 0, 0, W, H, 0.0f, 1.0f, proj, view, XMMatrixIdentity());
  // s.x, s.y = pixels; reject if camera-space z <= 0 (behind): XMVector3Transform(p, view).z <= 0
  ```
- Skip all world drawing while `Context.IsMapOpen` (fade out over ~250 ms like TacO `GetMapFade`), when
  `!NL->IsGameplay`, or when `UITick` is stale.

## 2. Marker billboards

### 2.1 Position & size
- Anchor = `(xpos, ypos + heightOffset, zpos)`. heightOffset defaults to 1.5 m.
- **World edge length = 2 m × iconSize** (× user scale).
  - TacO projects `camspace` and `camspace + (0.5·size, 0, 0)`, computes half-extent
    `s = |Δndc.x| · viewportWidth` px, draws the rect `center ± s`.
  - Pathing builds a quad of half-extent 0.5 scaled by `Size·2`.
- **Screen clamp:** half-extent `s = clamp(s, minSize, maxSize)`, so the edge is in `[2·minSize, 2·maxSize]` px
  (defaults: never below 10 px, cap 4096 px).
- TacO's icons are screen-aligned squares (no perspective skew). Pathing uses `Matrix.CreateBillboard` (faces the
  camera), or a static rotation when `rotate` is set.

### 2.2 Opacity
```
d     = distance(camera, anchor)        // TacO (gu = metres / 0.0254); Pathing uses player→marker
fade  = (fadeNear < 0 || fadeFar < 0) ? 1 : 1 - clamp((d - fadeNear) / (fadeFar - fadeNear), 0, 1)
if (d > fadeFar && fadeFar >= 0) cull
alpha = color.A/255 · alpha · fade · userGlobalOpacity · mapFade
```
Pathing extras: ×0.5 when the behaviour filter would hide it (edit/preview), ×0.5 in combat (user option),
"fade between character and camera" (shader), user max-opacity and max-view-distance caps.

### 2.3 Draw order & extras
- Sort far → near by camera-space z (TacO `mapPOIs.Sort`), then draw back to front with alpha blending.
- TacO "TacticalIconsOnEdge": markers outside the frustum are pushed onto the screen edge (`ProjectTacticalPos`
  intersects the ray to the marker with the frustum planes).
- TacO draws the distance under each icon (`TacticalDrawDistance`): `charDist` in gu, or `gu·0.0254` m with one
  decimal when metric.

## 3. Trails

### 3.1 Mesh (both TacO and Pathing build a triangle-strip ribbon per section)
```
halfWidth = 20 gu · trailScale = 0.508 m · trailScale           // Pathing. TacO: fixed 0.508 m (trailScale→UV only)
for each section (split at (0,0,0)), optionally resampled so spacing ≤ 20 gu (Pathing) :
  for i in points:
    dir = p[i+1] - p[i]            (TacO uses next - prev, with dir.y = 0 → always flat)
    side = normalize(cross(dir, up))                     // flat ribbon
    side = normalize(cross(dir, cross(dir, up)))         // iswall: vertical ribbon
    if dot(side, lastSide) < 0: flip *= -1               // avoid twisting on sharp turns
    v[2i]   = p[i] - side·halfWidth·flip ; uv = (1, dist/(2·halfWidth))
    v[2i+1] = p[i] + side·halfWidth·flip ; uv = (0, dist/(2·halfWidth))
```
- UV.v grows with distance travelled, so a square texture tiles once per trail width. Scroll V by `time·animSpeed`
  (TacO: `data.x = t_seconds·animSpeed` added to V).
- TacO draws trails **twice** (CW then CCW culling) with the first pass at half RGB, so the underside looks darker.
  Depth test **off**.
- TacO pixel shader extras:
  - far fade on camera-space z vs `GameToWorld(fadeNear/fadeFar)`.
  - **"fade-out bubble"**: ray-sphere test, radius 2 m around `charPos + (0,1,0)`, so the trail goes transparent
    around the character.
  - `color.rgb · texture · (alpha·mapFade·globalOpacity)`. Trail colour alpha is ignored.
- TacO HLSL is in `TrailLogger.cpp` (~L445). Pathing's is compiled into `Pathing/ref/*.mgfx` (not readable here).

### 3.2 Trails on maps
TacO reuses the same mesh with an orthographic "camera" = the map transform of §4, `width2d = mapDisplaySize·0.5`
px (/mapScale if scaleWithZoom), clipped to the minimap rect. Pathing draws polylines simplified with
Douglas–Peucker (`map-trail-douglas-peucker-error`) and can fade segments far above or below the player.

## 4. Minimap & world-map projection

### 4.1 Minimap rectangle on screen (TacO `GetMinimapRectangle`)
```
uiScale = {Small:0.9, Normal:1.0, Large:1.111, Larger:1.224}[uisz]
s       = (W<1024 || H<768) ? min(W/1024, H/768) : 1          // "window too small" scale
w, h    = CompassWidth·uiScale, CompassHeight·uiScale          // TacO pre-multiplies by uiScale
x1 = W - w·s ;  x2 = W
if IsCompassTopRight:  y1 = 1 ;              y2 = h·s + 1
else:                  delta = {Small:33, Normal:37, Large:41, Larger:45}[uisz]
                       y1 = H - h·s - delta·s ; y2 = H - delta·s
```
When the world map is open, the "rect" is the whole screen and the compass fields describe the big map.

### 4.2 Mumble-only transform (TacO `CompassData::BuildTransformationMatrix`)
No web API needed. It assumes 1 continent unit = 24 gu, which holds for normal maps.
```
dgu  = ((x - charX)/0.0254, (z - charZ)/0.0254)          // offset from character, game units
dgu.y = -dgu.y                                             // continent Y grows south
r    = (mapOpen || !IsCompassRotating) ? 0 : CompassRotation
d    = rotate(dgu, r) / 24                                 // → continent units, relative to player
off  = -rotate((MapCenter - Player) · s, r)                // view may be panned away from player
px   = (d + off) · uiScale / MapScale + rectCenter
```
For the world map, use `ignoreRotation=true`, full-screen rect, and the big-map `MapScale` / `MapCenter`.

### 4.3 API-accurate transform (Pathing `MapStates` / `MapProjection`)
Fetch `/v2/maps/{id}` → `map_rect [[x0,y0],[x1,y1]]` (gu, Y north) and `continent_rect [[cx0,cy0],[cx1,cy1]]`
(Y south). Canonical conversion:
```
gx = x_m · 39.37 ; gy = z_m · 39.37
cont.x = cx0 + (gx - x0) / (x1 - x0) · (cx1 - cx0)
cont.y = cy0 + (y1 - gy) / (y1 - y0) · (cy1 - cy0)
px     = rectCenter + rotate((cont - MapCenter) · uiScale / MapScale, r)
```
- Recommendation: start with §4.2 (no network). Add §4.3 with a cached map table when precision matters
  (instances, odd-scaled maps).
- Map icon size = `mapDisplaySize` (/MapScale if `scaleOnMapWithZoom`) · uiScale. Alpha ×
  `(1 - clamp((MapScale - mapFadeoutScaleLevel)/2, 0, 1))` (TacO).
- Skip icons outside the rect unless `keepOnMapEdge` (clamp them to the border).
- TacO "punches a hole": it clears its own 3D layer inside the minimap rect so world markers never draw over the
  compass. In an ImGui implementation, skip or clip world icons overlapping the minimap rect, or draw the minimap
  layer last with an opaque-free clip.

## 5. Picking (mouse → object)

### 5.1 Mouse ray
```cpp
XMVECTOR a = XMVector3Unproject(XMVectorSet(mx, my, 0, 0), 0,0,W,H, 0,1, proj, view, XMMatrixIdentity());
XMVECTOR b = XMVector3Unproject(XMVectorSet(mx, my, 1, 0), 0,0,W,H, 0,1, proj, view, XMMatrixIdentity());
// ray origin a, dir normalize(b - a)
```
TacO builds NDC `(mx/W·2-1, (1-my/H)·2-1, ±1)` and multiplies by `inverse(view·proj)`. That's equivalent.

### 5.2 Markers
TacO records each drawn marker's **screen rect** (`markerPositions[guid]`, plus `markerMinimapPositions`) while
the editor is open. A click inside one selects it, and the last drawn (nearest) wins. Cheap and works for
billboards. Pathing alternatively does ray vs bounding box of the billboard quad.

### 5.3 Trails (TacO `GW2Trail::HitTest`)
For each edge `p[i] → p[i+1]`: closest points between the mouse segment and the edge (`LineSegDist`,
clamped s, t ∈ [0,1]). Hit if `dist² < (trailScale·0.5)²`. Selected vertex = `t < 0.5 ? i : i+1`. Hit point =
closest point on the edge. **Ctrl+click** on a trail inserts a vertex at the hit point (index i+1).

## 6. Move gizmo (TacO "UberTool", `MarkerEditor.cpp` L1261–1884)

- Placed at the marker anchor (with heightOffset) or at the selected trail vertex.
- `scale = |camera - location| · 0.1` keeps a constant screen size.
- **Mirroring:** each axis flips to point toward the camera:
  `mirror = (sign(eye.x-loc.x), sign(eye.y-loc.y), sign(eye.z-loc.z))`. Toggle culling CW/CCW when an odd number
  of axes flip.
- Parts in local space:
  - 3 **arrows** (cylinder shaft r = 0.02 to z = 0.7, cone r = 0.1 to tip z = 1). Each is rotated onto X/Y/Z.
  - 3 **plane squares** (0..0.3)² for XY/XZ/YZ.
  - A ring mesh for rotation exists but is unused.
- Colours: X red, Y green, Z blue, alpha 0.5. Hover or drag turns them yellow (drag alpha 1).
- Hit test in gizmo-local space (`inverse(obj·view·proj)` applied to the mouse NDC points):
  - plane: intersect z = 0, hit if `0<x<0.3 && 0<y<0.3`.
  - arrow: plane through the axis with normal = projected mouse direction; hit if `0<z<1 && x²+y² < 0.15²`.
- Drag: on mouse-down store `clickedOriginalDelta = movePos(original) - original`. On move,
  `pos = movePos(original) - delta`, where `movePos` re-intersects the drag plane or axis. **Right-click while
  dragging cancels** (restores the original). Mouse-up commits one undoable `MoveMarker` / `MoveTrailVertex`.
- Also draws range visualisations: three orthogonal circles (XY, XZ, YZ) of radius `triggerRange` (yellow-ish,
  when behaviour ≠ 0 or route) and `infoRange` (green, when info set). Segments behind the character fade near it.

## 7. Depth / occlusion options

| Approach | Who | Cost |
|-|-|-|
| No occlusion (markers always on top of the world) | TacO, Pathing, ImGui MVP | none |
| Own depth buffer for overlay-only ordering (markers occlude each other, `occlude` attr) | Pathing (`occlude`), TaimiHUD | small |
| Hook `ID3D11DeviceContext::OMSetRenderTargets` to capture the game's depth-stencil view, then depth-test against the world | TaimiHUD `goggles.rs` + `depth_handler.rs` | high: fragile across game patches |

## 8. Constants cheat-sheet

| Constant | Value |
|-|-|
| inches per metre | 39.37 (TacO uses `/0.0254`) |
| game units per continent unit | 24 (approx., per map) |
| default marker world edge | 2 m × iconSize |
| default trail width | 1.016 m (40 gu) × trailScale |
| TacO trail recording step | ≥ 30 gu (0.762 m) |
| Pathing trail resample | 20 gu |
| default triggerRange / infoRange | 2 m |
| default heightOffset | 1.5 m |
| default mapDisplaySize | 20 px |
| default minSize / maxSize | 5 / 2048 px (half-extent) |
| TacO map fade time | 250 ms |
| TaimiHUD default FOV fallback | 50° vertical |

## 9. Our world preview (`src/view.rs`, `src/addon/preview.rs`)

The addon draws the pack being edited with ImGui's **background draw list**: over the game, under every ImGui
window, no depth (row 1 of §7).
- Camera per §1: MumbleLink camera position/front, vertical FOV read from the identity JSON (`"fov"`, re-read every
  60 frames), viewport = `io.DisplaySize`, up = +Y, near plane 0.1 m. Hidden while the map is open
  (`UiState::IS_MAP_OPEN`) and outside gameplay.
- Markers per §2.1–2.3: anchor `ypos + heightOffset`, half-size `iconSize · 1 m` projected, clamped to
  `[minSize, maxSize]`, `alpha` × fade (`fadeNear`/`fadeFar` in inches → metres, camera distance), sorted far →
  near. All attributes are inherited from the categories (`Pack::inherited`). No `iconFile` → the addon's icon.
- Trails per §3.1, simplified: flat quads per segment (no joints), half-width `0.508 m · trailScale`, centre line
  clipped at the near plane, `v` growing one texture repeat per trail width (the ImGui DX11 sampler wraps; the
  texture repeats in game). No texture → a translucent blue ribbon. Without fade distances, segments beyond
  300 m are skipped.
- **ImGui has no perspective-correct texturing:** vertices are 2D, so UVs are interpolated linearly on screen.
  A quad seen along its length (near end wide, far end narrow) shows its texture warped, and the warp changes as
  the camera moves: the texture seems to slide (seen in game, 2026-11-23). Fix: cut each segment into pieces no
  longer than 8 % of their camera depth (at most 32 per segment, `view::trail_pieces`).
- The selected markers get an outline and their number. The selected or recording trail is tinted yellow, the
  selected trail's points are white dots, the selected ones blue.
- The minimap and the world map (§4.1–4.2, TacO's transform, reimplemented in `view::MapView` and unit-tested):
  MumbleLink's compass size, rotation, player/map-centre continent coordinates and map scale describe the minimap
  while the map is closed and the world map while it's open, so only one is drawn at a time. Interface size from
  the identity's `uisz`. Icons `mapDisplaySize` (/`mapScale` if `scaleOnMapWithZoom`) × UI scale, faded by
  `mapFadeoutScaleLevel`, skipped when their centre is outside the rect. Clipped to the rect. Not drawn:
  `keepOnMapEdge`.
- Trails on the maps: flat ribbons along the projected points, half the icon size wide (TacO), at least 4 px, with
  their texture repeating once per ribbon width *measured on screen*, as TaimiHUD does (its world map trails are
  textured by default, its minimap ones untextured). TacO instead repeats the texture every 20 m of trail
  (`uvScale = GameToWorldCoords(20)·0.1`). Pieces under a pixel long are merged, for zoomed-out maps.
- "Hole punch": the world layer is emitted under up to three clip rects around the minimap rect, so nothing of the
  world draws over the compass. Primitives are built once and emitted per rect they overlap.
- `inGameVisibility` / `mapVisibility` / `miniMapVisibility` (inherited, TacO-style `atoi != 0`) decide the layers.
- Not yet: `rotate`, `color`, `animSpeed`, behaviour filters, edge icons. The rotating minimap follows TacO's
  code and matches in game (checked 2026-11-23).


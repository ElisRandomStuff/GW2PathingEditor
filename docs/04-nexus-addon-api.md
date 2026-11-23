# 04 — Nexus Addon API & MumbleLink

Sources: `reference/Nexus-API/Nexus.h` (API **v6**, the current header), `reference/GW2-Compass` (a small official
example addon, written against an **older** header revision with different member names),
`reference/gw2-mumble-cpp/Mumble.h`, TacO `MumbleLink.{h,cpp}`, TaimiHUD `src/exports/nexus/*`.

Nexus (Raidcore) is an addon loader for GW2. It injects via a d3d11 proxy, hooks Present, owns one ImGui context, and
loads `<GW2>/addons/*.dll`. An addon is a DLL exporting one function.

---

## 1. Entry point

```cpp
#include "Nexus.h"
extern "C" __declspec(dllexport) AddonDefinition_t* GetAddonDef();
```

```c
typedef struct AddonDefinition_t {
    uint32_t        Signature;   // unique addon id (see note)
    uint32_t        APIVersion;  // = NEXUS_API_VERSION (6); selects which AddonAPI_t revision you get
    const char*     Name;
    AddonVersion_t  Version;     // {Major, Minor, Build, Revision} uint16 each
    const char*     Author;
    const char*     Description;
    ADDON_LOAD      Load;        // void(AddonAPI_t*)
    ADDON_UNLOAD    Unload;      // void(); optional only with AF_DisableHotloading
    EAddonFlags     Flags;       // AF_None | AF_IsVolatile (disable on game update) | AF_DisableHotloading | AF_LaunchOnly
    EUpdateProvider Provider;    // UP_None, UP_Raidcore, UP_GitHub, UP_Direct, UP_Self
    const char*     UpdateLink;  // e.g. "https://github.com/<user>/<repo>" for UP_GitHub
} AddonDefinition_t;
```

- **Signature:** a unique integer. GW2-Compass uses `17` (Raidcore-assigned). Community convention for addons not
  hosted on Raidcore is a random **negative** number. *Convention, not stated in these sources; confirm on the
  Raidcore Discord/docs before release.*
- Hot reload: Nexus can unload/reload at runtime unless `AF_DisableHotloading`. `Unload` must deregister
  **everything** (render callbacks, keybinds, events, wndproc, quick access, close-on-escape) and free GPU objects.
- `AF_IsVolatile` is for addons that rely on game memory offsets. We read only MumbleLink, so we don't need it.

## 2. `AddonAPI_t` (v6) members

| Group | Member | Signature / notes |
|-|-|-|
| Renderer | `SwapChain` | `IDXGISwapChain*` → `GetDevice(__uuidof(ID3D11Device))` for our own D3D11 resources |
| | `ImguiContext`, `ImguiMalloc`, `ImguiFree` | Must call `ImGui::SetCurrentContext((ImGuiContext*)ctx)` and `ImGui::SetAllocatorFunctions((void*(*)(size_t,void*))malloc, (void(*)(void*,void*))free)` in Load |
| | `GUI_Register(ERenderType, GUI_RENDER)` / `GUI_Deregister(GUI_RENDER)` | `RT_PreRender`, `RT_Render`, `RT_PostRender`, `RT_OptionsRender` (see §3) |
| | `GUI_SendAlert(msg)` | short toast |
| | `GUI_RegisterCloseOnEscape(windowName, bool*)` / `GUI_DeregisterCloseOnEscape(name)` | Esc sets `*bool=false` for that ImGui window |
| Updater | `RequestUpdate(signature, url)` | for `UP_Self` |
| Logging | `Log(ELogLevel, channel, msg)` | `LOGL_CRITICAL=1 … LOGL_TRACE=5` |
| Paths | `Paths_GetGameDirectory()` | `<GW2>` |
| | `Paths_GetAddonDirectory(name)` | `<GW2>/addons` or `<GW2>/addons/<name>`. Create it yourself |
| | `Paths_GetCommonDirectory()` | `<GW2>/addons/common` |
| Hooks | `MinHook_Create/Remove/Enable/Disable` | shared MinHook instance |
| Events | `Events_Raise(id, data)`, `Events_RaiseNotification(id)`, `Events_RaiseTargeted(sig, id, data)`, `Events_RaiseNotificationTargeted(sig, id)` | cross-addon messaging |
| | `Events_Subscribe(id, cb(void*))` / `Events_Unsubscribe` | builtins: `EV_WINDOW_RESIZED` (null payload), `EV_MUMBLE_IDENTITY_UPDATED` (`Mumble::Identity*`), `EV_ADDON_LOADED` / `EV_ADDON_UNLOADED` (`uint32_t* signature`) |
| WndProc | `WndProc_Register(cb)` / `WndProc_Deregister(cb)` | `UINT cb(HWND, UINT msg, WPARAM, LPARAM)`. Convention: return `0` to consume, return `msg` to pass through (*verify against Nexus docs*) |
| | `WndProc_SendToGameOnly(...)` | bypass other hooks |
| Input binds | `InputBinds_RegisterWithString(id, handler, "ALT+SHIFT+M")` | handler: `void(const char* id, bool isRelease)`. Default string `"(null)"` = unbound. Users rebind in Nexus |
| | `InputBinds_RegisterWithStruct(id, handler, Keybind_t{Key, Alt, Ctrl, Shift})`, `InputBinds_Deregister(id)`, `InputBinds_Invoke(id, isRelease)` | |
| Game binds | `GameBinds_Press/Release/InvokeAsync/PressAsync/ReleaseAsync(EGameBinds)`, `GameBinds_IsBound` | send game keybinds (e.g. `GB_MiscInteract=65`, `GB_SquadMarkerPlaceWorld1..8`) |
| DataLink | `DataLink_Get(id) → void*`, `DataLink_Share(id, size) → void*` | `DL_MUMBLE_LINK` (`Mumble::Data*`), `DL_MUMBLE_LINK_IDENTITY` (`Mumble::Identity*`, parsed JSON), `DL_NEXUS_LINK` (`NexusLinkData_t*`) |
| Textures | `Textures_Get(id)`, `Textures_GetOrCreateFromFile/Resource/URL/Memory(...)` (sync, may return null until ready) | `Texture_t { uint32 Width, Height; void* Resource /*ID3D11ShaderResourceView*/ }`. Nexus owns lifetime: **don't free** |
| | `Textures_LoadFromFile/Resource/URL/Memory(..., cb(id, Texture_t*))` | async with callback. `FromMemory` is ideal for PNGs pulled out of a zip |
| Quick Access | `QuickAccess_Add(id, texId, texHoverId, keybindId, tooltip)` | icon in the Nexus bar. Clicking invokes the keybind |
| | `QuickAccess_Remove(id)`, `QuickAccess_Notify(id)`, `QuickAccess_AddContextMenu(id, targetShortcutId, render)`, `QuickAccess_RemoveContextMenu(id)` | right-click ImGui menu on a shortcut |
| Localization | `Localization_Translate(id)`, `Localization_TranslateTo(id, lang)`, `Localization_Set(id, lang, str)` | |
| Fonts | `Fonts_Get/Release(id, cb)`, `Fonts_AddFromFile/Resource/Memory(id, size, ..., cb, ImFontConfig*)`, `Fonts_Resize(id, size)` | |

`NexusLinkData_t { uint32 Width, Height; float Scaling; bool IsMoving, IsCameraMoving, IsGameplay; ImFont* Font, FontBig, FontUI; }`.
`IsGameplay` is false in loading screens, character select and cutscenes. Hide the 3D overlay then.

### 2.1 Old vs new member names (GW2-Compass uses the old header)

| Old (Compass) | v6 (`Nexus.h`) |
|-|-|
| `GetResource` | `DataLink_Get` |
| `RegisterRender(ERenderType_Render, f)` | `GUI_Register(RT_Render, f)` |
| `DeregisterRender` | `GUI_Deregister` |
| `RegisterKeybindWithString(id, void(*)(const char*), str)` | `InputBinds_RegisterWithString(id, void(*)(const char*, bool), str)`: **handler gained `isRelease`** |
| `DeregisterKeybind` | `InputBinds_Deregister` |
| `SubscribeEvent` / `UnsubscribeEvent` | `Events_Subscribe` / `Events_Unsubscribe` |
| `LoadTextureFromResource` | `Textures_LoadFromResource` |
| `AddSimpleShortcut` / `RemoveSimpleShortcut` | `QuickAccess_AddContextMenu` / `QuickAccess_RemoveContextMenu` |
| `GetAddonDirectory` | `Paths_GetAddonDirectory` |
| `EAddonFlags_None`, `EUpdateProvider_GitHub` | `AF_None`, `UP_GitHub` |

## 3. Render callbacks and where to draw what

| Type | Use for |
|-|-|
| `RT_PreRender` | **Our own D3D11 drawing of 3D markers and trails**, before Nexus renders its ImGui frame, so the UI stays on top. TaimiHUD does all "space" rendering here (`unsafe_render_pre`) |
| `RT_Render` | ImGui windows (editor panels, gizmo overlays drawn with `ImGui::GetBackgroundDrawList()` / `GetForegroundDrawList()`). GW2-Compass draws its in-world N/E/S/W letters by projecting 3D points with DirectXMath and drawing text into the background draw list. That's the cheap route for icons too |
| `RT_PostRender` | after ImGui. Rarely needed |
| `RT_OptionsRender` | inside our tab in the Nexus options window (settings UI) |

Two rendering strategies for markers:
1. **ImGui-only (MVP):** project each marker to screen (§5 of doc 05) and `AddImage` a quad into the background
   draw list, sorted far to near, sized per iconSize/minSize/maxSize. No depth testing against the world, and trails
   become screen-space polylines. Simple and robust.
2. **D3D11 in PreRender (TaimiHUD/TacO style):** own vertex/pixel shaders, constant buffer (view, proj, player pos),
   billboarded instanced quads, trail ribbon meshes. Save and restore the device-context state around our draws.
   Occluding by the game's depth buffer requires hooking `OMSetRenderTargets` to find the game's depth view
   (TaimiHUD "goggles", `src/space/goggles.rs`). That's advanced and optional. Neither TacO nor Pathing occlude
   against the world.

## 4. ImGui version

Nexus addons build against **Dear ImGui 1.80** (GW2-Compass vendors 1.80. TaimiHUD's Nexus build uses its `imgui180`
shim, while its newer ArcDPS path uses 1.92). **Compile against the exact ImGui headers matching the loader's
version.** The context is shared, so struct layouts must match. Recheck when Nexus updates.

## 5. Minimal addon skeleton (v6)

```cpp
#include <windows.h>
#include "Nexus.h"
#include "Mumble.h"
#include "imgui.h"

static AddonAPI_t*      API    = nullptr;
static Mumble::Data*    ML     = nullptr;
static Mumble::Identity* MLId  = nullptr;
static NexusLinkData_t* NL     = nullptr;
static AddonDefinition_t Def{};
static bool ShowEditor = false;

static void OnKeybind(const char* id, bool isRelease) {
    if (isRelease) return;
    if (!strcmp(id, "KB_PATHEDIT_TOGGLE")) ShowEditor = !ShowEditor;
}
static void OnIdentity(void* p) { MLId = (Mumble::Identity*)p; }
static void RenderUI() {
    if (!ShowEditor || !NL || !NL->IsGameplay) return;
    if (ImGui::Begin("Pathing Editor", &ShowEditor)) { /* ... */ }
    ImGui::End();
}
static void RenderOptions() { ImGui::Text("Pathing Editor settings"); }

static void Load(AddonAPI_t* api) {
    API = api;
    ImGui::SetCurrentContext((ImGuiContext*)api->ImguiContext);
    ImGui::SetAllocatorFunctions((void*(*)(size_t, void*))api->ImguiMalloc,
                                 (void(*)(void*, void*))api->ImguiFree);
    ML   = (Mumble::Data*)api->DataLink_Get(DL_MUMBLE_LINK);
    MLId = (Mumble::Identity*)api->DataLink_Get(DL_MUMBLE_LINK_IDENTITY);
    NL   = (NexusLinkData_t*)api->DataLink_Get(DL_NEXUS_LINK);
    api->Events_Subscribe(EV_MUMBLE_IDENTITY_UPDATED, OnIdentity);
    api->InputBinds_RegisterWithString("KB_PATHEDIT_TOGGLE", OnKeybind, "ALT+SHIFT+P");
    api->GUI_Register(RT_Render, RenderUI);
    api->GUI_Register(RT_OptionsRender, RenderOptions);
    api->GUI_RegisterCloseOnEscape("Pathing Editor", &ShowEditor);
    CreateDirectoryA(api->Paths_GetAddonDirectory("PathingEditor"), nullptr);
}
static void Unload() {
    API->GUI_DeregisterCloseOnEscape("Pathing Editor");
    API->GUI_Deregister(RenderOptions);
    API->GUI_Deregister(RenderUI);
    API->InputBinds_Deregister("KB_PATHEDIT_TOGGLE");
    API->Events_Unsubscribe(EV_MUMBLE_IDENTITY_UPDATED, OnIdentity);
    ML = nullptr; MLId = nullptr; NL = nullptr;
}

extern "C" __declspec(dllexport) AddonDefinition_t* GetAddonDef() {
    Def.Signature   = 0;              // TODO: pick/obtain a unique id
    Def.APIVersion  = NEXUS_API_VERSION;
    Def.Name        = "Pathing Editor";
    Def.Version     = {0, 1, 0, 0};
    Def.Author      = "Eli";
    Def.Description = "Create and edit TacO/Blish marker packs in game.";
    Def.Load        = Load;
    Def.Unload      = Unload;
    Def.Flags       = AF_None;
    Def.Provider    = UP_None;        // UP_GitHub + UpdateLink once published
    Def.UpdateLink  = nullptr;
    return &Def;
}
```

Language alternative: TaimiHUD is Rust on `nexus-rs` (zerthox/nexus-rs; TaimiHUD uses a fork on Codeberg).
**We chose Rust** (2026-11-23); the Rust specifics are in §7.

---

## 6. MumbleLink

GW2 writes a Mumble "Link" shared-memory block every frame (`CreateFileMapping("MumbleLink")`. GW2's `-mumble <name>`
argument changes the name; TacO supports that). Nexus maps it and hands us `Mumble::Data*` (`DL_MUMBLE_LINK`), plus a
pre-parsed identity.

### 6.1 Layout (`gw2-mumble-cpp/Mumble.h`, matches TacO's `LinkedMem`)

```cpp
struct Vector3 { float X, Y, Z; };
struct Data {                    // total ≈ 5460 bytes
  uint32_t UIVersion;            // 2
  uint32_t UITick;               // increments each game frame; unchanged ⇒ not in game (loading/char select)
  Vector3  AvatarPosition;       // metres, map-local, Y up   ← what TacO stores as a new marker's position
  Vector3  AvatarFront;          // character facing (unit)
  Vector3  AvatarTop;            // (unused)
  wchar_t  Name[256];            // L"Guild Wars 2"
  Vector3  CameraPosition;       // metres
  Vector3  CameraFront;          // unit view direction
  Vector3  CameraTop;            // (unused, 0) — TacO/others always use up=(0,1,0)
  wchar_t  Identity[256];        // JSON (§6.3)
  uint32_t ContextLength;        // always 48
  Context  Context;
  wchar_t  Description[2048];
};
struct Context {                 // GW2-specific "context" blob
  uint8_t  ServerAddress[28];    // sockaddr_in / sockaddr_in6
  uint32_t MapID;                // GW2 API map id
  uint32_t MapType;              // EMapType (header declares uint8_t; padding keeps layout identical)
  uint32_t ShardID;              // used by TacO for behavior 6 "once per instance"
  uint32_t InstanceID;
  uint32_t BuildID;
  uint32_t UIState;              // bit0 IsMapOpen, bit1 IsCompassTopRight, bit2 IsCompassRotating,
                                 // bit3 IsGameFocused, bit4 IsCompetitive, bit5 IsTextboxFocused, bit6 IsInCombat
  uint16_t CompassWidth, CompassHeight;   // px (before UI scale; TacO multiplies by its UI scale)
  float    CompassRotation;      // radians (only meaningful if compass rotation enabled)
  float    PlayerX, PlayerY;     // continent coords of player
  float    MapCenterX, MapCenterY;// continent coords at centre of the (mini)map view
  float    MapScale;             // map zoom: continent units per pixel-ish (see doc 05 §6)
  uint32_t ProcessID;
  uint8_t  MountIndex;           // EMountIndex: 0 None,1 Jackal,2 Griffon,3 Springer,4 Skimmer,5 Raptor,
                                 // 6 RollerBeetle,7 Warclaw,8 Skyscale,9 Skiff,10 SiegeTurtle
};
```

`EMapType`: 0 AutoRedirect, 1 CharacterCreation, 2 PvP, 3 GvG, 4 Instance, 5 Public, 6 Tournament, 7 Tutorial,
8 UserTournament, 9 WvW_EBG, 10 WvW_Blue, 11 WvW_Green, 12 WvW_Red, 13 WvW_FortunesVale, 14 WvW_ObsidianSanctum,
15 WvW_EdgeOfTheMists, 16 Public_Mini, 17 BigBattle, 18 WvW_Lounge.

The compass fields describe the **minimap** while the big map is closed, and the **world map** while it is open.
TacO keeps separate `miniMap` and `bigMap` copies, updating whichever is live (`MumbleLink.cpp`).

### 6.2 Parsed identity (`Mumble::Identity`, from Nexus)

```cpp
struct Identity { char Name[20]; EProfession Profession; uint32_t Specialization; ERace Race;
                  uint32_t MapID; uint32_t WorldID; uint32_t TeamColorID; bool IsCommander;
                  float FOV; EUIScale UISize; };
```
`EProfession`: 1 Guardian … 9 Revenant (0 None). `ERace`: 0 Asura … 4 Sylvari. `EUIScale`: 0 Small, 1 Normal,
2 Large, 3 Larger.

Note `Name[20]` is truncated (character names can be longer in UTF-8). Parse the raw JSON yourself if you need the
full name, e.g. for per-character behaviour state.

### 6.3 Identity JSON (raw `Identity` wchar string)

`{"name":"…","profession":1,"spec":27,"race":2,"map_id":50,"world_id":…,"team_color_id":0,"commander":false,"map":50,"fov":0.873,"uisz":1}`.
`fov` is the **vertical** FOV in radians. `spec` = third-slot specialization id. TacO parses `map_id`, `uisz`,
`world_id`, `name`, `fov` with and without a space after the colon.

### 6.4 Practical rules

- "In game" = `UITick` changing **and** `NexusLink->IsGameplay`. Skip rendering otherwise.
- New-marker position = `AvatarPosition` (feet). It jitters slightly while moving. TacO optionally smooths the
  character position over 6 frames in camera space (`averagedCharPosition`).
- Map changes: watch `Context.MapID` (or `EV_MUMBLE_IDENTITY_UPDATED`) and reload per-map data.
- UI scale factor used by TacO: Small 0.9, Normal 1.0, Large 1.111, Larger 1.224.
- Optional RTAPI addon (GW2-RealTime-API, `DL_RTAPI`, see Compass `OnAddonLoaded`) gives exact, non-lagged
  character position and facing. A nice-to-have. Handle its load and unload events.

---

## 7. Rust: `nexus-rs` (what we build on)

Read from `zerthox/nexus-rs` tag `0.12.0` (crate `nexus`, edition 2024) and `zerthox/gw2-mumble-rs` `0.3.2` on
2026-11-23. Not cloned into `reference/`; cargo fetches them.

### 7.1 Crate and features
- `nexus = { git = "https://github.com/zerthox/nexus-rs", tag = "0.12.0", features = ["mumble"] }`.
  `mumble` pulls `gw2_mumble` for `DL_MUMBLE_LINK`. Default features `panic`, `panic_trace`, `panic_msgbox` install a
  panic hook that logs and shows a message box. `log` routes the `log` crate into the Nexus log.
- ImGui is `arcdps-imgui` 0.8.0 (imgui-rs 0.8 API over **Dear ImGui 1.80**, `tables-api` enabled), re-exported as
  `nexus::imgui`. Its `-sys` crate compiles imgui + cimgui C++ with the `cc` crate and defines
  `IMGUI_DISABLE_WIN32_FUNCTIONS`, so our copy of ImGui has no clipboard code of its own. `ui.set_clipboard_text`
  calls the `io.SetClipboardTextFn` that **Nexus** set on the shared context. *Unverified in game.*
- `windows` 0.62 comes in transitively.

### 7.2 Entry point
```rust
nexus::export! {
    name: "Pathing Editor",          // default: CARGO_PKG_NAME
    signature: -0x24dc02d4,          // required
    load, unload,                    // fn() each
    flags: AddonFlags::None,
    provider: UpdateProvider::None,  // GitHub: update_link defaults to Cargo.toml `repository`
}
```
- `Author` and `Description` come from `Cargo.toml` (`authors`, `description`). The version comes from the crate
  version.
- The macro's load wrapper sets the ImGui context and allocators (`globals::init`) before calling `load`.
  `init` panics if run twice, which is fine because hot reload unloads the DLL.
- Registration functions return a `Revertible`. `.revert_on_unload()` queues the deregistration, which runs after
  our `unload`. `register_close_on_escape` and textures are **not** revertible: deregister close-on-escape in
  `unload` yourself.

### 7.3 Callbacks and threads
- `render!(f)` / `keybind_handler!(f)` wrap a **non-capturing** `fn` into an `extern "C-unwind"` callback. State
  therefore goes in statics.
- Render callbacks get a thread-local `imgui::Ui` (`with_ui`), so they're render-thread only. Keybind handlers
  fire on press *and* release (`is_release`). Which thread they run on is undocumented (probably the WndProc
  thread), so we only set an atomic flag there and act on it in the next frame.
- `register_close_on_escape(name, &mut bool)` keeps the raw pointer. Use a `static AtomicBool` and pass
  `&mut *FLAG.as_ptr()`.
- Keybind default `"(null)"` registers it unbound. `localization::set_translation(id, "en", text)` should give
  keybinds a readable name in Nexus' list. *Unverified that Nexus shows translations there.*

### 7.4 MumbleLink through `gw2_mumble`
- `data_link::get_mumble_link() -> Option<MumblePtr>`. `MumblePtr` reads fields one at a time with
  `read_volatile` (`read_avatar().position`, `read_map_id()`, `read_map_type()`, `read_camera()`, ...).
- **Avoid `read_context()` and `read()`:** they read the whole `Context`, including `mount_index`, into a Rust
  `enum Mount`. An unknown mount id from a future game update would be undefined behaviour. Read single fields
  instead.
- `gw2_mumble` uses `std::os::windows`, so it only compiles for Windows. Keep platform-independent logic out of the
  addon module so it can be unit-tested on Linux.
- `data_link::read_nexus_link()` returns `NexusLink` (`is_gameplay`, sizes, fonts).

### 7.5 Building from Linux
- Target `x86_64-pc-windows-gnu`, linker `x86_64-w64-mingw32-gcc` (`.cargo/config.toml`). The distro `rust`
  package has no Windows std, so this needs rustup (`rustup target add x86_64-pc-windows-gnu`).
  With rustup, `rust-toolchain.toml` (which lists the target) makes rustup install it on first use in the
  project directory. **Not i686:** GW2 has been 64-bit only since 2019, and a 64-bit process can't load a 32-bit
  DLL.
- The DLL must not import MinGW runtime DLLs (`libstdc++-6.dll`, `libgcc_s_seh-1.dll`, `libwinpthread-1.dll`).
  `scripts/build.sh` checks the imports with `objdump -p`.
  - `-C target-feature=+crt-static` is set, but it does **not** make libstdc++ static. `arcdps-imgui-sys`'s `cc`
    build emits `-lstdc++`, and the linker picks `libstdc++.dll.a`.
  - The only libstdc++ symbols ImGui used were `__cxa_guard_acquire/release` (thread-safe init of function-local
    statics). `.cargo/config.toml` sets `CXXFLAGS_x86_64_pc_windows_gnu=-fno-threadsafe-statics`, so the import
    disappears. That's safe because ImGui only runs on the render thread.
  - Result (2026-11-23, MinGW-w64 GCC 16, UCRT): imports only `kernel32`, `ntdll`, `user32` (panic message box),
    `bcryptprimitives` (getrandom), and the `api-ms-win-crt-*` UCRT forwarders, which ship with Windows 10+. The only
    export is `GetAddonDef`. Release DLL ≈ 1.7 MB.

### 7.6 Textures from files (image previews)

- nexus-rs converts file paths with `path_to_ansi(..).expect(..)` (`ffi/path.rs`). A path the system code page
  can't represent **panics** inside the render callback. Only pass ASCII paths (always convertible), or read the
  bytes yourself.
- `get_texture_or_create_from_*` "may return null until ready" (§2), so Nexus may still read the buffer passed to
  `*_from_memory` after the call returns. Its lifetime isn't documented. Use a `'static` buffer
  (`include_bytes!`) for memory loads.
- The pack window's previews (`src/addon/thumbnails.rs`): `load_texture_from_file` (async) for ASCII paths, then
  `get_texture` each frame. The identifier includes the file's size and mtime, so an edited file gets a new
  texture (Nexus never drops the old one). At most 8 requests per frame. Unverified in game.

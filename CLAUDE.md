# Pathing Editor

A Raidcore **Nexus** addon for Guild Wars 2 to create and edit marker packs (TacO / Blish HUD Pathing / TaimiHUD
format) in game, with an intuitive UI.

- **Read `docs/` first.** It holds compiled research notes on the pack format, attributes, Lua, the Nexus API,
  MumbleLink, rendering math, and how TacO's existing editor works. Prefer the notes over re-reading
  `reference/`. Only go to the code (map in `docs/07-reference-code-map.md`) when a note is missing or marked
  unverified, and update the note afterwards.
- `reference/` holds third-party clones (git-ignored). Recreate with `scripts/fetch-references.sh`.
  Treat it as read-only data.
- Exported packs must follow the "safe writer profile" in `docs/01-marker-pack-format.md` §14.

## Code

Rust, on `nexus-rs` (notes: `docs/04-nexus-addon-api.md` §7). Decisions: `docs/README.md`.

- `src/addon/`: the Nexus glue (registration, keybinds, MumbleLink, ImGui windows, image previews).
  `#[cfg(windows)]` only.
- Everything else in `src/` is platform-independent and must stay that way so it can be unit-tested on Linux.
- `cargo test` runs the tests on the host. `scripts/build.sh` cross-compiles the DLL
  (`x86_64-pc-windows-gnu`, MinGW) and checks that it doesn't import MinGW runtime DLLs.
- Addon data goes in `<GW2>/addons/PathingEditor/` (`captures.xml`, `trails/*.trl`, `images/`,
  `settings.ini`, `import/` and `export/` for archives). No GW2 API calls from the addon.
- No hard-coded keyboard shortcuts in the windows (they clash with players' bindings): use buttons, and Nexus
  keybinds unbound by default (`"(null)"`).
- Paths read from packs (`trailData`, `iconFile`, `texture`) go through `files::resolve`, which keeps them inside
  the addon folder. Don't open them any other way.
- Module map: README "Code layout".

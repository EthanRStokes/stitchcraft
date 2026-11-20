# Stitchcraft

A Scratch-style block editor whose canvas is a Minecraft mod. Drag blocks
together, press Export, and get a buildable Fabric **and** NeoForge mod source
tree — one set of Java, both loaders, on
[FrozenLib](https://github.com/FrozenBlock/FrozenLib)'s cross-platform registry
and event APIs.

Built on [Blockworked/blockstitch](https://github.com/Blockworked/blockstitch)
(`qml` branch): `blockstitch-core` is the document model and every editing
operation, and the Qt 6/QML components are the canvas itself.

## The shape of it

```
crates/blocks    the vocabulary — what a block can say about a mod
crates/export    the compiler   — canvas in, Java + Gradle + resources out
crates/devtools  the test bench — Java, Gradle runs, test games, debugger
crates/app       the editor     — Qt 6/QML, on blockstitch's canvas
```

### Blocks

blockstitch-core asks a host for exactly one thing: an instruction enum
implementing `BlockKind`. That is `McBlock`, and it has three families:

- **declarations** — `register block`, `register item`, `register entity`,
  `register sound`, `register creative tab`. Header-shaped, one per stack.
- **event hooks** — `when a player joins`, `when [block] is right-clicked`,
  `on command /[name]`, `every server tick`, and so on. Also header-shaped; the
  handler is the rest of the stack, the way `when green flag clicked` works.
- **commands and control flow** — everything else, including variables, lists
  and dicts, which blockstitch-core already knows how to rename and reconcile.

How each block *draws* is declared in Rust too (`catalog.rs`) rather than in
QML, so a field renamed on the enum stops the build instead of quietly drawing
an empty slot. The tests check every row against the variant it builds.

Declarations stay short: a fresh `register block` only asks for its id, name,
material and hardness. Everything most mods leave at Minecraft's default
(blast resistance, light, needing the right tool, drops, the item form, the
creative tab; an item's stack size and rarity; a mob's health, speed, attack
and spawn egg) is an optional part added from the block's **+** and removed
with its **×**. A part that is off means the vanilla default.

The palette files blocks by what they act on: **Registry**, **Events**,
**World** (blocks, effects, time and weather, the server), **Players**
(messages, and what you do to players and mobs), then blockstitch's own
Flow, Data and Operators.

### Export

One canvas stack becomes one Java method. Every command becomes a call into a
generated `Rt.java`, which is **the only generated file that touches
Minecraft's own API** — so when a game update renames a method, the fix is a few
lines there rather than every call site the canvas produced.

FrozenLib is what makes one set of sources serve both loaders:

| Job | What it uses |
| --- | --- |
| Registering blocks, items, entities, sounds, tabs | `DeferredRegister` — immediate on Fabric, deferred to NeoForge's own on NeoForge |
| Lifecycle, ticks, joins, block breaks, damage, death | its cross-platform `Event<T>` |
| Entity attributes | `DefaultAttributeRegistry` |
| Client-side renderers | `EntityRendererRegistry` |

What FrozenLib has no answer for is generated per loader, and there is very
little of it: an entrypoint class each, and a four-line command bridge
(`CommandRegistrationCallback` on Fabric, `RegisterCommandsEvent` on NeoForge).

The hooks that are not events in the game at all — a block being right-clicked,
an item being used, one of the mod's own mobs ticking — are dispatched from
generated `ScriptedBlock` / `ScriptedItem` / `ScriptedMob` classes through a
table in `Handlers`.

The exported project is a plain multiloader Gradle build: `common/` is a shared
source directory that each loader adds to its own source sets, so it compiles
twice, once against each loader's Minecraft. No Architectury, no vanilla-only
jar to compile against.

### Dev tools

The toolbar builds and runs the mod without exporting anything by hand:

- **Build jars** compiles ready-to-install Fabric and NeoForge jars; **Save jars…**
  in the console copies them wherever you like.
- **Run** starts a development client, a test server, or both, on Fabric or
  NeoForge (the menu beside it picks). Servers run offline-mode on
  `localhost`, so the client can join; the console takes server commands.
  The first server asks you to agree to the Minecraft EULA.
- **Debug** does the same with the debugger attached: blocks light up as they
  run, right-click a block → **Toggle breakpoint** to pause before it, then
  **Continue** (F8) or **Step** (F10), with the mod's variables, lists and dicts
  shown live. A block that throws is marked on the canvas with the error.

Each run works in its own Gradle project under the app's data folder
(`~/.local/share/stitchcraft/workspaces/<mod id>` on Linux), rewritten only
where the canvas changed, so Gradle's caches stay warm. Exported projects carry
a Gradle wrapper, so neither path needs Gradle installed. Java is the one
requirement: Stitchcraft uses a JDK 17+ from `JAVA_HOME` or `PATH`, or offers
to download Eclipse Temurin 25 for its own use. Gradle itself fetches the JDK
25 that Minecraft 26.3 builds need.

## Building

Needs Rust 1.85+, Qt 6.10+ (a full SDK, with its CMake files and private
headers), CMake 3.19+ and a C++ toolchain (cxx-qt 0.10 requires Qt 6.10).

The window is frameless, the way Blockwork's is: [QWindowKit](https://github.com/stdware/qwindowkit)
(Apache-2.0) moves, snaps and maximizes it, and the toolbar doubles as the title
bar. The first build downloads QWindowKit's source; offline, point
`STITCHCRAFT_QWINDOWKIT_SOURCE_DIR` at a checkout of the pinned commit
(`crates/app/native/CMakeLists.txt`).

Cargo downloads blockstitch (the `qml` branch of
[Blockworked/blockstitch](https://github.com/Blockworked/blockstitch)) by itself on
the first build, and `Cargo.lock` pins the exact commit. Nothing to fetch or copy.
`cargo update -p blockstitch-core -p blockstitch-qml` moves to a newer one.

One-time setup: tell Cargo where Qt is. Copy `.cargo/config.toml.example` to
`.cargo/config.toml` and edit the `QMAKE` path. That file is git-ignored because
the path is specific to your machine. After that, running the editor is just:

```bash
cargo run
```

The config sets `QMAKE` for the build and adds a runner (`scripts/run-with-qt.cmd`)
that puts Qt's DLLs on `PATH` for the run, so nothing has to be set by hand. A bare
`cargo run` and `cargo build` cover just the editor; use `cargo test --workspace`
to test everything.

The logic crates need no Qt at all:

```bash
cargo test -p stitchcraft-blocks -p stitchcraft-export
```

### Exporting without the editor

```bash
cargo run -p stitchcraft-export --bin stitchcraft-export -- project.stitch out/
```

And a worked example that builds a small mod in code, saves the canvas, and
exports it:

```bash
cargo run -p stitchcraft-export --example wonder_blocks -- out
```

## Installing

Install [`just`](https://github.com/casey/just) (`cargo install just`), then, from
the checkout, on Windows, Linux or macOS:

```bash
just install    # build, then install to this machine
just replace    # rebuild, then swap the installed copy for the new build
just uninstall
```

`replace` builds first and only then removes the old copy, so a failed build leaves
the installed app alone.

| | `just install` puts it | Notes |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%\Programs\Stitchcraft`, plus a Start menu entry and an "Installed apps" entry | Per user; no administrator prompt. Qt is copied in with `windeployqt`, so the install does not need Qt. It refuses to replace a copy that is running. |
| Linux | `/usr/local` (`PREFIX=~/.local just install` to change) | Uses `sudo` unless the prefix is writable. Needs Qt 6 (with Quick Controls, SVG) from your distro at run time; `just appimage` builds a self-contained one instead. |
| macOS | `/Applications/Stitchcraft.app` | Self-contained bundle, ad-hoc signed, not notarized. |

Other recipes: `just installer` (Windows Inno Setup installer, needs
[Inno Setup 6](https://jrsoftware.org/isdl.php)), `just msix`, `just appimage`
(Linux), `just bundle` (macOS). Output goes to `dist/`.

### Releases

Every push builds all the packages (`.github/workflows/build-artifacts.yml`) and
keeps them as workflow artifacts for two weeks. Pushing a tag builds them again
and publishes a GitHub release (`release.yml`) with, per platform: a Windows
installer, portable zip and MSIX; a macOS app bundle; a Linux AppImage. The
release notes come from the matching `## [x.y.z]` section of `CHANGELOG.md`.
ARM builds (Windows, Linux) are only attempted when the repository is public,
because GitHub does not offer ARM runners to private ones.

The release workflow can also publish to package managers, each only if its secret
exists: Chocolatey (`CHOCO_API_KEY`), a Homebrew tap `jbcaleb/homebrew-tap`
(`HOMEBREW_TAP_TOKEN`), and winget (`WINGET_TOKEN`; the first version of a winget
package has to be submitted by hand).

The MSIX is unsigned in a release (a real certificate, or the Store, has to sign it),
and its publisher identity in `packaging/msix/AppxManifest.xml` is a placeholder.

## Licensing

Stitchcraft is GPL-3.0-or-later (see `LICENSE`). It links `blockstitch-qml`, which
is GPL-3.0-or-later; `blockstitch-core` is MIT. The mods it exports depend on
[FrozenLib](https://github.com/FrozenBlock/FrozenLib), which is GPL-3.0, so check
its terms before distributing an exported mod.

## What is verified, and what is not

Verified here: 104 tests pass; the whole workspace builds warning-free; the
editor starts, loads its QML and renders with no runtime errors; the exporter
produces a complete, deterministic tree whose Java files all balance their
braces and whose JSON all parses.

The generated Java is compiled against Minecraft 26.3 with FrozenLib 3.0.3:
the example project builds jars on both loaders, and a test server starts on
both with the debugger attached (breakpoints, stepping, variables). Clients
have been built but not played, and nothing has been checked in a real world
beyond the server starting.

## Known limits

- Dragging a block or value onto the sidebar deletes it, along with everything below
  it in the stack; the sidebar turns into a "Drag here to delete" zone while you
  carry one. The block itself is still clipped at the canvas edge on the way in,
  because blockstitch draws it inside the canvas.
- Stitchcraft describes behaviour, not art. Models point at texture paths you
  fill in, and declared entities get a `NoopRenderer` — enough not to crash,
  invisible in game. Every export lists what is missing.
- `while` loops are bounded (`Rt.LOOP_LIMIT`). A handler runs inside the server
  tick, so an unterminated loop would hang the world rather than just spin.
- Variables, lists and dicts are saved server-wide under
  `<game dir>/stitchcraft/<mod id>.json`, not per world — a canvas has no notion
  of which save it belongs to.
- Custom blocks support inputs but not branch callbacks; blockstitch-core's
  `BlockPiece::Branch` is unused.

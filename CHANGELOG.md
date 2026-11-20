# Changelog

Release notes are taken from the section whose heading matches the tag, so a
release `0.1.0` uses the `## [0.1.0]` section below.

## [Unreleased]

- Build jars, run a development client and/or test server, and debug, all from
  the toolbar, with a console for each run. No export or Gradle install needed;
  Stitchcraft can download Java for itself.
- Debugger: blocks light up as they run, breakpoints from a block's menu,
  pause/continue/step, live variables, and errors marked on the block.
- Exported projects include a Gradle wrapper, and now build against Minecraft
  26.3 (FrozenLib 3.0.3, Fabric Loader 0.20.1, NeoForge 26.3.0.113).
- Register blocks are shorter: settings most mods leave at the default are
  optional parts added with the block's "+". Older projects open unchanged.
- New Players category for messages and what you do to players and mobs.
- Coordinates and mob sizes are captioned (x, y, z, width, height).
- Effect levels count from 1, as in game; older projects are converted.
- A frameless window like Blockwork's: the toolbar is the title bar, with its
  own minimize, maximize and close buttons, rounded corners and a shadow on Linux.
- Moved to the shared blockstitch editor and the Blockworked look.

## [0.1.0] - 2026-09-29

First release.

- Scratch-style block canvas (from blockstitch) for describing a Minecraft mod.
- Event-hook header blocks, and blocks that register items, blocks and entities.
- Export to a Fabric and a NeoForge mod using FrozenLib's registries.

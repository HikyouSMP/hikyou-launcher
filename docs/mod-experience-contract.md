# Mod Experience Contract

## Purpose

The user chooses Mods, not dependency versions. The launcher chooses compatible
artifacts, prepares a complete configuration, preserves the previous usable
revision and explains decisions it cannot make automatically.

Fast launch means using a completed revision without provider resolution on the
warm launch path. It does not mean hiding incompatibility or forcing every
management task into a one-dimensional keyboard list.

## Interaction Priorities

- Main search, channel switching and launch remain keyboard-first.
- Occasional management and settings support explicit mouse-driven editing.
- Creation defaults and a running environment's choices are separate surfaces.
- Show readiness separately from a saved request: saved does not mean activated.
- Present incompatibility at the point of decision, with the affected Mods and
  exact target. Do not repeat long Mod lists in every action label.
- Keep the old usable configuration while updates are incomplete.
- Never silently interpret a network failure as an unsupported Mod.
- Recommendations do not override an explicit exclusion.
- World data and configuration are not rolled back with a Mod revision.

## Implemented Scope

[Selection boundaries](mod-selection-boundaries.md) describes the current owners,
creation-only copying, per-environment migration, Fabric validation and Snapshot+
freshness policy. Immutable objects, revisions, validated staging, history and
launch leases remain the underlying mechanisms; they were not replaced by a new
storage framework.

There is no universal proof of gameplay compatibility. Dependency validation,
successful package CI and a real game reaching its title screen are separate
evidence. This change does not claim new macOS/Windows game-launch observations.

## Remaining Work

Contextual template pinning from arbitrary Mod details, richer folder editing,
a redesigned two-dimensional Plus Play collection, persistent choice history and
verified independent world backup/restore remain separate improvements. The
present template editor provides collection/detail columns and virtual folders;
Plus Play retains its existing Mod-list surface.

Policy-library tests cover selection and scoped answers, not the complete
filesystem, provider, recovery and UI implementation.

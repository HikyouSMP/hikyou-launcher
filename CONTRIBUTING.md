# Contributing to Hikyou Launcher

Thanks for taking a look at Hikyou Launcher. The project is early, so focused feedback and small fixes are especially valuable.

## Good Places To Start

- Reproduce launcher issues on Windows or macOS and include logs
- Add tests around path handling, loader metadata, mod install behavior, or launch arguments
- Keep existing workflow modules small and behavior-preserving
- Improve crash log parsing with real-world examples
- Polish keyboard navigation and accessibility
- Improve Modrinth mod or modpack install edge cases
- Improve smart profile reliability and status visibility
- Add launch metrics or crash diagnosis fixtures
- Improve README/docs, screenshots, translations, and issue descriptions

## Refactoring Priorities

The codebase has already been split away from the original large `App.tsx`, `lib.rs`, `mods.rs`, `launcher.rs`, and `crash_parser.rs` shape. Prefer incremental refactors that preserve behavior and keep the current workflow boundaries clear.

1. Add tests before touching launch arguments, loader version JSON merging, Java runtime selection, or path safety.
2. Keep `src/App.tsx` as a composition root. Put workflow behavior in hooks or focused components.
3. Keep command-window keyboard behavior under `src/hooks/navigation/`.
4. Keep provider API details behind provider modules. Modrinth-specific version
   lookup and file selection belong in `src-tauri/src/core/modrinth_provider.rs`.
5. Keep disk mutation for installed jars in `src-tauri/src/core/mod_installer.rs`.
   Resolver code should produce a plan; installer code should commit it.
6. Keep crash parsing facts, rule data, matching, user-facing messages, and
   diagnosis output separated.
7. Keep formatting-only changes separate from behavior changes.

## Issue Seeds

These are small enough to become `good first issue` tickets:

- Add tests for `src/utils/intent.ts` version and loader parsing
- Add tests for profile sorting by `lastLaunchedAt`
- Extract one small repeated UI pattern without changing behavior
- Add a crash log fixture and expected parsed result
- Document the Modrinth modpack install flow with one screenshot
- Add a Japanese/English translation consistency pass for settings labels
- Add a launch metrics fixture or debug-view formatting test
- Add a crash diagnosis fixture for a Fabric dependency conflict

These are better as roadmap issues because they need design discussion:

- Latest+ profile behavior and update policy
- Modrinth install reliability and metadata caching strategy
- Additional mod source providers behind the existing provider boundary
- Java runtime selection policy: Zulu vs Liberica NIK vs user override
- Crash diagnosis rules, evidence quality, and deterministic explanations
- Server-specific profiles and account selection

## Development Setup

Requirements:

- Rust stable
- Bun
- Platform-specific Tauri prerequisites

```bash
bun install
bun run tauri dev
```

Useful checks:

```bash
bun run build
bun run check:version
# Change the app version in package.json and Cargo.toml together.
bun run set:version -- 26.1.0-beta.1
cd src-tauri
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Project Shape

```text
src/                    React + TypeScript UI
src/components/         Panels and reusable UI components
src/hooks/              Shared UI state and Tauri integration hooks
src/hooks/navigation/   Command-window keyboard behavior
src-tauri/src/commands/ Tauri command modules
src-tauri/src/core/     Minecraft manifests, loaders, Java, mods, launch logic
src-tauri/src/core/modrinth_provider.rs  Modrinth API and version selection
src-tauri/src/core/mod_installer.rs      Installed mod jar commit/remove/disable
src-tauri/src/core/launcher_state.rs     Launcher-owned history database
src-tauri/src/auth/     Microsoft/Xbox/Minecraft auth and secure storage
```

## Pull Request Guidelines

- Keep PRs focused. Small compatibility fixes are easier to review.
- Include the platform you tested on.
- Add or update tests when touching shared logic.
- Avoid broad formatting-only changes unless the PR is specifically about formatting.
- Do not include account tokens, local paths, or private server data in logs/screenshots.
- Do not add `.env` files, signing keys, certificates, or generated build output to a PR.
- Follow [SECURITY.md](SECURITY.md) for vulnerability reports; do not use a public issue.
- If a change touches launch, auth, profile creation, mod sync, or path opening,
  include the relevant runtime log path or command trace you used to verify it.

## Issue Guidelines

For launch issues, include:

- OS and version
- Minecraft version
- Loader and loader version
- Java/runtime information if visible in debug
- Relevant game log or crash excerpt
- What you expected to happen

For UX ideas, screenshots or short screen recordings are useful. The launcher is intentionally keyboard-first, so keyboard flow details matter.

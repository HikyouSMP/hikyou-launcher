<div align="center">

# Hikyou Launcher

**A command-native Minecraft launcher for players who want the fastest path from idea to game.**

[![License](https://img.shields.io/badge/license-GPL--3.0-blue?style=flat-square)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-FFC131?style=flat-square&logo=tauri)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-stable-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00?style=flat-square&logo=svelte&logoColor=white)](https://svelte.dev)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square)](#platform-support)

<br />

<img src="docs/assets/launcher-main.png" alt="Hikyou Launcher profile picker" width="760" />

</div>

---

## Why This Exists

Minecraft launchers are often either too heavy, too generic, or too far away from how technical players actually work.

Hikyou Launcher treats launching Minecraft as a direct command surface: press a shortcut, type what you want, launch the right profile, manage mods, inspect logs, and get back to playing. The launcher is built around Minecraft-native concepts from the first pixel: profiles, loaders, modpacks, crash diagnostics, Java runtimes, accounts, and performance diagnostics.

The long-term goal is a launcher that removes routine setup work without hiding power from advanced users: a small native shell, a Rust core, a focused Svelte UI, and enough structure to grow into a serious community project.

## What Works Today

- **Keyboard-first profile launcher** with a compact native command window
- **Independent profiles** with separate game directories, mod sets, memory, and window settings
- **Minecraft version support** through Mojang manifests
- **Loader support** for Vanilla, Fabric, Quilt, Forge, and NeoForge
- **Modrinth integration** for searching and installing mods
- **Modpack install flow** from Modrinth `.mrpack` projects
- **Smart profiles** with Latest+ and Snapshot+ as launcher-managed profiles
- **Mod revisions** with content-addressed storage, exact versions, history, comparison, and restore
- **Managed Mod requests** kept separate from resolved files, with bounded whole-configuration dependency resolution for Fabric
- **Recommended auto mods** prepared in the background; launches use the last completed configuration
- **Microsoft account login** using Hikyou's own application identity, Windows
  WAM, and the Xbox/Minecraft authentication chain
- **Java management** with automatic Zulu updates and per-version runtime selection
- **JVM tuning modes** including Smooth defaults and an opt-in Performance Lab
- **Game log streaming**, a dedicated Log Inspector, structured crash diagnostics, and launch metrics
- **Profile content management** for shaders, resource packs, worlds, data packs, and saved servers
- **SQLite-backed local storage** for disposable API cache data and launcher-owned state history
- **Secure credential storage** selected per platform

## Loader Support

| Loader   | Minecraft versions    |
| -------- | --------------------- |
| Vanilla  | All releases          |
| Fabric   | 1.14 and later        |
| Quilt    | 1.14 and later        |
| Forge    | 1.20.1 and earlier    |
| NeoForge | 1.20.2 and later      |

## Platform Support

| Platform | Priority    | Notes                                        |
| -------- | ----------- | -------------------------------------------- |
| Windows  | Primary     | Windows 11 on supported servicing releases   |
| macOS    | Secondary   | Currently vendor-supported macOS releases    |
| Linux    | Best effort | Maintained desktop distributions             |

Linux requires an up-to-date system `ca-certificates` package, including when using AppImage.

## Security Model

Hikyou stores account tokens outside `settings.json`. The frontend keeps account metadata, while token material is stored through the Rust backend.

| Platform | Storage backend                                                        |
| -------- | ---------------------------------------------------------------------- |
| Windows  | WAM for Microsoft sign-in; TPM-backed NCrypt for offline Minecraft data |
| macOS    | Secure Enclave when available, with machine-bound AES-256-GCM fallback |
| Linux    | machine-id-bound AES-256-GCM plus permission-600 files                 |

Linux support is currently lower priority than Windows and macOS, but token storage is no longer plain text.

## Architecture

```text
src/                         Svelte + TypeScript UI
src/components/              Launcher panels and reusable UI pieces
src/components/library/      Profile content, world, data pack, and server UI
src/hooks/                   UI state, keyboard flow, Tauri integration
src/hooks/navigation/        Command-window keyboard behavior
src-tauri/src/commands/      Tauri command surface
src-tauri/src/core/          Minecraft manifests, loaders, Java, mods, launch logic
src-tauri/src/core/mod_*     Mod providers, installer, recommendations, modpacks, sync state
src-tauri/src/core/crash_*   Crash parsing, rule DB, matching, messages, diagnostics
src-tauri/src/core/{profile_content,worlds,datapacks,servers}.rs
                             Independent local-content ownership boundaries
src-tauri/src/auth/          Microsoft/Xbox/Minecraft auth and secure storage
src-tauri/src/auth/windows_wam/
                             Windows OS authentication interop and cancellation
```

Core choices:

- **Tauri 2** instead of Electron, keeping the launcher small and native
- **Rust** for launch orchestration, downloads, auth storage, caching, and filesystem work
- **Svelte 5 + TypeScript** for the command-first UI
- **Bun** for frontend dependency management and builds
- **SQLite** for local API response caching and launcher-owned state history

The codebase is intentionally split by ownership boundaries rather than by technology alone. Modrinth provider logic, install commit logic, launch orchestration, crash rule data, and UI primitives are separated so contributors can change one behavior without learning the entire app.

Local state is deliberately separated by purpose:

- `caches/cache.db` is disposable provider/API cache data.
- `state/launcher_state.db` is launcher-owned history such as launch metrics and future profile health signals.
- Profile directories own worlds, configuration, and the editable `.minecraft/mods` working tree.
- `meta/mod-objects` stores Mod bytes by content hash; `state/mod-revisions` stores configuration history and prepared generations.

See [Mod objects and revisions](docs/mod-objects.md) for the update, launch, and restore boundaries.

## Build From Source

Requirements:

- Windows 11 x64, a currently supported macOS release, or a maintained Linux x64 desktop
- Rust stable
- Bun
- Platform-specific Tauri prerequisites

The repository contains no signing keys, account credentials, or local
environment files. Keep signing credentials in your local environment only;
they are intentionally excluded by `.gitignore`.

Official releases inject Hikyou's Microsoft application ID in GitHub Actions.
Forks must provide their own `HIKYOU_MSA_CLIENT_ID`; see `.env.example`.

```bash
git clone https://github.com/HikyouSMP/hikyou-launcher.git
cd hikyou-launcher
bun install
bun run tauri build
```

For development:

```bash
bun install
bun run tauri dev
```

## Project Status

Hikyou Launcher is in active development and should currently be treated as a
**pre-release Beta**. The core launcher path is real, but compatibility testing,
especially across older loaders and platforms, is still expanding. Back up
important profiles before trying a pre-release build.

For security reports, see [SECURITY.md](SECURITY.md). Do not post sensitive
security details in a public issue.

Good areas to contribute:

- Minecraft launch compatibility across old and new versions
- Fabric / Quilt / Forge / NeoForge edge cases
- Modrinth mod and modpack install UX
- Smart profile behavior, especially Latest+ / Snapshot+ update policy
- Crash log parsing and human-readable diagnostics
- Launch metrics and performance diagnostics
- Java runtime detection and runtime selection
- Windows and macOS polish
- Accessibility, keyboard navigation, and localization
- Small refactors that keep workflow boundaries clean
- Tests around loader metadata, path safety, mod installs, and launch arguments

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup notes, useful checks, and issue guidelines.

## Roadmap Ideas

- Broader loader dependency-graph support and more actionable explanations when requested Mods cannot be resolved
- Better smart profile update and recovery status surfaces
- Wider cross-version launch regression coverage for Vanilla, Fabric, Forge, NeoForge, and smart profiles
- Server-specific profiles and account selection
- Shared folders and reusable mod groups
- More specific crash explanations with mod names, required dependencies, confidence, and evidence lines
- Deterministic crash diagnosis rules that contributors can improve with real logs and schema-backed fixtures
- Sandboxed extension or script surface for advanced workflows

Deferred work:

- CurseForge integration is intentionally not part of the current product line.
- Local AI crash summarization is not the current direction; deterministic diagnostics are preferred for correctness, privacy, and debuggability.

## Development Process

Hikyou Launcher is developed with extensive use of AI coding agents.

## Support The Project

If this is the kind of launcher you have wanted, the most useful support right now is:

- Star the repository so more Minecraft developers notice it
- Try builds and report exact platform/version issues
- Share screenshots, UX feedback, and launcher workflows you want supported
- Open focused issues for loader, modpack, auth, or Java problems
- Send small PRs with tests, docs, translations, or compatibility fixes

## License

[GPL-3.0](LICENSE)

---

<div align="center">
Built with Rust, Tauri, Svelte, focused platform components, and a bias toward
zero-friction Minecraft launch.
</div>

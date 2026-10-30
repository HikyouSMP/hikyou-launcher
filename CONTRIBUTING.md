# Contributing to Hikyou Launcher

Thanks for taking a look at Hikyou Launcher. The project is early, so focused feedback and small fixes are especially valuable.

## Good Places To Start

- Reproduce launcher issues on Windows, macOS, or Linux and include logs
- Add tests around path handling, loader metadata, mod install behavior, or launch arguments
- Keep existing workflow modules small and behavior-preserving
- Improve crash log parsing with real-world examples
- Polish keyboard navigation and accessibility
- Improve Modrinth mod or modpack install edge cases
- Improve smart profile reliability and status visibility
- Add launch metrics or crash diagnosis fixtures
- Improve README/docs, screenshots, translations, and issue descriptions

## Refactoring Priorities

Prefer focused changes that preserve behavior and keep workflow ownership clear.

1. Add tests before touching launch arguments, loader version JSON merging, Java runtime selection, or path safety.
2. Keep `src/App.svelte` as a composition root. Put workflow state and event ownership in focused `.svelte.ts` modules.
3. Keep command-window keyboard behavior under `src/hooks/navigation/`.
4. Keep provider API details behind provider modules. Modrinth-specific version
   lookup and file selection belong in `src-tauri/src/core/modrinth_provider.rs`.
5. Keep disk mutation for installed jars in `src-tauri/src/core/mod_installer.rs`.
   Resolver code produces a plan; `mod_repository` owns immutable artifacts and
   revisions, while `mod_runtime` orders desired-state edits, preparation and
   recoverable publication. Do not mutate a live configuration from the UI.
6. Keep crash parsing facts, rule data, matching, user-facing messages, and
   diagnosis output separated.
7. Keep formatting-only changes separate from behavior changes.

Follow [AGENTS.md](AGENTS.md) for error, recovery and asset-protection contracts.
Production unwrap/expect/panic is rejected by CI; legacy test-helper debt remains
documented in the [principle audit](docs/principle-audit.md), not a permitted pattern.

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
- Managed Zulu update policy and explicit user overrides
- Crash diagnosis rules, evidence quality, and deterministic explanations
- Server-specific profiles and account selection

## Development Setup

Requirements:

- Rust stable
- Bun
- Platform-specific Tauri prerequisites

To test Microsoft sign-in, copy `.env.example` to `.env` and set
`HIKYOU_MSA_CLIENT_ID` to your own approved public-client registration.

```bash
bun ci
bun run tauri dev
```

Windows calls native WAM through Microsoft's Rust Windows bindings. No C#/.NET
authentication executable or MSAL runtime is built or distributed. Native sign-in
needs device-level testing; CI does not sign in to Microsoft accounts.

Useful checks:

```bash
bun run build
bun run check
bun run check:version
bun run check:privacy
bun run test:scripts
cd src-tauri
cargo check
cargo clippy --all-targets -- -D warnings
cargo clippy --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic
cargo test
```

Frontend dependencies are pinned and installed with `bun ci` in CI. Add packages
only at a clear ownership boundary; review their upstream source, transitive
dependencies, license, and current advisories. Run `bun audit` when updating the
lockfile. A clean advisory report is not proof that a dependency is trustworthy.

Rust HTTP clients must use `crate::http` so new dependencies or request paths do
not silently change certificate trust. Windows/macOS use OS verification; Linux
uses the system CA bundle with webpki. Review TLS defaults, active target features,
and certificate updates when adding networking libraries. Do not bypass failed
certificate verification or fall back to bundled roots.

`bun run test:scripts` includes the Svelte controller regression tests. For the
isolated browser workflows, start `bun run dev` and run `bun run test:ui` with an
installed Playwright runtime. If it is outside this project's module resolution,
set `HIKYOU_TEST_RUNTIME_MODULES` to its `node_modules` directory. An installed
browser can be selected with `HIKYOU_TEST_BROWSER_CHANNEL` (for example `msedge`).
These fixtures do not use real accounts or modify launcher data. Evidence is
written to ignored `.local/frontend-smoke/`; native sign-in still needs a device test.

Use `bun run tauri build` for distributable builds: the wrapper remaps developer
source paths and checks the resulting executable. Direct Cargo builds are not
privacy-checked distribution artifacts. See [runtime boundaries](docs/runtime-boundaries.md).

## Releases

Release preparation is automated so the version commit and Git tag cannot be
created in the wrong order. Replace the example version below with the next
unused version; never rerun a published version or rewrite its tag.

```bash
# Update all version sources, run release checks, and create the local commit.
bun run release:prepare -- 26.1.0-beta.8

# After reviewing the commit, push main and tag that exact commit.
bun run release:publish
```

For an intentional one-command release, including remote publication:

```bash
bun run release -- 26.1.0-beta.8 --publish
```

The scripts require `main`, a clean worktree, aligned version sources, and a tag
that does not already exist on GitHub. Published tags are never replaced
automatically.

## Project Shape

```text
src/                    Svelte + TypeScript UI
src/components/         Panels and reusable UI components
src/hooks/              Shared UI state and Tauri integration hooks
src/hooks/navigation/   Command-window keyboard behavior
src-tauri/src/commands/ Tauri command modules
src-tauri/src/core/     Minecraft manifests, loaders, Java, mods, launch logic
src-tauri/src/core/profile_content.rs  Shader and resource-pack file ownership
src-tauri/src/core/worlds.rs            World discovery and validated ZIP import
src-tauri/src/core/datapacks.rs         Data-pack mutation and world checkpoints
src-tauri/src/core/servers.rs           Loss-preserving servers.dat editing
src-tauri/src/core/modrinth_provider.rs  Modrinth API and version selection
src-tauri/src/core/mod_installer.rs      Installed mod jar commit/remove/disable
src-tauri/src/core/mod_repository.rs     Content-addressed artifacts and revisions
src-tauri/src/core/mod_runtime/          Desired-state edits and recoverable publication
src-tauri/src/core/profile_lifecycle.rs  Non-destructive retirement and restore
src-tauri/src/core/settings_store.rs     Bounded settings reads and atomic replacement
src-tauri/src/core/launcher_state.rs     Launcher-owned history database
src-tauri/src/auth/     Microsoft/Xbox/Minecraft auth and secure storage
src-tauri/src/auth/windows_wam/  Windows OS authentication interop
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

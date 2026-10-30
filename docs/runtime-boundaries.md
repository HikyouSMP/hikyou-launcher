# Runtime Boundaries

## Mod Revisions

`core/mod_intent` owns desired roots, exclusions and resolved-intent records.
`core/mod_repository` owns content-addressed artifacts, immutable revision records,
prepared generations, and atomic published heads. `core/mod_runtime` coordinates
working copies, local validation, background update discovery, edit/use leases,
and recoverable checkout. Provider resolution never runs on the warm Latest+
launch path. See [Mod objects and revisions](mod-objects.md) for the contracts and limits.

`core/mod_solver` plans a whole requested graph without writing partial solutions.
`mods::local_graph` selects compatible nested Fabric candidates for both planning
and final validation; `mod_metadata` owns bounded Fabric JSON compatibility.
`mods::loader_bundle` reads verified loader-owned candidates, not user Mod intent.
`mod_runtime/preparation` owns bounded Minecraft/loader candidate selection;
`mod_runtime/requests` owns durable user changes, and `mod_runtime/record` handles
validated imports. UI snapshots remain derived from these backend records.
`core/native_cache` owns reusable native generations; `core/mod_health` records
observed process outcomes without claiming game compatibility.

## Ownership

- `src/main.ts` loads the Svelte UI for the current window. Auxiliary windows do not load the main application workflow.
- `src-tauri/app_commands.rs` declares command registration and window access in one place. Main-window commands are not automatically available to log or sign-in windows.
- `core/running_processes.rs` owns active launch state and cancellation. `core/game_process.rs` owns the child process, output forwarding, termination, and exit notification.
- `auth/windows_wam.rs` uses Microsoft's Rust bindings to the Windows WAM API. Native requests start on the Rust UI thread with the main HWND; asynchronous completion and Xbox/Minecraft exchange do not block that thread. No .NET, MSAL DLL or authentication helper executable is distributed.
- Interactive sign-in first opens the Windows account picker (`AccountsSettingsPane`), then requests a token from the selected provider without forcing new-account entry. Windows owns the account list; Hikyou does not enumerate or persist it. Picker handlers are removed on completion, cancellation or a dropped future.
- `auth/session.rs` owns the active authentication operation. Cancellation, logout, account switch and credential commits are ordered in the backend. Late or canceled results cannot commit. The frontend receives public account snapshots, never Microsoft/Xbox/Minecraft tokens.
- Account-bound silent acquisition uses an opaque Windows account reference in the existing encrypted credential record. Old hash-only records require one interactive sign-in when refresh is needed. Logout removes Hikyou's credential, not the Windows account. Refresh failures preserve the record so transient network failures do not delete a login.
- An Xbox HTTP 401 rejects the Microsoft token: invalidate that WAM result and retry once for the same account. Other errors do not clear the cache or start an automatic interactive login.

## Application Identity

The canonical application ID in `src-tauri/tauri.conf.json` is
`io.github.hikyousmp.hikyou-launcher`. Repository links use
`https://github.com/HikyouSMP/hikyou-launcher`. A regression test checks the
configuration and documentation and rejects accidental old-organization links.

The former `io.github.hikyou-smp.hikyou-launcher` remains only as an explicitly
legacy macOS Keychain access-group namespace. It is not the new app's bundle ID.
Persisted key tags, TPM key names, HKDF salts and encrypted-record formats are
not renamed: branding changes must not silently orphan credentials. Developer ID
signing and Keychain entitlement migration still need a device-level test before
claiming compatibility with an old Secure Enclave-backed installation.

Launcher-owned data uses the explicit `hikyou-launcher` directory below the OS
data root, independently of Tauri's bundle ID. The rename does not move, scan or
delete profiles, worlds or credentials. Tauri-managed WebView data is keyed by
the bundle ID and may start fresh; it is not the launcher asset store.
The Microsoft OAuth client ID is independent and unchanged.

Windows product name and NSIS uninstall registry identity stay unchanged. The
WiX UpgradeCode is pinned to its previously generated value
`72d4dea6-df92-56f0-bea9-784697c2e0e3`, verified against the pre-rename generated
installer. This preserves its installer-family identity; it does not replace an
executed install/upgrade/uninstall test.

Reference: [Tauri identifier and WiX configuration](https://v2.tauri.app/reference/config/).

## Failure And Recovery

`startup::Gate` publishes one startup result. Application commands are rejected
until required resources are ready. A setup failure produces a privacy-filtered
native dialog and an issue-report destination before exit; framework failures
before a window/event loop exists still return through the process error path.

Mod maintenance distinguishes absent protection state from unreadable, oversized
or malformed state. Unknown state blocks changes without replacing the ready
head. Recommendation edits persist a desired policy and per-profile pending work
before propagation. Retrying the same policy resumes unfinished targets rather
than forgetting the difference. Automatic replay cannot unpin a restored profile;
a fresh explicit policy save is required to override that choice.

Removing a normal profile retires its complete directory into `retired-profiles`
under the launcher data root; it does not purge worlds, configuration or Mods.
The backend holds both edit and play leases and refuses active games/preparation.
The confirmation offers immediate Undo. Restore validates identity and never
overwrites an existing profile. Retired data survives application restarts, but a
persistent retired-profile browser and verified uninstall/reinstall lifecycle
are not implemented. This is retention, not an independent backup.

The unused existing-profile Modpack-overwrite command is not exposed. New-profile
Modpack import remains supported; its interrupted-import recovery is not yet a
complete transactional lifecycle.

`core/settings_store` bounds reads and atomically replaces complete settings.
Only missing settings start from defaults; damaged/unreadable records block
saves. The frontend serializes settings saves, reports errors and offers retry;
after a failed initial read it does not write fallback settings. Pending debounce
changes are not an acknowledged durable commit and can be lost on early exit.
Profile JSON replacements are atomic, but profile enumeration still has legacy
error-reporting gaps. Java manual checks report maintenance/state-write failure
even if a validated installed runtime remains usable for launching.

CI rejects production `unwrap()`, `expect()` and explicit panic error handling.
Legacy tests still contain prohibited helpers; this debt is not a policy exception.
See [principle audit](principle-audit.md) for the bounded audit and remaining work.

## Search Suggestions

Version creation candidates use one explicit release or snapshot channel; they
are never mixed. Tab switches the channel on the main command surface. The saved
channel replaces the legacy `showSnapshots` preference; it is not duplicated in
Settings. Existing profiles and Snapshot+ enablement are separate from this filter.

`src/search/suggestions.json` defines canonical keywords, aliases, token or
whole-query matches, shortcut labels, localization keys and command IDs.
Aliases are exact matches, not partial-word guesses. `mp` or `modpack` followed
by Space at the end of the search field enters Modpack search and consumes the
keyword; IME composition and modified Space shortcuts are not intercepted.
Escape and a fresh Backspace press in an empty Modpack search field share one
exit action. Text deletion, held keys and IME composition do not exit the mode.
The default release channel has no badge; only the snapshot exception is shown.
Mode chips use neutral colors. On Snapshot entry the background expands from its
fixed right edge, and the label slides right-to-left inside the clipped frame.
Layout width stays fixed, and reduced-motion preferences disable both animations.
Both layers share a 250 ms critically damped curve without staggering, sampled
once in `utils/chipMotion.ts` for CSS `linear()`; older engines use a non-bouncing
Bezier fallback. No animation library or per-frame JavaScript is required.
The pure resolver selects the first matching definition;
`SearchSuggestion.svelte` renders it in the footer and emits a known action.
`CaretShortcut.svelte` mirrors browser text layout to place the same shortcut
beside the input caret, not at the field's right edge. It hides during composition,
selection, mid-query editing or insufficient space, leaving the footer available.
Definitions cannot evaluate code, supply URLs or invoke backend commands.

The advanced-mode entry counts seven consecutive brand clicks. After the third,
the footer reports the remaining clicks; two seconds without another click resets
the count. Unlocking still opens the explicit confirmation dialog before saving.

## Development Startup

`bun tauri dev` starts an owned Vite server on an OS-assigned loopback port. After Vite listens,
the wrapper passes its actual URL to Tauri and disables the concurrent dev hook.
Normal exit, startup failure and Ctrl+C close the server. Existing servers are
not reused or terminated. `bun run dev` remains a standalone browser server.

`scripts/tauri-session.mjs` holds an exclusive loopback session listener derived
from the canonical workspace path. This prevents overlapping wrapped dev/build
commands from competing for the same build and development session. The OS releases it on process exit;
no persistent lockfile needs manual removal. A conflicting local endpoint causes
an explicit startup failure. Direct Cargo commands still use Cargo's
own build-directory lock.

Before Windows bundling, `scripts/check-bundle-inputs.mjs` rejects adjacent DLLs.
The current app distributes none; WiX otherwise picks up stale DLLs from older
builds even with an empty resource list. Introducing a real DLL dependency must
update this packaging policy deliberately rather than bypass the check.

## HTTPS Trust

`src-tauri/src/http.rs` owns Rust HTTPS client initialization. Windows and macOS
delegate certificate verification to the OS. Linux loads the system CA bundle
and verifies with webpki; CA updates require restarting the app, not rebuilding
it. No bundled-root or plaintext-HTTP fallback is enabled. Linux does not gain
Windows/macOS-style online revocation checking from this configuration.
Linux installations must provide an up-to-date `ca-certificates` package. Standard
`SSL_CERT_FILE` / `SSL_CERT_DIR` overrides also affect which certificates are trusted.

This policy does not replace the independent TLS handling in Windows WAM, the
WebView, or a launched Java runtime; those components also need security updates.

## Managed Java

Automatic Java selection uses Azul Zulu JREs, without JavaFX or CRaC. Java 16
requirements use Java 17; other required major versions are preserved.
Majors are installed on demand, not all at launcher installation. An absent
Java 17 before a game requires it is expected, not a failed installation.

Settings expose per-generation choices for Java 8, 17, 21 and 25. Empty
`game.javaOverrides` entries retain automatic selection; external runtimes are
user-managed and are not auto-updated. Browsing uses the native file picker and
reads bounded release metadata without executing the candidate. Launches repeat
validation and a time/output-limited version probe, including Forge/NeoForge
installers. Managed files cannot be pinned as external overrides because cleanup
and updates own those paths. Legacy all-version overrides remain a fallback until
cleared in Settings; per-generation choices take precedence. In Settings, an empty
field means automatic management, not a saved path to the currently installed
runtime. Explicitly restoring automatic selection for a generation also bypasses
the legacy override for that generation. Only custom paths appear as input values;
the automatic runtime's product and installed version are shown separately.
Manual paths are validated and saved on Enter or blur; clearing the field restores
automatic selection. Browsing validates the chosen runtime before saving it.

`core/java_catalog.rs` queries Azul's public GA/CA/PSU metadata and verifies the
archive SHA-256 over HTTPS with the shared certificate policy. `java_inventory.rs`
reads installed release versions and selection state without executing every
runtime. `java_updates.rs` coordinates per-major updates with OS file locks.
The Metadata API uses `x64` or `aarch64` filters, not the old Discovery API's
`arch=x86&hw_bitness=64` combination. Response architecture, bitness, OS and Linux
glibc are checked before download. Installed release architecture and the JVM's
reported architecture/64-bit data model are checked before reuse. A previously
downloaded wrong-architecture runtime is replaced even when its update interval
has not elapsed; it is never used as a fallback if repair fails.

Maintenance runs on startup and every minute while the app is running, with a
24-hour metadata-check interval and a one-hour retry after failures. Launches
also check when due. Each major uses one fixed directory, such as `zulu-21`.
Settings also offers a manual check for already installed managed runtimes.
It bypasses the metadata-check interval, not process leases or replacement safety.
It checks metadata even while a runtime is busy, defers replacement until idle,
and does not preinstall missing generations or update user-managed external Java.
Downloads are staged and validated before replacement; a single temporary
previous-directory backup supports rollback and recovery after an interrupted swap.
It is removed after a successful update, not retained as version history.

Shared OS file leases protect launch preparation, installers and running games.
Replacement takes an exclusive lease and checks OS processes for games that
outlived their launcher. Busy runtimes defer updates, retrying after one minute.
Old UUID-named Zulu generations are removed only after a validated fixed-directory
installation is selected and the runtime is idle. Failed cleanup is reported in
DEBUG and retried by maintenance. Cleanup stays inside the managed Java root;
Liberica NIK and unrelated directories are not deleted. NIK and Java 16 are not
auto-selected. "Latest" means the latest published build for that major, not a
promise of continued vendor support.

Reference: [Azul Metadata API](https://docs.azul.com/core/install/metadata-api).

## Archive Dependencies

ZIP support is limited to Stored and Deflate, the standard Minecraft/JAR formats;
password-protected and other compression methods are not supported. `core/archive.rs`
checks all members, supported methods, size limits and payload checksums before
imports write destination files. Unsupported formats report the entry and remedy.

`core/java_archive.rs` owns both ZIP and tar.gz staging, runtime selection and
replacement. It preserves the previous installation until validation succeeds;
internal relative JDK links remain supported, but links that change meaning after
relocation are rejected.

Modpack writes, mod metadata and datapack operations use `core/managed_fs.rs` for
handle-relative I/O. Linked paths and hard-linked read inputs are rejected at
these boundaries; replacement writes do not truncate an outside hardlink peer.
This is scoped filesystem confinement, not an OS sandbox for the entire launcher.

## Privacy

`bun run tauri build` remaps known developer source roots and checks the application
executable for those roots. This is a check for known build paths, not an arbitrary
secret detector. Native C diagnostics also need compiler-specific path trimming;
the artifact check must remain enabled when changing toolchains. Do not distribute
debug symbols or unchecked Cargo output.

Launcher logging redacts the current home and launcher directories. Game logs,
crash reports, and user-supplied messages can still contain identifying data;
review excerpts before sharing them. Fonts are bundled locally rather than
requested from Google at runtime.

## Limits

Window permissions restrict frontend IPC; they do not isolate native code from
same-user malware. WAM and encrypted token storage are not application-identity
or process-injection protection. Package Identity, AppContainer, signing, and
enclave isolation are not implemented by this cleanup.

CI checks Windows, Linux, and both macOS architectures. A successful build or unit
test is not evidence of interactive sign-in or Minecraft launch on another OS.
The check workflow also builds platform packages without publishing them.
`release:prepare` builds local packages before creating the version commit;
other operating systems still require their CI results before publication.

References: [Tauri runtime authority](https://v2.tauri.app/security/runtime-authority/),
[Rust source-path remapping](https://doc.rust-lang.org/rustc/remap-source-paths.html).

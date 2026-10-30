# Mod Objects And Revisions

## Desired State And Resolution

The user's selection is not the installed file list. `state/mod-intents/` stores
requested projects, exact imported local objects, enabled choices and explicit
project exclusions. Accepting a request is local and wakes background preparation;
the request handler does not perform provider access or claim installation succeeded.
Mutations serialize with preparation for the same profile. A failed
update retains both the request and the previous completed head.

Initial recommendations are seeded once per intent. Later discovery of a default
or an alias cannot re-enable a disabled user choice or resurrect an excluded Mod.

Preparation resolves all enabled roots together. Required provider dependencies,
Fabric `depends` / `breaks`, provided IDs, nested jars, Minecraft, Java and loader
predicates constrain the same bounded backtracking search. A selected root cannot
be dropped or disabled to manufacture a solution. Disabled/excluded projects
cannot be enabled transitively. Only dependencies recorded as former transitives
are eligible for automatic removal; unknown legacy jars remain conservative roots.

Directly installed Fabric mods are mandatory. Nested JARs are candidates: shared
copies are deduplicated, one compatible version is selected, and a nested candidate
requires an active parent. Resolution and final local validation share this rule;
unused nested versions do not contribute dependencies or conflicts. Top-level
duplicates remain errors. See [Fabric nested JARs](https://wiki.fabricmc.net/documentation:fabric_loader#nested_jars).

Loader-owned nested mods also participate. Preparation reads the exact Fabric
Loader artifact from the official Maven origin, verifies its published SHA-512
and declared identity/version, and includes its nested candidates. This includes
MixinExtras without hardcoding its version or adding it as a requested project.
Discovery is bounded to 16 MiB per Loader artifact and cached for up to 16 Loader
versions in memory. It does not run on warm launch. Offline unit fixtures inject
their loader graph; the opt-in public-data smoke exercises actual discovery.

Fabric metadata accepts literal control characters inside quoted strings, matching
the loader's JsonReader for that case. A bounded normalization precedes the normal
JSON parser; other JSON syntax, UTF-8 and depth checks are unchanged. Authentication,
settings and revision JSON do not use this compatibility path.

Provider project IDs identify floating managed requirements; filenames and display
names do not. Legacy slugs are verified against the provider before normalization.
A manually supplied/changed jar is an exact SHA-256 local root, not permission to
replace it with a similarly named upstream artifact. Imported local roots do not
automatically update until an explicit managed project is chosen instead.

A fixed local JAR with an unknown provider version cannot prove an exact provider
requirement or exclusion. Preparation blocks as unsupported rather than claiming
incompatibility, guessing an identity from its filename or replacing the object.

Each new revision includes its intent digest and an immutable resolution sidecar
containing the selected wishes and dependency edges. Changing wishes can create a
new revision even with identical jar bytes. Restore recovers those wishes as well
as artifacts. A recovery marker covers the head-publication/bookkeeping boundary;
the next request recovers a committed restore before applying another edit.

This follows the requested-manifest / resolved-lock separation used by package
managers, not Cargo's resolver implementation or SemVer rules. Fabric predicates
have their own semantics. References: [Cargo dependency resolution](https://doc.rust-lang.org/cargo/reference/resolver.html),
[Fabric metadata](https://docs.fabricmc.net/develop/loader/fabric-mod-json).

## Storage And Ownership

- `meta/mod-objects/` stores jar bytes by SHA-256 content ID. Optional upstream `hashes` are provenance, not the local object identity.
- `state/mod-revisions/revisions/` stores immutable camelCase snapshots: profile ref, parent, exact Minecraft/loader target, ordered mod entries, and creation time.
- Entries record filename, object ID, byte size, enabled state, and optional versions/display metadata. `version` is the embedded runtime version, `providerVersionNumber` is the friendly distribution version, and `providerVersion` is an opaque provider ID. Missing metadata remains unknown; it is not inferred as "latest".
- `state/mod-revisions/heads/` publishes a profile's selected revision. History includes only completed, metadata-valid generations; unpublished complete candidates can appear, but pending captures and abandoned staging cannot. Newest is not necessarily head. Listing is not cryptographic verification.
- `state/mod-revisions/generations/<revision>-<nonce>/mods/` contains launch-specific copies. An atomically published locator under `generation-heads/` selects a completed generation. Generations and objects are retained; no automatic garbage collection exists. Disk use grows with changed jar bytes and retained copies.

## Runtime Invariants

- Capture does not change source mods or publish head. Materialization and verification precede publication. Readers are lock-free and use immutable files plus atomic locators; writers take a filesystem lock. Unsafe names/reparse traversal are rejected.
- Fabric launches pin a generation through `-Dfabric.modsFolder`. Preparing a newer revision or updating the editable mods tree does not change the running game's selected mod files.
- Background preparation checks out ready mod content into the editable working tree under the runtime lock. Worlds and game configuration are not replaced.
- Non-Fabric normal profiles use the loader-compatible working mods folder; edits and checkout are rejected while active.
- Normal-profile restore rejects a different Minecraft/loader target. Smart-profile restore pins the selected revision until an explicit launcher Mod edit or Resume. Recipe/metadata changes alone do not clear the pin. Resume clears it and prepares again.
- Restore changes only Mod configuration, never world state or settings. Before opening a world in an older Minecraft version, back it up. Smart restore confirmation always includes this warning because the comparison source is user-selectable, not necessarily the active target.

## Commands And UI

Main-window commands only, registered through `app_commands.rs` so existing permission generation remains authoritative:

- `get_profile_mod_history({ profileId }) -> ModRevision[]`: validates the profile ref and revision ownership; repository reads run in `spawn_blocking`.
- `get_profile_mod_revision_diff({ profileId, fromId, toId }) -> { fromId, toId, fromTarget, toTarget, added, removed, changed }`: both revisions must belong to the requested profile. `changed` contains `{ before, after }`; filename order is deterministic. Exact filenames match first, then unambiguous project/mod identities (or enabled/disabled filename pairs). Ambiguous matches remain additions/removals. Object IDs, versions, enabled state, and metadata participate in comparison. No absolute filesystem paths are returned.
- `restore_profile_mod_revision({ profileId, revisionId }) -> ModRevision`: delegates to the runtime coordinator.
- `resume_profile_mod_updates({ profileId }) -> ModRevision`: smart refs only; delegates to the runtime coordinator.
- `prepare_smart_profile({ profileId }) -> { mcVersion, loader, loaderVersion, revisionId }`: runtime preparation; generation paths remain backend-only.
- `get_profile_mod_requests({ profileId })`: desired requirements, active revision ID, pending flag and a bounded preparation status; no raw provider errors or local paths are persisted in that status.
- `remove_requested_mod({ profileId, requestedKey })`: removes even an unresolved request and records an explicit project exclusion.

Mod management opens history only on deliberate action. The surface shows target, count, timestamp, short revision IDs, exact available versions, compact object hashes (full values in tooltips/accessibility labels), and directional differences. Restore requires confirmation, failures stay visible, and successful restore/resume reloads installed mods. Toggle/remove failures remain visible without discarding the existing list. Browser smoke fixtures use synthetic data and never touch live profiles.

The installed list projects the published head. Pending or blocked wishes appear
separately and remain removable. Events refresh backend snapshots; the frontend
does not optimistically pretend a filename was toggled or a download completed.

## Launch Work

Maintenance runs on launcher startup and while open; requests wake it immediately.
It verifies stored objects off the warm launch path and prepares disposable copies.
The final input signature, recipe and desired digest must still match before a
recoverable checkout publishes the successor. An expired update-check timestamp
does not invalidate a completed generation. First use still requires preparation.

Native-library extraction has a separate completed-generation cache. Warm reuse
checks exact input identities/metadata and hashes output files, but does not reopen
and decompress unchanged native jars. Failed extraction does not replace the
previous receipt or overwrite a running game's native directory.

`state/mod-launch-outcomes/` records bounded factual attempts for a revision:
preparing, spawned, normal exit, failed, stopped or observation interrupted. It
retains the last observed normal exit separately. No token, command line, raw log
or absolute path is stored. Exit code zero is not proof of title-screen arrival
or gameplay compatibility, and never causes automatic rollback.

## Limits

Mod files are limited to 512 MiB per artifact. Larger existing mods are rejected,
not silently omitted. This is an explicit resource/compatibility boundary.
New update discovery is scheduled every 30 minutes with a 5-minute retry backoff;
provider-response caches keep their own expiry. A first preparation/import can
still take time. Warm Latest+ and Fabric launches do not resolve dependencies or
rehash Mod contents; other loaders retain a working-folder use lease.

Smart search considers the newest eight Minecraft versions in its own release or
snapshot channel, plus its current target; each gets three recent stable-first
Fabric loader candidates plus its current loader where applicable. It falls back
only on a proven unsatisfiable result, never on network or search-limit failures.
The graph search is bounded to 128 enabled roots/projects, 64 compatible candidate
versions per project, 20,000 states and 512 MiB prepared candidate bytes. Exhausting
a truncated catalog is unknown, not proof that no compatible version exists.
Known provider dependencies are resolved before unrelated roots. Root selection
uses remaining candidate counts and remembers proven conflicts
between mandatory top-level versions to prune unrelated combinations. Fixed
Minecraft/Java/loader conflicts and provider exact-version conflicts participate
in that pruning; unknown predicates never justify learned exclusions. Fabric
wildcards compare numeric components, including omitted zero components (for
example, `26.3` satisfies `26.3.x`), rather than string prefixes.
See the [Fabric predicate parser](https://github.com/FabricMC/fabric-loader/blob/master/src/main/java/net/fabricmc/loader/impl/util/version/VersionPredicateParser.java).
Nested
choices never produce those learned exclusions. The local nested search separately
limits itself to 256 identity groups and 20,000 states; exhausting either is unknown.
Identity lookup has a 60-second deadline and target preparation a 300-second
deadline. Limits preserve the old head and expose a pending/blocked request.

Launch errors offer a user-reviewed GitHub Issue draft. Only allowlisted error
codes, limit kinds, bounded numeric solver counts and build/OS/architecture are
included. Raw errors, full logs, profile/account identifiers and filesystem paths
are not copied into the draft. Opening GitHub sends the reviewed draft in a URL;
it does not submit an Issue automatically. Copy failures and browser-open failures
remain visible, with a selectable preview as a manual-copy fallback.

The native cache retains up to 32 generations per Minecraft/OS/architecture
context. It does not delete potentially in-use generations automatically; reaching
that exceptional limit requires cache maintenance rather than overwriting them.

Content hashes detect byte changes; they do not establish that a mod is safe, trusted, compatible, or free from malicious behavior. Mods execute arbitrary Java with the game's permissions. Immutable generation selection is not a sandbox for executable mod code. Quick receipt checks detect ordinary filesystem changes, not an attacker with the launcher's own permissions.

Strict Fabric validation handles Java and loader requirements. Full Forge/Quilt dependency-graph validation is unsupported and is not claimed.

# Mod Lifecycle Redesign

Status: partially implemented. The selection and scoped-answer contracts are
connected to the application. The broader planner, choice-history and retention
design below remains a proposal unless listed in the implementation scope here.
The current runtime contract is [Mod objects and revisions](mod-objects.md).

The isolated `hikyou-mod-policy` library implements the selection and scoped-answer
contracts below. Application adapters connect them to saved intents, semantic
commands, background preparation, contextual questions and activation gates.
The artifact store and solver are retained, not replaced by another framework.

## Contract Library

`src-tauri/crates/mod-policy` is an in-process Rust library with no Tauri, filesystem,
network, Java or provider-client dependency. The workspace tests it independently;
the application remains the default build member. It uses existing serde/JSON/SHA-256
dependencies, not an additional solver or database.

- `Policy` composes explicit recommendations and overrides, preserves pins across
  inclusion changes, and produces a per-key desired state with its source.
- Changes return a successor and require an exact revision. Invalid/stale actions
  leave their input unchanged, including after a change-away-and-back sequence.
- `Decision` models pending, deferred, accepted and revoked answers. Acceptance
  checks the displayed policy/facts/baseline; permission checks bind the profile,
  target, named omissions, version policies and material-impact digest.
- Identities are validated bounded values. The application adapter must
  establish canonical provider IDs and existing profile identity before use.
- A decision models omission permission only. It cannot prove the proposed graph
  is compatible, make files ready, authorize world changes or activate a generation.

Validated, schema-versioned checkpoints are available for backend storage. The
frontend cannot construct the policy or decision. It submits an action and the
expected backend snapshot/proposal version. The application keeps existing Intent
records as the single choice authority, with additive action sequences/provenance;
there is no separate writable policy database or destructive migration.

The integrated preparation path first tries the complete requested graph. Only a
proven conflict can start up to 24 group feasibility trials per loader target,
within the existing overall preparation deadline. The displayed omissions are
validated as a complete alternative, not claimed minimal or optimal. Unknown
failures abort; old usable configurations and base wishes remain intact. The
normal-profile target is fixed; smart proposals concern the newest channel target,
not an automatic downgrade. World and configuration files are not part of checkout
or rollback. A composition warning precedes explicit acceptance; no world opens
automatically, and a Mod-history restore is still not a world backup.

Answers and candidate permission records live under `state/mod-decisions/`.
Acceptance is durable before checkout. Interrupted activation resumes only while
the original wishes, baseline, recipe and working files still match. Deferred
questions remain quiet across retries; accepted decisions remain discoverable in
the profile's Mod view. Revocation is saved before further work, and warm launch
and the pre-spawn capability recheck consult authoritative permission. The hashed
revision marker prevents a missing receipt from becoming unrestricted authority.

Remaining architecture work includes a network-free typed planner with immutable
catalog inputs, independent bounded choice history, preview-only reachability GC,
and wider real-world compatibility/performance measurements. These are not implied
by successful contract tests or by the UI integration.

Run the isolated contract tests with:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked -p hikyou-mod-policy
```

The bounded answer-sequence test covers all accept/defer/revoke action sequences
through depth five, including rejected actions as unchanged states: 364 trace
prefixes, not 364 unique states or a proof of the production application.

## Objective

A user selects Mods, not filenames or compatible version combinations. Hikyou
maintains a complete compatible configuration in the background. Launch consumes
an already prepared configuration; it does not perform an update. Every selection,
exclusion, pending change and rollback has an explainable source.

Automation handles mechanical work, not decisions that change what the user gets.
When a verified conflict requires a tradeoff, present concrete alternatives and
obtain a scoped answer instead of hiding it behind permanent modes or generic
errors. Compatible preparation should not require repetitive confirmation.

Success means preserving accepted choices and usable configurations, not making
every combination launch. Incompatible Mod code and missing upstream releases
cannot be repaired by a launcher. Metadata validation is not a gameplay guarantee.

## Integration Map

The existing content-addressed store, immutable revisions, generation pinning and
recoverable checkout are useful foundations, not work to discard indiscriminately.
The main gaps are above and between those mechanisms:

| Boundary | Current behavior | Required change |
| --- | --- | --- |
| Recommendation and choice | Inheritance, explicit overrides and last-action provenance are integrated in Intent | Independent bounded immutable choice history |
| Edit target | Semantic commands use project/local-object identity and an expected state version | Wider integration and real-device acceptance coverage |
| Explanation | Legacy unknown origin, inherited choices, exclusions, pending wishes and active state are projected together | Full historical causal explanation |
| Bootstrap | UI bootstrap initializes recommendations while backend maintenance retries | Backend owns initialized policy and reconciliation readiness |
| Display | One versioned backend projection of desired and active state is integrated | Future presentation changes must preserve its authority |
| Resolution | Provider queries, artifact inspection, graph search and stage mutation share a pipeline | Immutable solver inputs and a typed planning result before publication |
| Semantic identity | Intent/revision hashes include display metadata | Separate compatibility/configuration identity from presentation metadata |
| Retention | Changed artifacts and generations accumulate without GC | Reachability and lease-based retention, initially preview-only |

Code evidence: `mod_runtime/requests.rs` (`current`, `remove_request`,
`change_installed_request`, `recommendation_changes`), `mod_intent.rs`
(`fingerprint`, `verify_selected`), `mod_solver.rs` (`Source`, `Provider`,
`reconcile_with_source_and_runtime`), `commands/mods.rs` (`active_mods`),
`selection.rs`, `decisions.rs`, `ModsPanel.svelte`, and `useAppBootstrap.svelte.ts`.

An excluded project can remain as a disabled entry in the active revision. The
former filename-based command could then fail to find its root and misreport it
as a required dependency. Identity-based edits now cover this reversal even with
no artifact at all. Exclusion preservation and reversal are both regression gates;
neither depends on a project-specific exception or clearing historical choices.

## Domain Model

Do not introduce one large mutable `Mod` object. Give distinct identities to
distinct facts, using validated Rust types and schema-versioned records.

| Entity | Identity and responsibility |
| --- | --- |
| Package | Provider namespace plus canonical project ID; slug/name are lookup/display data |
| Release | Provider namespace plus opaque release ID; contains distribution targeting and dependencies |
| Artifact | Exact bytes identified by SHA-256 and size; a release can offer several artifacts |
| Loaded unit | Loader-specific ID/version, aliases, environment and nested-parent relation inside an artifact |
| Choice commit | Immutable user selection/override changes, parent, bounded action reason and source references |
| Decision and answer | Versioned concrete alternatives and an explicit answer scoped to their consequences |
| Resolution | Exact target, selected artifacts/units, dependency edges and complete input provenance |
| Activation | A verified physical generation and resolution selected by the profile reference |

A project is not a runtime Mod ID. One artifact may contain many loaded units;
different artifacts may provide the same unit. Provider version labels are not
assumed to be SemVer. Exact local imports stay content-addressed and are never
silently converted into an automatically updated provider package.

Presentation metadata is stored separately. Changing an icon or translated name
must not create a new solver problem, rewrite wishes or invalidate a ready launch.
Resolution identity excludes timestamps and history parents; choice history records
those separately. Identical selected bytes and semantics can reuse one resolution.

Version policy is separate from inclusion: `TrackCompatible(channel)` is the normal
managed choice, `PinRelease(releaseKey)` is an explicit fixed provider release, and
`ExactObject(objectId)` preserves a local import. Changing inclusion must not
silently change a pin. A provider release ID alone does not attest bytes; the
resolution records the selected artifact digest and metadata observation. Changed
upstream bytes cannot silently rewrite an existing resolution.

## Policy And Choices

The backend owns an immutable recommendation-policy revision and an immutable
profile choice revision. A profile override has three states:

- `Inherit`: use the recommendation policy; absence of an override means this.
- `Enable`: require the package, regardless of the recommendation default.
- `Exclude`: prohibit the package, including transitive installation.

An inherited disabled/default-off recommendation contributes no root, but it is not
a prohibition on a required dependency. An explicit exclusion is a prohibition.
Removing a custom root returns it to `Inherit`; if the user means to prohibit it
as a dependency, that is the separate exclusion action. Turning a recommended Mod
off creates an exclusion. Returning to recommendations deletes the override, not
the stored artifact. The UI labels these consequences before committing.

Policy evaluation is a pure function:

```text
evaluate(policyRevision, choiceRevision, targetPolicy, environmentFacts)
    -> EffectiveSpec { requiredRoots, exclusions, exactLocalRoots, explanations }
```

Every decision includes its source and applicability reason. Conditional
recommendations retain their conditions; they are evaluated for each target,
not permanently filtered by the first Minecraft/loader version encountered.
An applicable enabled recommendation is a required root, just like an explicit
enabled user root. No solver heuristic may silently demote or omit it. A separate,
explicit target-scoped answer can authorize named omissions without changing the
base choice; such omissions remain visible pending requirements, not exclusions.

No special-case rule forces Sodium back on. The same policy applies to every
package. Changing a global recommendation does not secretly erase a local override.
The snapshot exposes that override and offers an explicit return-to-inheritance
action. Unresolved aliases cannot cause a second choice or use display-name equality
as identity; canonicalization conflicts require a visible migration decision.

Choice commits record the product action and previous choice, not raw UI events,
account identifiers, absolute paths or logs. Migrated choices are labelled
`legacy_unknown`; absence of historical evidence is not invented user intent.
This is bounded configuration history, not a general event-sourcing framework.

## Decisions At Conflicts

Keep ordinary selection and launch simple. Ask only when a real tradeoff is known,
not when a dependency can be added automatically without sacrificing another
choice. Distinguish these cases:

| Situation | Backend result | User action |
| --- | --- | --- |
| Compatible update/addition | Prepare a complete successor | No conflict question |
| New target cannot include all chosen roots | Preserve current activation and derive verified alternatives | Keep/wait, or explicitly advance with the listed omissions |
| Added Mod conflicts with existing choices | Preserve the current activation; explain complete affected root groups | Cancel the addition, or explicitly approve a replacement selection |
| Network/metadata failure, unsupported rule or search limit | Uncertainty with retry/diagnostics | Retry/defer/report, not consent to a supposedly proven incompatibility |

A decision is a backend-owned immutable proposal. It contains an ID, profile,
expected choice/policy versions, baseline activation, target and concrete affected
roots/dependency consequences. Each offered alternative has a structured effect
and feasibility evidence; accepting a string supplied by the frontend cannot
construct an arbitrary plan. Acceptance rechecks the captured versions under the
mutation lock. Changed inputs return a stale-proposal result and refreshed choices.

An alternative may solve a hypothetically revised spec to demonstrate feasibility;
it is not a solution to the original spec or eligible for activation until the
corresponding effect is explicitly accepted. Consent changes the allowed plan,
not the validity rules for its remaining runtime dependencies.

There are two different effects, never interchangeable:

- `ChangeSelection`: an explicit durable choice commit for the exact displayed
  additions/removals. The default never removes another chosen root implicitly.
- `AllowMissingAtTarget`: allow only the named enabled roots to be absent at the
  exact Minecraft target for this profile. Keep those roots in the base wishes,
  with a pending reason and the answer's identity in resolution evidence.

A target-scoped answer is not a permanent preference or permission for another
profile, Minecraft release, newly missing root, changed pin or larger material
impact. Bind it to the omitted requirements' semantic identities and consequence
scope; retain the accepted proposal as provenance. Unrelated cosmetic changes do
not invalidate it. The original proposal's baseline precondition is checked on
acceptance, not used to expire the answer after every same-target update.

If compatible versions arrive, include pending roots automatically only after
the complete graph validates without sacrificing another choice or introducing
an unapproved consequence. Do not promise that an upstream author will release a
compatible version. Newly missing roots or a new target require a new answer.

Waiting/dismissing leaves the current usable activation intact and is durable for
the unchanged question. Background retries do not repeatedly reopen it. Materially
changed options can produce a new proposal. Request IDs and expected versions make
duplicate input idempotent; an old answer cannot authorize a newer proposal.

Revocation is also a backend event, not a UI flag: it invalidates the answer and
pending work relying on it, including late job completions. It does not silently
downgrade Minecraft or reconstruct a missing Mod. Retain the old generation as an
asset, but further launch/publication relying on the revoked exception needs a
fresh explicit decision or a fully satisfying replacement. Explain this pending
state and offer a validated restore without automatically opening older worlds.

Not every conflict has two participants. Show the complete root groups affected
by an option, including shared dependencies, plus a keep-current/cancel action.
Do not claim an alternative is minimal, exhaustive or safest without evidence.
Do not offer a graph that violates runtime constraints merely because the user
accepted omissions. World/content-loss risk and verified backup/restore are
separate gates; an answer to omit a Mod is not permission to delete user assets.

## Ownership And Interfaces

All modules remain in-process Rust components unless isolation has a separate,
demonstrated requirement. No new daemon, service or IPC protocol is required.

| Owner | Inputs and output | Must not do |
| --- | --- | --- |
| Policy reducer | Immutable policy/choices to EffectiveSpec | Read directories, fetch APIs, mutate choices |
| Provider adapter | Typed queries to bounded catalog observations | Decide user policy or write active files |
| Artifact inspector | Verified bytes to versioned loaded-unit facts | Execute Mod code or guess project identity |
| Planner | EffectiveSpec and immutable observations to planning outcome | Network, filesystem mutation, publish partial graphs |
| Decision coordinator | Proven tradeoff and candidate plans to a scoped proposal/answer | Treat missing evidence as consent or broaden the answer's effect |
| Reconciler | Desired inputs to retryable prepare/activation work | Rewrite wishes after a failed search |
| Repository/activator | Immutable records, staged generation, expected refs | Touch worlds/configuration or accept UI paths |
| Projection | One consistent state read to UI snapshot and permitted actions | Infer success from a filename or optimistic UI state |

Provider access and inspection may be incremental: the planner returns
`NeedFacts(querySet)`, the reconciler collects them, then solves again over a new
immutable snapshot. This preserves lazy retrieval without hidden side effects in
the graph engine. Query outcomes distinguish authoritative empty data, unavailable
data, malformed metadata and bounded/truncated results.

## Resolution Contract

The planning result is a tagged type, not error-string prefix matching:

```text
Solved(completePlan, validationEvidence)
NeedFacts(querySet)
Unsatisfiable(conflictExplanation, observedUniverse)
Unsupported(feature, affectedInputs)
BudgetExceeded(limit, counts, pendingQueries)
Unavailable(provider, retryClass)
InvalidInput(validationFailure)
```

Unsatisfiability is relative to an explicitly recorded observed candidate universe.
Exhausting a truncated catalog, an unsupported predicate or a timeout never proves
that a combination does not exist. A newly published release can change that
universe. Failure never becomes a successful smaller/empty Mod set.

The solver input is a loader-neutral finite candidate graph, with loader adapters
responsible for predicates and activation rules. For Fabric, rules include:

- Every accepted root is selected unless an applicable explicit answer covers its
  omission; every omission has that evidence and all active required edges hold.
- Active aliases share their loader-defined identity constraints.
- Nested candidates require an active parent; unselected nested candidates impose
  no dependencies or conflicts. Shared nested copies are not top-level duplicates.
- Minecraft, actual Java major, loader version, client environment and loader-owned
  bundles participate in the same problem.
- Provider dependencies and embedded metadata are separately attributed evidence;
  contradictions are reported, not resolved by silently discarding one source.

Keep the existing engine behind the boundary initially. Evaluate Rust Resolvo
(CDCL) and PubGrub as alternatives on the same fixtures before choosing a dependency.
The adoption gate includes nested-parent/alias encoding, unsupported semantics,
cancellation, limits, license, maintenance and transitive-dependency review. A fast
package solver with incorrect Fabric encoding is not an improvement. An independent
final-plan validator and official Fabric comparisons remain required.

For Latest+, the default preserves all enabled roots and chooses the newest
supported Minecraft release with a complete solution. An explicit scoped advance
answer can choose a newer target with its named pending roots; it must not become
a silent default for future targets. Within a target prefer stable loader/Mod releases with
deterministic provider ordering and identity tie-breaks. Preserve current versions
for unrelated packages on a single-package edit when compatible. Explicit/background
update sweeps consider newer compatible releases. Candidate-window limits are
reported as limits on freshness, never as proof of globally newest compatibility.
Unknown newer-target failures retain the old active configuration without calling
it a completed update. Do not repeatedly redownload candidates across target trials.

## Reconciliation And Publication

One atomic, versioned `ProfileRef` points to its choice commit, active activation,
and update mode. Global policy has its own atomic immutable-revision reference.
Preparation captures both refs, the current activation, environment/algorithm
versions and catalog observations. These form a semantic input fingerprint.

```text
desired input changed -> queued -> collect facts -> solve -> acquire objects
                     -> validate complete plan -> prepare generation -> activate
```

This is a convergence loop, not a durable instruction pointer. A restart observes
the desired/active difference and resumes safe idempotent work. A running operation
has an attempt ID, phase, input fingerprint, bounded error and retry class. Stale
attempts cannot overwrite current status or activate after a newer edit/cancellation.
New input wakes work immediately; unchanged unavailable input backs off.

Activation rechecks captured refs and ownership under a fixed lock order
(global policy before profile). A changed expected sequence makes the candidate
obsolete, never permission to overwrite newer choices. Objects and verified
generation/activation records are durable before the single profile-ref commit.
An acknowledged choice edit is durable independently of a successful update.

Before the commit, a crash leaves the old activation usable. After it, a crash
leaves the new complete activation selected. Post-commit bookkeeping is recoverable
and cannot turn committed success into a second destructive attempt. Each
filesystem's replace/durability behavior needs real all-platform fault tests;
atomic rename alone is not a universal power-loss guarantee.

Fabric launches lease/pin the exact generation. Its editable `mods` folder becomes
an export/import boundary, not an independent desired-state authority. External
changes are captured conservatively and shown as drift; they cannot reinterpret a
missing managed file as a new empty selection. Unknown bytes remain exact assets.
Do not add a watcher that can silently lose events: periodic scans reconcile drift.
Other loaders retain their working-folder lease and journal until an equivalent
generation capability is proven. Do not claim full loader-graph parity prematurely.

Ready state and update state are independent. Launch can use a validated active
generation while an update is queued, blocked or offline; the UI must say it is
using the prior configuration. A damaged active generation blocks launch and offers
repair/restore. An accepted change is never presented as already active.
Ordinary fallback cannot bypass a revoked exception or a separate user-data gate.
Input equality permits skipping only previously completed reusable work; failed,
interrupted or damaged results still require retry/repair with the same wishes.

## History And Retention

History links choice commits, exact resolutions and activations. It can answer
what changed, which root required a dependency, why an older release remains and
why a package is excluded. Restore creates a new choice/history action referencing
the historical immutable resolution; it does not erase later history. Pinned
restore remains explicit until resume/edit, with old-world compatibility warnings.

Worlds, saves, game configuration and unrelated files are outside these mutations.
Mod rollback is not world rollback or a backup of user data.

Retention roots include current and retained history, exact local imports, restore
pins, incomplete transactions, retired profiles and running-generation leases.
Before deleting any object, mark reachable records and report a preview. Unknown
state or surviving-process uncertainty blocks deletion. Start with preview only;
automated GC requires interrupted-GC and restore tests. Use independent copies or
proven copy-on-write clones, not mutable hardlinks into the content store.

## UI Contract

Replace separate installed/request queries with one backend snapshot carrying
policy/choice versions, activation/resolution IDs, target, attempt status and rows.
Rows are the union of applicable recommendations, explicit choices and active
packages. Each row exposes desired state, active version, decision reason and
permitted actions; exclusion does not disappear just because no JAR is active.

Commands are semantic: `SetChoice(packageKey, choice, expectedSequence)`,
`AcceptDecision(proposalId, optionId, expectedSequence)`, `RevokeAnswer`,
`ImportLocalObject`, `Restore`, `Resume`, `CheckUpdates`. A stale snapshot returns
a refresh/retry conflict. UI filenames or paths are never mutation authority.
Local-object actions use validated object references. Dependencies explain their
requesting root; exclusion offers a conflict explanation, not silent root deletion.

Snapshot reads capture policy and profile reference versions under the same lock
order as publication, then project immutable records after releasing the locks.
One response is internally consistent; a later edit still requires its version
precondition. Pending/blocked status applies only to its matching input fingerprint.

The normal surface stays one searchable list. Show an exception only when desired
and active differ; details/history are progressive disclosure. Svelte owns search,
focus and uncommitted drafts, not installed state or reconciliation decisions.

Do not expose generations, planner stages or parallel state lanes as permanent
navigation. Present at most one contextual decision for the current task, with a
short consequence and concrete action labels. Background work must not steal
focus or turn the existing Enter-to-launch action into unexpected consent. A normal
launch command does not implicitly accept omissions or world-risk changes.
Use Hikyou's command-first navigation rather than copying a demo's play-button,
tile or question-card layout. Immediate Undo may be transient, but restore and
answer revocation must remain discoverable by command after the notification ends.

## Performance And Trust

Warm launch must perform zero provider calls, zero dependency solves, zero archive
decompression and zero full-content hashing. It still checks the pinned generation's
receipt/identity and holds its lease; background verification is separate. A timing
target is not permission to skip a missing/damaged state check.

Cache inspections by content digest plus parser/loader-rules version. Cache provider
observations by provider, target and query, with completeness/freshness evidence;
TTL never changes user choices. Share in-flight requests and unchanged artifacts
across profiles, cap concurrency, and reuse graph facts across target attempts.

Benchmark launch-selection latency separately from Java startup/title-screen time.
Initial goal: warm P95 selection under 50 ms for a 100-Mod fixture on a recorded
machine, with disk/cache conditions reported; this is an unmeasured acceptance
target, not a current result or an industry-fastest claim. Track query count,
downloaded/hashed bytes, solver states, peak memory and cancellation latency too.

Provider metadata, filenames and archive members are untrusted. Retain bounded
reads, HTTPS origin policy, digest verification, managed filesystem confinement
and no-code inspection. Content hashes do not certify a Mod as safe. Ordinary
receipt checks and read-only intent do not sandbox same-user malware or Mod code.
Diagnostics use reviewed, bounded structured evidence; no automatic log upload.

## Migration And Gates

1. Add the pure policy model and one explanatory snapshot without changing the
   current writer. Characterize legacy exclusions, disabled artifacts and aliases.
2. Produce shadow choice/resolution records from bounded legacy reads. Preserve
   exclusions as `legacy_unknown`, local bytes and all existing history. Missing
   policy, corrupt policy and intentionally empty policy are distinct cases.
3. Compare effective roots, active artifacts and explanations. Stop on ambiguity;
   do not guess a deletion's origin, restore a default or turn a transitive into a
   managed user request merely because it appears in an old directory.
4. Activate the new writer per profile only after contract/recovery tests. Keep
   legacy records unchanged and one authoritative writer. No dual-write rollout.
5. Add the isolated solver adapter and recorded-catalog replay comparisons, then
   enable background update publication. Enable GC only in a later verified step.

Rollback during shadow mode just discards disposable shadow output. After cutover,
a controlled migration rollback restores the retained legacy selection and exports
new exact assets before changing ownership. Running an older binary against new
state is not assumed safe; binary-downgrade compatibility needs its own test.

Required tests, ordered by impact:

- At every persistence/activation cut point, interrupt/restart and verify that the
  complete old or complete new configuration remains, with world/config sentinels.
- Global enabled + profile excluded + disabled active artifact has a visible reason
  and a working explicit re-enable/inherit action; retry/restart preserves it.
- Default-off versus explicit exclusion, empty policy, first target not applicable,
  canonical alias collision and policy update during an in-flight solve.
- Every solved graph includes all non-waived roots, accounts for each explicitly
  waived root, obeys exclusions, and passes independent exhaustive small-graph
  and official Fabric predicate/nested-graph comparisons.
- Any target-scoped omission requires matching answer evidence; rejection, a
  changed profile/target/pin, enlarged impact or revocation cannot reuse it. An
  unchanged pending question does not reopen on every retry/restart. Newly available
  roots rejoin only through a complete validated plan. Multi-root conflicts show
  every affected choice, not a misleading pair of options.
- Network failure, catalog truncation, malformed/unknown predicates, timeout,
  disk-full and cancellation never publish a partial or empty replacement.
- Cosmetic metadata changes do not resolve again; stale attempts/snapshots cannot
  overwrite newer choices; a blocked update leaves warm-launch work unchanged.
- Restore and retention/GC previews preserve exact local imports, active leases,
  retired profiles and unknown data. Restore tests must restore, not just list.
- Run actual Windows/Linux/macOS ARM/Intel CI on the changed commit. Distinguish
  cross-compilation from native behavior and packaging from game compatibility.

For decision-state model checking, include accept/reject/defer/revoke, restart,
stale completion and multiple profiles/targets. Deliberately inject cross-profile
approval reuse and a revocation-bypass bug and require counterexamples. Report the
actual model, bounds, assumptions and commands. A count of explored states alone
does not establish that the implementation or filesystem satisfies that model.

## Precedents And Limits

- [Cargo manifest/lock separation](https://doc.rust-lang.org/cargo/reference/resolver.html):
  adopt requested-versus-resolved state and controlled updates, not Cargo's SemVer
  assumptions or permission to load multiple versions of the same runtime Mod ID.
- [Nix profiles](https://nix.dev/manual/nix/2.35/package-management/profiles) and
  [OSTree atomic upgrades](https://ostreedev.github.io/ostree/atomic-upgrades/):
  adopt prepared immutable generations and a small commit point, not Linux symlink
  mechanics or world/configuration rollback.
- [Controller reconciliation](https://kubernetes.io/docs/concepts/architecture/controller/):
  adopt desired/observed separation and restart convergence, not Kubernetes services
  or distributed-system infrastructure for a desktop application.
- [PubGrub](https://github.com/dart-lang/pub/blob/master/doc/solver.md),
  [Rust PubGrub](https://github.com/pubgrub-rs/pubgrub) and
  [Resolvo](https://github.com/prefix-dev/resolvo): evaluate learned conflicts and
  explanatory solving behind a shared contract; none proves Fabric compatibility.
- [Fabric metadata](https://docs.fabricmc.net/develop/loader/fabric-mod-json) and
  [Modrinth version API](https://docs.modrinth.com/api/operations/getprojectversions/):
  preserve the distinction between distribution packages and loaded units. These
  are evolving external contracts, not hardcoded per-Mod exceptions.

# Plus Play And Mod Preinstallation

## Owners

Plus Play is the collective name for Latest+ and Snapshot+. It is not another
profile or shared selection database.

| Owner | Stored meaning | When applied |
| --- | --- | --- |
| Preinstallation template | Project identities, display metadata, optional loader scope and virtual folders | Copied once into a new ordinary profile |
| Latest+ | Its own requested Mods and exclusions | Background preparation, release channel |
| Snapshot+ | Its own requested Mods and exclusions | Background preparation, newest release or snapshot |
| Ordinary profile | Its own copied or explicitly edited requests | Its fixed Minecraft/loader target |
| Prepared revision | Exact validated objects and resolved versions | Launch and explicit restore |

Template changes never propagate into existing profiles or Plus Play. The
template is not read during launch. Virtual folders organize the template, not
the runtime mods directory. Provider capabilities and a user's loader scope are
different: filtering a search does not silently set a stored application rule.

## Preinstallation

Settings opens a collection/detail editor. Search adds a project; the detail
column sets its loader scope and folder. Folders may have parents; dissolving a
folder moves its children and members to its parent without deleting Mods.

Creation previews query releases for the actual Minecraft/loader target. A
missing release is reported before creation; the user may select available Mods,
create without them, or cancel. Provider failure is not treated as incompatibility.
Vanilla skips the template entirely. Version availability does not establish
whole-graph compatibility: the normal resolver still has to prepare a complete
graph before it can launch.

The backend rechecks the template revision, target and preview digest. Stale
acknowledgement requires a fresh preview, never a silent different copy.
core/profile_creation saves a durable goal containing the new profile and its
captured requests before publishing the profile. Recovery resumes that goal,
not the current template. Completed goals are retained; they do not recreate
removed profiles.

## Plus Play

Each environment has an independent Intent. Recommendations are starter
suggestions during explicit setup, not a live authority. Disabled requests keep
their names and identities; they cannot reappear because another list changed.
This implementation retains disabled entries as exclusions rather than deleting
their metadata. Dependencies remain resolver output.

Plus Play searches Fabric projects. The backend also verifies Fabric releases
on project addition or re-enabling. This is not proof of compatibility with a
particular Minecraft version; final graph validation owns that decision.

Latest+ retains its completed revision when a requested Mod cannot follow a new
release. A validated alternative can ask whether to wait or proceed without the
named Mods. Consent is scoped to the target and exact composition, never shared
with Snapshot+.

Snapshot+ prioritizes the newest known release or snapshot. A separate,
versioned product-policy authorization permits validated temporary omissions.
This is not fabricated user consent. Desired requests remain intact, so later
preparation can restore compatible Mods. The UI distinguishes a ready newest
version with pending Mods from failed preparation.

For an environment containing saved worlds, automatic omission authorization is
withheld. Existing explicit review is used instead. Verified independent world
backup/restore is not implemented; this guard must not be described as a backup.

Both tracks currently require an available Fabric loader. They do not silently
change to Vanilla. Unknown provider errors, exhausted search and damaged state
cannot authorize omissions or publish partial graphs.

## Migration

The old common list remains read-only migration input. Settings can explicitly
import it into preinstallation. It is never automatically copied there.

Each Plus Play environment offers a one-time review of its previous effective
selection, including legacy exclusions. Provider observations restore project
names and identify entries without Fabric releases. Confirmation commits an
independent selection under the profile edit lease and a snapshot revision check.
A legacy completed revision remains usable while review or preparation is pending.
A first launch without a completed revision surfaces setup in the main window.

The former shared-list IPC writers and launch-time reinjection are removed.
Legacy migration/projection readers remain until retained histories can be
interpreted without them. Old planner comparison helpers are test-only.

## Failure Boundaries

Template edits use a filesystem lock, revision check and atomic replacement.
A failed save is not displayed as a successful selection. Corrupt state blocks
writes and remains available for diagnosis. Provider observations are bounded;
they never execute Mod code. New-profile preparation failures leave requests
durable and visible rather than silently claiming an empty successful install.

The staged-file reconciliation applies both requested states and legacy
exclusions. It does not alter live files until complete validation and checkout.
Local objects, user worlds and configuration retain their separate owners.

See [runtime boundaries](runtime-boundaries.md) for storage, launch authorization
and recovery details, and [product contract](mod-experience-contract.md) for the
interaction priorities.

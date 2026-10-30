# Principle Audit

Scope: the 2026-10-30 audit prioritizes user assets, fail-closed protection state,
recoverable maintenance and error visibility. This is not an exhaustive security
scan, a zero-bug claim, or proof of resistance to same-user malware.

## Changes

- Required startup resources gate application commands; setup errors are visible.
- Production unwrap/expect/panic paths are removed and rejected by CI.
- Invalid Mod protection state blocks mutation instead of resetting protection.
- Recommendation propagation has a durable, resumable desired-state journal.
- Profile removal retains assets, rejects active use and supports immediate Undo.
- The unused existing-profile Modpack overwrite IPC is removed.
- Settings replacements are atomic; load/save failures are shown, not hidden.
- Java checks distinguish a usable fallback from successful maintenance.

## Remaining Work

- New-profile Modpack imports need staging/publication/recovery as one lifecycle.
  A failed import can leave partial, unpublished directories or a visible profile
  whose subsequent Mod-recording step failed. Existing profiles are not its target.
- Retired-profile recovery needs a persistent UI, not only immediate Undo.
  Retention under the data root is not an independent backup or uninstall guarantee.
- Installer uninstall/reinstall behavior needs an executed asset/credential test.
  Generated installer paths alone do not prove either retention or credential removal.
- Profile enumeration, background maintenance and some other UI error handlers
  still suppress details. Centralized user-visible diagnostics are not complete.
- Legacy tests still use unwrap/expect. Migrate fallible fixtures incrementally;
  do not hide them behind lint exceptions or new panic helpers.
- Full-world backups, external retention policy, package identity, process
  isolation and code signing are separate, unimplemented properties.
- macOS/Linux machine-identifier-derived credential fallback keys are not
  OS-protected secrets. HKDF does not make a public identifier secret; these
  fallbacks must not be described as equivalent to Keychain or Secure Enclave.
  Replacing them needs a separately tested OS-keystore migration that preserves
  existing credentials on failure. Missing macOS identifiers now fail closed
  rather than using a shared fixed identifier.

## Verification Boundary

Unit and controller tests exercise corruption, partial propagation, retries,
active-use refusal, no-overwrite restore and settings-save ordering. Platform CI
also compiles and packages Windows, Linux and both macOS architectures on the
exact PR commit. A successful CI run does not prove native sign-in, game startup,
uninstall safety, power-loss durability or macOS interactive UI behavior.

The ownership and recovery contracts are in [runtime boundaries](runtime-boundaries.md).
Private incident analysis and unfinished-review notes belong in local `.agent`,
not in public user documentation or raw uploaded logs.

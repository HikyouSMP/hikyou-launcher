# Engineering Rules

- Read `docs/runtime-boundaries.md` before changing ownership or persistence.
  Local `.agent/README.md` guidance also applies when present.
- Design the owner, inputs, trust boundary, desired result, validation and recovery
  path before implementing. Search primary precedents; adapt their assumptions.
- Do not introduce `unwrap()` or `expect()`, including tests. Do not replace them
  with panicking helpers, silent defaults or lint exceptions. Legacy violations
  are explicit audit debt, not permitted patterns.
- Preserve user worlds, configuration and exact local objects. Cache cleanup,
  profile removal and uninstall must not silently purge user assets.
- Missing state and unreadable/corrupt state are different outcomes. Unknown
  protection state blocks mutation; failed work must not be published as complete.
- Keep operations and modules bounded. Separate usable fallbacks from successful
  maintenance. Backend checks must enforce safety independently of the UI.
- Surface actionable, privacy-filtered failures. Send diagnostics only with consent.
- Verify high-impact failure/recovery paths first and use all-platform CI for the
  exact changed commit. CI does not prove interactive sign-in or game compatibility.
- Record failures, causal review gaps, prevention and verification in local `.agent`
  incident notes. Do not include credentials, raw user logs or identifying paths.
- Ask before remote publication, deleting user data, changing trust defaults or
  adding uncertain dependencies. Never rewrite published tags.

# Windows Credential Hardening Options

## Decision summary

The current TPM/CNG design is a strong disk-at-rest baseline with low user
friction. It is not a complete defense against a hostile process running as the
same user. The best near-term path is accurate runtime measurement, signed
releases, and a compatibility-tested process-mitigation baseline. In parallel,
prototype WAM as the preferred way to remove the refresh token from Hikyou's
storage entirely. Treat VBS enclaves as the strongest longer-term option for
supported Windows 11 systems.

## Security ceiling

Three goals cannot all be guaranteed against arbitrary same-user code at once:

1. no user confirmation,
2. fully offline reusable credentials, and
3. no credential use by hostile code running as the same user.

If legitimate code can silently obtain and use a credential while offline, an
attacker with equivalent execution and caller privileges can attempt the same
operation. Stronger isolation can make this much harder, but a complete boundary
requires an unforgeable application principal, user presence, or a remote service
that refuses requests without stronger proof.

Minecraft itself must eventually receive a Minecraft access token. Therefore the
target is not the impossible claim that no plaintext token ever exists. The
target is: the Microsoft refresh token never enters the Tauri or Java process;
only a narrowly scoped, short-lived Minecraft token crosses the boundary at the
last responsible moment; its persistence and residual lifetime are minimized.

## Target architecture

The strongest practical low-friction design is layered:

1. **Independent app registration.** Use Hikyou's own public-client ID, modern
   personal-account/Xbox scopes where approved, PKCE, exact redirect URIs, and no
   client secret in any desktop binary.
2. **OS token broker.** Let WAM own token maintenance and device-bound refresh
   material when the exact Xbox/Minecraft flow passes the prototype gate.
3. **Minimal credential worker.** Keep token exchange outside the WebView/Tauri
   process. Prefer an AppContainer/LPAC package identity for caller/resource ACLs;
   use VBS-backed keys or a VBS enclave on supported systems.
4. **Capability-shaped protocol.** The worker never exposes a refresh token or a
   generic decrypt operation. It accepts only operations such as “obtain a
   Minecraft launch token for this account now”, applies rate and state checks,
   and returns the least powerful result.
5. **Hardened worker, compatible host.** Apply strict process mitigations to the
   small native worker, which has no WebView or plugin surface. Keep the Tauri
   host's mitigations compatible with WebView2 and Java spawning.
6. **Signed supply chain.** Authenticode-sign the launcher, worker, installer, and
   update metadata; make updates fail closed on signature mismatch.
7. **Offline degradation.** Persist no refresh token in the normal-process
   fallback. Keep only the minimum Minecraft/offline account material needed for
   local play, with an explicit expiry and no promise of online service access.

This closes storage theft and dramatically narrows same-user, injection, memory,
and CNG-call attacks. It still cannot make an already compromised Windows session
equivalent to a trusted one without user presence or server-side token binding.

## Client ID model

A desktop launcher is an OAuth public client. Its client ID is intentionally a
public application identifier and must be assumed extractable from source,
network traffic, logs, or the binary. It is not a client credential and does not
grant access to user accounts by itself. A client secret embedded in Hikyou would
be ineffective as proof of app identity and must never be added.

Using Hikyou's own registration is preferable because Hikyou controls its exact
redirect URIs, supported account types, publisher/consent identity, permissions,
revocation, and operational monitoring. PKCE protects an intercepted
authorization code; signed package identity and broker isolation address the
separate problem of distinguishing installed application instances.

The WAM prototype should test two paths rather than guessing:

- Hikyou's approved modern registration with `XboxLive.SignIn` and
  `XboxLive.offline_access` against the Microsoft consumer authority.
- The current legacy `MBI_SSL` path only as a compatibility control.

Success means completing Microsoft -> Xbox User -> XSTS -> Minecraft exchange,
silent renewal, account switching, logout/revocation, offline launch, and recovery
after broker/cache reset. A successful login alone is not sufficient evidence.

## Option A: Harden the current CNG design

Keep the current-user Platform Crypto Provider key and encrypted token record.
Add verified diagnostics, signing, dependency integrity, and only those Windows
process mitigations proven compatible with Tauri and WebView2.

- Helps: copied credential files, accidental disclosure, many unsigned injection
  paths, and misleading security diagnostics.
- Does not solve: a same-user attacker that can call the same silent CNG key,
  read the process, or control the launcher.
- Usability: unchanged.
- Compatibility gate: test WebView2 child processes, OAuth callback, updater,
  launcher plugins, JVM spawning, and overlays before enforcing mitigations.

Candidate mitigations for audit mode are extension-point disable, low-integrity
image blocking, remote-image blocking, CFG, and strict handle checks. Code
Integrity Guard and Arbitrary Code Guard must not be enabled blindly: CIG only
accepts Microsoft/Store/WHQL-signed images, and WebView2 is multi-process.

Modern CNG also offers `NCRYPT_REQUIRE_VBS_FLAG` and `NCRYPT_PREFER_VBS_FLAG`.
A VBS-protected key can improve private-key isolation without adding prompts,
but it does not stop the same user from requesting a permitted key operation and
does not isolate the plaintext token after decryption. Evaluate it as a compatible
key-backend upgrade, not as the complete same-user boundary.

## Option B: Move refresh-token custody to WAM

Use Windows Authentication Manager/MSAL so the OS broker owns the durable token
cache and Hikyou requests short-lived tokens. This is the best low-friction
architecture when the identity protocol supports it because the launcher no
longer needs to persist or normally handle a Microsoft refresh token.

- Helps: all four threats by shrinking Hikyou's secret lifetime and removing its
  durable refresh-token file/key path.
- Does not solve: malware using the user's active Windows session or compromising
  WAM/the OS.
- Usability: usually improves through silent SSO and account integration.
- Blocking uncertainty: Hikyou currently uses the legacy consumer/Xbox scope
  `service::user.auth.xboxlive.com::MBI_SSL`, the legacy client ID, and
  `login.live.com` endpoints. Official documentation reviewed here does not prove
  that WAM can reproduce this exact Xbox/Minecraft chain for an open launcher.

Adoption gate: a separate prototype must obtain an equivalent Microsoft token,
complete Xbox User, XSTS, and Minecraft exchanges, preserve account switching,
and define offline launch behavior. Do not migrate production storage until all
cases pass.

## Option C: Isolate credential work in a VBS enclave

Move key derivation, token decryption, and ideally token refresh into a VBS
enclave on supported Windows 11 systems. VBS enclaves isolate memory from the
host process and the rest of the normal OS. This is the strongest technical path
against launcher injection and process-memory reads without prompting on every
launch.

- Helps: code injection, host-process memory reads, and key extraction.
- Remaining risk: a compromised host can still request allowed operations unless
  the enclave protocol strictly limits operations and binds requests to trusted
  state. It cannot make a silently usable account immune to all same-user abuse.
- Usability: transparent on supported devices, fallback required elsewhere.
- Cost: high. Requires Windows 11 24H2-era support, VBS/HVCI, enclave signing,
  a narrow host/enclave protocol, native build/release work, and a new audit
  surface.

Adoption gate: threat-model the enclave API first; prototype only refresh and
decrypt operations; verify host compromise cannot request arbitrary plaintext;
measure OS support and fallback behavior before product integration.

## Option D: Packaged identity plus an AppContainer credential worker

Give the Windows distribution stable package identity and place a minimal
credential worker in an AppContainer/LPAC. Restrict its IPC and CNG object ACLs
to the package identity. AppContainer tokens carry an unforgeable package SID,
which can distinguish the worker from an arbitrary unpackaged same-user process.

- Helps: direct same-user opening of broker-owned objects and broad filesystem or
  network access from the credential worker.
- Does not solve: injection into a full-trust launcher that is authorized to ask
  the worker for operations, unless the request protocol and process boundaries
  are also hardened.
- Usability: potentially transparent after installation.
- Cost/risk: high. Merely packaging the existing full-trust Tauri app as MSIX does
  not sandbox it; the worker, capabilities, IPC authentication, updates, and
  fallback distribution all need separate design and compatibility tests.

This is a research option, not a reason to replace MSI/NSIS immediately.

## Rejected as a primary solution

- PCR binding: useful for boot-state binding, but firmware/Secure Boot changes can
  make credentials inaccessible and it does not stop post-boot same-user malware.
- A normal Windows service: moving the key into a service adds complexity but
  does not create a caller identity boundary if every same-user client is allowed.
- MSIX package identity by itself: a medium-integrity full-trust packaged desktop
  app is not automatically an AppContainer.
- Protected Process Light: an ordinary launcher cannot opt into the anti-malware
  protected-service model without Microsoft/ELAM requirements.
- Token Protection: current Microsoft documentation limits it to supported Entra
  resources; Hikyou cannot unilaterally bind legacy Xbox/Minecraft tokens.
- Windows Hello on every use: strong user-presence protection, but explicitly not
  selected because of launch friction.

## Recommended sequence

1. Ship and observe the structured runtime measurements added in this change.
2. Sign release artifacts and update metadata; reject unsigned update paths.
3. Build a process-mitigation compatibility harness and enable only passing
   mitigations.
4. Prototype WAM against the exact current Xbox/Minecraft flow.
5. If WAM cannot support the flow, prototype a VBS-enclave credential backend as
   an optional high-security tier while retaining a measured CNG fallback.

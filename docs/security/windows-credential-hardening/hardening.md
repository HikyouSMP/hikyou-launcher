# Windows Credential Hardening Options

## Decision summary

The TPM/CNG design remains a strong disk-at-rest baseline for the short-lived
Minecraft token kept for offline launch. The long-lived Microsoft credential is
now owned by WAM on Windows: a minimal Native AOT MSAL adapter obtains a
short-lived Microsoft access token and passes it to Rust through a private child
process pipe. Hikyou's approved public-client identity is used throughout.
VBS enclaves remain a possible stronger future isolation tier for supported
Windows 11 systems.

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
only a short-lived Microsoft access token crosses from the broker to Rust for
the immediate Xbox/Minecraft exchange; Minecraft receives its own short-lived
token at launch, and only that offline-useful token is encrypted by Hikyou.

## Target architecture

The strongest practical low-friction design is layered:

1. **Independent app registration.** Use Hikyou's own public-client ID, modern
   personal-account/Xbox scopes where approved, PKCE, exact redirect URIs, and no
   client secret in any desktop binary.
2. **OS token broker.** WAM owns token maintenance and the broker-managed
   long-lived account session. A Native AOT MSAL adapter contains only Microsoft
   acquisition.
3. **Narrow broker protocol.** The child accepts interactive or account-bound
   silent acquisition and returns one short-lived access token. It does not
   expose refresh tokens, decryption, files, network proxying, or arbitrary APIs.
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

A desktop client ID is a public application identifier, not a credential. It is
extractable from a release binary or network traffic, and an embedded client
secret would provide no useful proof of desktop-app identity. Hikyou uses its
own registration to control consent, redirects, revocation, and monitoring.
Official builds provide the ID through `HIKYOU_MSA_CLIENT_ID`; forks use their
own registration. Compatibility evidence is recorded below.

### Windows authentication adapter

The Windows adapter uses Microsoft's supported MSAL.NET WAM implementation
because no equivalent Microsoft-supported Rust library exists. It remains a
small, fixed-purpose Native AOT process. Hikyou stores a SHA-256 account-selection
key instead of WAM's durable `HomeAccountId`; this reduces exposed account
metadata but is pseudonymization, not an authentication boundary.

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
- Implementation: Windows uses Hikyou's client ID, MSAL/WAM, modern Xbox scopes,
  and a Native AOT sidecar. macOS/Linux use the same identity and Xbox contract
  through browser OAuth + PKCE. Hikyou persists the Minecraft token for offline
  launch but no Microsoft refresh token on Windows.

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

1. Ship and observe the WAM lifecycle and structured runtime measurements added
   in this change.
2. Sign release artifacts and update metadata; reject unsigned update paths.
3. Build a process-mitigation compatibility harness and enable only passing
   mitigations.
4. Complete the remaining multi-account, consent-revocation, and non-Windows
   device scenarios listed below.
5. Keep a VBS-enclave credential backend as a possible optional high-security
   tier while retaining a measured CNG fallback for unsupported systems.

## WAM compatibility measurements

Measured on Windows on 2026-08-30 with an isolated probe that did not read or
write Hikyou credential storage and did not print token contents.

| Client identity | Microsoft acquisition | Xbox User | Minecraft XSTS | Minecraft login |
| --- | --- | --- | --- | --- |
| Prism client ID | browser OAuth + PKCE | passed | passed | passed |
| Hikyou approved client ID | browser OAuth + PKCE | passed | passed | passed |
| Hikyou approved client ID | MSAL.NET + WAM | passed | passed | passed |
| Legacy shared client ID | MSAL.NET + WAM | passed | passed | passed |

Both successful modern flows requested `XboxLive.SignIn` and
`XboxLive.offline_access`, used a `d=` RPS ticket, requested XSTS for
`rp://api.minecraftservices.com/`, and called Minecraft
`/launcher/login` with platform `PC_LAUNCHER`. The Hikyou registration used the
required broker redirect URI
`ms-appx-web://microsoft.aad.brokerplugin/<HIKYOU_MSA_CLIENT_ID>`
and the `Personal Microsoft accounts only` audience.

### Corrected interpretation of the initial HTTP 400 result

The initial probe results were false negatives. The diagnostic code used .NET
`JsonContent.Create`, which emitted
`Content-Type: application/json; charset=utf-8`. Prism and Microsoft's Xbox
examples send the media type as exactly `application/json`. Although the bodies
were semantically identical JSON, Xbox User authentication returned an empty
HTTP 400 for the request containing the charset parameter. After serializing the
same body manually and setting the exact media type, Xbox User succeeded. XSTS
showed the same behavior until its request was corrected as well.

The corrected probe then completed the full chain for both Prism's current client
ID and Hikyou's approved client ID. The Hikyou client also completed the full
chain when the Microsoft token was acquired through MSAL.NET's WAM broker. The
successful final response resolved the expected Minecraft profile; token values
were neither printed nor persisted.

This changes the security conclusion: WAM is technically compatible with the
Hikyou registration and the public Xbox/Minecraft chain. SISU or GDK title
provisioning is not required for this tested modern public-client path. The
production code now uses this verified path; the legacy SISU implementation was
removed rather than retained as a second source of authentication truth.

The investigation also establishes a general diagnostic requirement: compare
the serialized HTTP request, including method, endpoint, headers and media-type
parameters, rather than treating equivalent data models as equivalent wire
requests. A response without a body must not be attributed to identity or
service provisioning until request-level differences have been eliminated.

### Production lifecycle measurements

Measured after integration:

1. interactive WAM acquisition succeeded;
2. account-bound silent acquisition succeeded in a separate broker process;
3. an unknown account ID failed as `interaction_required` without returning a token;
4. the legacy shared Minecraft/Modrinth control ID also completed the corrected
   WAM -> Xbox -> XSTS -> Minecraft chain;
5. Rust tests enforce exact `application/json` without media-type parameters;
6. the broker is self-contained Native AOT and a generated NSIS installer script
   includes and removes the sidecar; and
7. token values were not printed, persisted by the probe, or passed through
   command-line arguments or environment variables.

Still requiring device/user scenarios not available in this single-account test:

- switching between two real Microsoft accounts;
- real consent revocation and Windows broker-cache reset;
- macOS/Linux device-level callback testing; and
- older supported Windows versions without a usable broker.

Primary references:

- https://learn.microsoft.com/en-us/entra/msal/msal-acquire-cache-tokens
- https://learn.microsoft.com/en-us/entra/identity-platform/msal-client-applications
- https://learn.microsoft.com/en-us/gaming/gdk/docs/services/fundamentals/s2s-auth-calls/service-authentication/live-website-authentication
- https://learn.microsoft.com/en-us/gaming/gdk/docs/services/fundamentals/portal-config/live-setup-partner-center-partners
- https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/auth/steps/MSAStep.cpp
- https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/auth/steps/XboxUserStep.cpp
- https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/auth/steps/LauncherLoginStep.cpp

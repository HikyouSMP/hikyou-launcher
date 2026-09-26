# Windows Credential Hardening Context

This assessment records the measured implementation after the WAM migration.
Use repository history to bind it to a particular revision; do not copy a stale
commit hash into this living document.

## Current boundary

On Windows, WAM owns the long-lived Microsoft credential. Hikyou invokes a
minimal self-contained MSAL adapter for interactive or account-bound silent token
acquisition. The adapter returns a short-lived Microsoft access token through a
private child-process pipe and exits. Rust immediately exchanges it for Xbox,
XSTS, and Minecraft tokens. Hikyou intentionally encrypts the Minecraft token
for offline-friendly launches, but does not persist a Microsoft refresh token.

The adapter is a deliberately narrow C# exception around Microsoft's supported
MSAL.NET WAM package. Hikyou does not implement MSAL or WAM token-cache behavior
itself. The project-owned public Client ID is supplied once at build time through
`HIKYOU_MSA_CLIENT_ID`; forks must use their own registration.

MSAL adds the OpenID Connect `profile` scope to WAM requests. This produces the
Microsoft consent entry for basic profile access even though Hikyou requests no
`User.Read` permission. Hikyou uses only WAM's account identifier for account
selection and does not retain profile names, pictures, or usernames.

The existing TPM/CNG storage still protects the offline Minecraft record at rest.
WAM removes Hikyou's reusable Microsoft refresh token from that boundary. It
does not make an unlocked, compromised Windows session trustworthy: same-user
malware may still induce broker use, inject into Hikyou, or read short-lived
tokens while the process is running.

## Measured properties

At startup, Hikyou now records structured diagnostics for the active secure
storage backend. Windows measurements query the provider implementation flags,
key export policy, UI policy, PCR mask, and security-descriptor support. Missing
or unsupported properties remain `unavailable`; a provider name is not treated
as proof of hardware backing.

The measurements contain no token, key, ciphertext, SID, or ACL contents. They
are written to the launcher log and exposed in Debug for local inspection.

### Observed test-host result

On 2026-08-30, build `26.1.0-beta.1` recorded the following in
`session_20260830_003324.log`:

- provider implementation flags: `0x00000001` (hardware flag verified)
- private-key export policy: `0` (export and archival export disabled)
- key scope: current-user KSP namespace; no machine-key flag
- security descriptor: valid, owner present, non-NULL DACL, four ACEs, neither
  owner nor DACL marked defaulted
- UI policy: property not returned (`0x80090011`), therefore unavailable rather
  than inferred disabled
- PCR mask: property unsupported (`0x80090029`), therefore unavailable rather
  than inferred disabled

This is a device-specific observation, not a guarantee for every installation.

## Threats in scope

1. Malware running under the same Windows user.
2. Native or WebView-related code injection into Hikyou Launcher.
3. Reading plaintext credentials from the running process.
4. Another same-user process opening or invoking the persisted CNG key.

Kernel compromise, malicious firmware, and physical attacks against a powered
and unlocked device are not solvable by an ordinary desktop launcher.

## Primary evidence

- [Microsoft CNG persisted-key scope](https://learn.microsoft.com/windows/win32/api/ncrypt/nf-ncrypt-ncryptcreatepersistedkey)
- [CNG key storage providers](https://learn.microsoft.com/windows/win32/seccertenroll/cng-key-storage-providers)
- [CNG key-storage properties](https://learn.microsoft.com/windows/win32/seccng/key-storage-property-identifiers)
- [Windows Authentication Manager for desktop apps](https://learn.microsoft.com/windows/apps/develop/security/)
- [MSAL WAM integration](https://learn.microsoft.com/entra/msal/dotnet/acquiring-tokens/desktop-mobile/wam)
- [VBS enclaves](https://learn.microsoft.com/windows/win32/trusted-execution/vbs-enclaves)
- [Windows credential protection and VBS key isolation](https://learn.microsoft.com/windows/security/book/identity-protection-advanced-credential-protection)
- [Windows exploit-protection reference](https://learn.microsoft.com/defender-endpoint/exploit-protection-reference)
- [WebView2 process model and security](https://learn.microsoft.com/microsoft-edge/webview2/concepts/security)

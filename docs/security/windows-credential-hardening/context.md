# Windows Credential Hardening Context

Source revision: `a904eaec874aa764e6195655fd83f03d1c2b0ee2`

This assessment was prepared with working-tree changes present. It separates
the measured current implementation from future architecture options.

## Current boundary

Hikyou stores the long-lived Microsoft refresh token in an AES-256-GCM record.
On Windows, the AES key is wrapped with a persisted RSA-2048 key opened from the
Microsoft Platform Crypto Provider in the current-user key namespace. The RSA
private key is requested as non-exportable. Microsoft, Xbox User, XSTS, and
Minecraft access tokens are transient except for the intentionally cached
Minecraft token used for offline-friendly launches.

The current process can use the key silently. This protects copied files and
disk-at-rest material, but it does not create an application identity boundary:
malware running as the same Windows user can potentially open the named key,
inject into the launcher, read its memory, or induce/proxy a decrypt operation.

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

# Security Policy

## Supported Releases

Security fixes are applied to the latest published beta or stable release.
Older prereleases are not maintained.

## Reporting a Vulnerability

Do not report security vulnerabilities in public GitHub issues or discussions.

Maintainers must keep GitHub Private Vulnerability Reporting enabled. Submit
reports through **Security** > **Report a vulnerability**. Include the affected
version, reproduction steps, impact, and any proof of concept.

Maintainers should acknowledge reports within seven days, investigate privately,
and coordinate a fix before public disclosure.

## Architecture Notes

The current Windows credential boundary, measured properties, residual same-user
risks, and researched hardening paths are documented in the
[Windows credential hardening context](docs/security/windows-credential-hardening/context.md).

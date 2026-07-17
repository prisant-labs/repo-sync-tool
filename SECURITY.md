# Security Policy

RepoSync is an open-source, community-maintained desktop utility. Security
reports are welcome and appreciated: they help keep everyone who runs the tool
safe.

The project is pre-1.0 (current line: v0.9.x) and maintained on a best-effort
basis. Please read the expectations below before reporting.

## Supported versions

While RepoSync is pre-1.0, only the most recent released version receives
security fixes. There are no long-term-support branches yet.

| Version           | Supported          |
| ----------------- | ------------------ |
| v0.9.x (latest)   | Yes                |
| Older prereleases | No                 |

## Reporting a vulnerability

Please do not open a public issue for a security problem. Report it privately
through GitHub's private vulnerability reporting:

**[Report a vulnerability](https://github.com/prisant-labs/repo-sync-tool/security/advisories/new)**

That link opens the same private form as the "Report a vulnerability" button on
the repository's Security tab. The report stays private, and the fix is
coordinated in the same thread.

To help us reproduce and triage quickly, please include:

- the affected version and platform (Windows or macOS, and the OS version),
- the steps to reproduce, ideally a minimal proof of concept,
- the impact you observed or believe is possible.

## What to expect

- **Acknowledgement:** best effort, typically within 5 business days.
- **Assessment:** we confirm the issue, gauge severity, and tell you whether and
  roughly when we plan to fix it.
- **Fix and disclosure:** we aim to ship a fix before any public disclosure.
  Please give us reasonable time to remediate before you disclose publicly. We
  will credit you in the release notes unless you would rather stay anonymous.

## Scope

In scope:

- the RepoSync desktop application (the Tauri shell and the `reposync-core`
  sync engine),
- the auto-updater and the update channel,
- the release artifacts published from this repository.

A vulnerability in a bundled third-party dependency is in scope when it has a
real, demonstrable impact on RepoSync users; a fix there usually means updating
the dependency. Purely theoretical issues, social-engineering attacks, and
attacks that require an already-compromised machine or physical access are out
of scope.

## A note on current posture

RepoSync is prerelease software. During the v0.9.x line, release binaries may be
unsigned and the auto-update path is still being hardened. Treat prerelease
builds accordingly, and build from source if your threat model calls for it.

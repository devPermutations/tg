# Security Policy

## Supported versions

`tg` is currently pre-1.0. Only the latest **MINOR.PATCH** release on
the `main` branch receives security fixes.

| Version | Supported |
| --- | --- |
| 0.5.2 (latest) | ✅ |
| 0.5.0 – 0.5.1 | ❌ Upgrade to 0.5.2 — see [GHSA-5pvm-3m24-8p3f](https://github.com/devPermutations/tg/security/advisories/GHSA-5pvm-3m24-8p3f) below |
| < 0.5.0 | ❌ Pre-feature-freeze, not supported |

If you're running an older version, upgrade with:

```bash
git pull
cargo install --path . --force
systemctl --user restart tg-listen
```

## Reporting a vulnerability

**Don't open a public GitHub issue for security reports.** Instead:

1. **GitHub Security Advisories** (preferred). Open a private report at
   <https://github.com/devPermutations/tg/security/advisories/new>.
   Only the maintainers see it; GitHub auto-generates the disclosure
   workflow.
2. **Email** is a fallback if you don't have a GitHub account: send to
   the address shown by `git log --format='%ae' main | head -1`. Use
   `[tg security]` in the subject line.

Please include:

- Affected version (`tg --version`)
- A description of the vulnerability and its impact
- Reproduction steps or a proof-of-concept (don't include real bot
  tokens in any report — use a placeholder)
- Suggested fix or mitigation if you have one

### Response expectations

- **Acknowledgement** within 7 days.
- **Fix or mitigation plan** within 30 days for HIGH severity, longer
  for lower severity. This is a single-maintainer side project, so
  timelines aren't contractual — they're best-effort.
- **Public disclosure** happens after a patched release is shipped.
  If the vulnerability is being actively exploited in the wild, we'll
  coordinate disclosure timing with you.

## Severity guidance

We use the same severity bands as the in-repo security audit:

- **HIGH** — Directly exploitable: remote code execution, token
  recovery, allowlist bypass, anything that gives an external party
  uninvited keystroke-injection into the tmux pane.
- **MEDIUM** — Exploitable under specific conditions, or with
  significant but partial impact.
- **LOW** — Defense-in-depth, hardening, theoretical issues that
  don't meet the bar above.

Severity is not the same as confidence. A LOW-confidence MEDIUM
finding may still be reported and reviewed, just don't expect a same-week
patch.

## Out of scope

The following are explicitly out of scope and won't be treated as
vulnerabilities:

- Anything requiring shell access on the same user account as
  `tg-listen`. The threat model assumes the local account is trusted.
- Denial of service via large attachments, oversized text, or rapid
  message flooding (Telegram rate-limits the bot side; we honor that).
- Dependency advisories that don't affect `tg`'s actual usage of the
  library. `cargo audit` runs in CI; we triage each finding on its
  merits.
- Vulnerabilities in `whisper.cpp`, `ffmpeg`, `tmux`, or any other
  external dependency. Report those upstream.

## Past advisories

| Advisory | Affected versions | Patched in | Description |
| --- | --- | --- | --- |
| [GHSA-5pvm-3m24-8p3f](https://github.com/devPermutations/tg/security/advisories/GHSA-5pvm-3m24-8p3f) | v0.5.0 – v0.5.1 | v0.5.2 | Bot token leaked into `tg-listen` journald output via `ureq` error formatting. Local-account read-only. See [v0.5.2 release notes](https://github.com/devPermutations/tg/releases/tag/v0.5.2). |

## Audit process

The codebase is small enough to be fully auditable. The
[Security model](README.md#security-model) section of the README
documents the threat model, the layered defenses, and what's not
claimed. A `/security-audit` run on `main` (via Claude Code's
security-guidance plugin) is part of the recommended pre-release
checklist; the audit report is stored in the v0.5.2 advisory above.

If you're evaluating `tg` for production-like use, you're expected to
run your own audit. The repo is small (≈ 1500 LOC of Rust), uses
no `unsafe`, and the architecture document in [docs/design.md](docs/design.md)
covers the design decisions in full.

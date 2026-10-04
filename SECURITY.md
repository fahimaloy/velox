# Security Policy

## Supported Versions

| Version | Supported |
| --- | --- |
| `main` (development) | ✅ |
| 0.1.x | ✅ |
| older releases | ❌ |

Security fixes are applied to `main` and backported to the latest 0.1.x release line where applicable.

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues, discussions, or pull requests.**

Report them privately instead:

- **Email:** [fahimaloy@tutamail.com](mailto:fahimaloy@tutamail.com)

Include as much of the following as you can:

- The type of issue (e.g. panic, memory-safety bug, privilege escalation)
- Full paths of the affected source files and the location of the relevant code
- Step-by-step instructions or a minimal reproduction
- Proof-of-concept or exploit code, if available
- Impact assessment, including how an attacker might exploit the issue
- The version/commit you tested against

## Response Expectations

- **Acknowledgement:** within 72 hours of your report.
- **Initial assessment:** within 7 days — you will receive a severity estimate and a plan.
- **Fix timeline:** proportional to severity. Critical issues are prioritized for an expedited patch; lower-severity issues are fixed in the normal release cadence.
- **Coordination:** you will be kept informed throughout, credited in the release notes (unless you prefer otherwise), and given reasonable time before public disclosure.

## Scope

The Rust workspace crates (`velox-core`, `velox-sfc`, `velox-dom`, `velox-renderer`, `velox-style`, `veloxc`) and the CI/tooling configuration in this repository are in scope. The nested `velox_web/` site repository is tracked separately.

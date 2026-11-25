# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| latest  | :white_check_mark: |
| < latest| :x:                |

Only the latest release receives security updates. We recommend always running the most recent version.

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **Do NOT open a public GitHub issue.** Security vulnerabilities must be reported privately.
2. **Email:** Send a detailed report to the maintainers via [GitHub Security Advisories](https://github.com/graphit-labs/graphit-code/security/advisories/new).
3. **Include:**
   - A description of the vulnerability
   - Steps to reproduce the issue
   - Potential impact assessment
   - Suggested fix (if any)

## Response Timeline

- **Acknowledgment:** Within 48 hours of receiving the report.
- **Assessment:** Within 7 days, we will confirm the vulnerability and assess its severity.
- **Fix:** Critical vulnerabilities will be patched within 14 days. Non-critical issues will be addressed in the next scheduled release.
- **Disclosure:** We will coordinate public disclosure with the reporter after a fix is available.

## Security Measures

This project implements the following security practices:

- **CI Security Scanning:** All commits are scanned with `govulncheck` for known Go dependency vulnerabilities.
- **Static Analysis:** `gosec` runs as part of the linting pipeline to detect common security issues.
- **Dependency Updates:** Dependabot monitors Go, npm, and GitHub Actions dependencies for known vulnerabilities.
- **Checksum Verification:** All release binaries include SHA-256 checksums. The `self-update` command verifies checksums before replacing binaries.
- **Code Review:** All changes to `main` require review before merge.

## Scope

The following are in scope for security reports:

- Authentication and authorization bypasses
- Path traversal vulnerabilities
- Code injection via MCP tools or CLI inputs
- Insecure defaults that could lead to data exposure
- Supply chain vulnerabilities in dependencies

The following are **out of scope**:

- Vulnerabilities in third-party services (e.g., GitHub, Git hosting)
- Social engineering attacks
- Denial of service via resource exhaustion (the tool runs locally)

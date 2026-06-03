# Vulncheck Integration

## Objective
Add `govulncheck` to the CI pipeline (`make ci`) and resolve the reported vulnerabilities.

## Changes Made
- **Makefile**: Added `vulncheck` target which runs `govulncheck ./...`.
- **Makefile**: Integrated `vulncheck` into the `ci` and `check` targets so it runs as part of the pipeline.
- **go.mod**: Bumped the Go version from `1.25.6` to `1.25.11` and added `toolchain go1.25.11` to resolve the reported 10 standard library vulnerabilities.

## Verification
- Running `make vulncheck` locally to confirm vulnerabilities are resolved.

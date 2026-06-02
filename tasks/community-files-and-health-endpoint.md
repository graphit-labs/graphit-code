# Task: Community Files and Health Endpoint
## Goal
Implement missing community standards files (Code of Conduct, Issue Templates, PR Templates) and add liveness/readiness health check endpoints to the `UnifiedServer`.

## Changes Made
- Created `CODE_OF_CONDUCT.md` based on the Contributor Covenant v2.1.
- Created `.github/ISSUE_TEMPLATE/bug_report.yml` for structured bug reporting.
- Created `.github/ISSUE_TEMPLATE/feature_request.yml` for feature requests.
- Created `.github/ISSUE_TEMPLATE/config.yml` disabling blank issues and linking discussions.
- Created `.github/pull_request_template.md` with a standard checklist.
- Added `/health` and `/ready` endpoints in `internal/uiserver/unified_server.go` to provide health checking for the server.
- Fixed linter failures caught by `make ci` (`errcheck` on `os.MkdirAll` in `mcpserver_test.go` and `errorlint` on `errors.Is` in `daemon_test.go`).

## Verification
- Ran `make ci` successfully with all tests and linters passing.

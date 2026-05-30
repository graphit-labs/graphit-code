---
title: "Self-Update URL Brand Customization"
type: task
status: complete
date: 2026-05-30
---

# Self-Update URL Brand Customization

## Objective

Allow white-label distributors to customize the self-update release endpoint via the brand system and Makefile, so builds can point `self-update` to any server — not just the GitHub API.

## Files Changed

| File | Change |
|---|---|
| `internal/brand/brand.go` | Added `SelfUpdateURL` package-level variable (default empty) |
| `internal/updater/updater.go` | Extended `LatestRelease(repo, selfUpdateURL)` to accept a custom URL; when non-empty, it's used directly instead of building a GitHub API URL |
| `cmd/graphit/commands/lifecycle.go` | Updated `newSelfUpdateCmd` to pass `brand.SelfUpdateURL`; relaxed guard to allow self-update when either `GitHubRepo` or `SelfUpdateURL` is set; genericized progress message |
| `Makefile` | Added `SELF_UPDATE_URL ?=` variable and corresponding ldflags injection |
| `internal/brand/brand_test.go` | Added `SelfUpdateURL` to save/restore cycle and custom value assignment |
| `internal/updater/updater_test.go` | Updated all `LatestRelease` calls to new 2-arg signature; added `TestLatestReleaseCustomURL` covering custom URL usage and trailing-slash stripping |
| `docs/guides/private_brand_customization.md` | Documented `SelfUpdateURL` in the customization parameters table; added `SELF_UPDATE_URL` to compilation and Make examples |

## Key Decisions

1. **Additive parameter on `LatestRelease`** — Added `selfUpdateURL` as a second string parameter rather than an options struct, keeping the function signature simple for a single optional override.
2. **Empty string = default behavior** — When `SelfUpdateURL` is empty (the default), the updater falls back to the standard GitHub API pattern (`https://api.github.com/repos/{repo}/releases/latest`). This is fully backwards-compatible.
3. **Custom URL used directly** — When set, the URL is used as-is (with trailing slash stripped). The endpoint must return JSON in the GitHub Release format (`tag_name`, `assets[]`). This keeps the updater simple while supporting any server that mimics the GitHub release response schema.
4. **Guard relaxed** — The self-update guard now allows execution when *either* `GitHubRepo` or `SelfUpdateURL` is configured, so a build with only a custom URL (no GitHub repo) can still self-update.

## Use Cases

### UC-01: Default Self-Update (GitHub)

- **Actor**: End user running `graphit self-update`
- **Preconditions**: Binary built with `GITHUB_REPO` set, `SELF_UPDATE_URL` empty
- **Main Flow**: CLI calls `LatestRelease(repo, "")` → constructs GitHub API URL → fetches release → downloads + verifies + replaces binary
- **Postconditions**: Binary updated to latest GitHub release

### UC-02: Custom Self-Update (Private Server)

- **Actor**: End user running branded binary (e.g., `devkit self-update`)
- **Preconditions**: Binary built with `SELF_UPDATE_URL=https://releases.company.com/devkit/latest`
- **Main Flow**: CLI calls `LatestRelease(repo, url)` → uses custom URL directly → fetches release JSON → downloads + verifies + replaces binary
- **Postconditions**: Binary updated from private release server

### UC-03: No Update Source Configured

- **Actor**: End user running `self-update` on a build with both `GitHubRepo` and `SelfUpdateURL` empty
- **Preconditions**: Both brand variables are empty strings
- **Main Flow**: Guard triggers → user sees "self-update is not configured for this build"
- **Postconditions**: No action taken, clear error message

## Test Cases & Acceptance Criteria

### TC-01: Default GitHub URL (Ref: UC-01)
- **Given** `selfUpdateURL` is empty
- **When** `LatestRelease("org/repo", "")` is called
- **Then** the request URL is `https://api.github.com/repos/org/repo/releases/latest`

### TC-02: Custom URL Override (Ref: UC-02)
- **Given** `selfUpdateURL` is `https://my-server.example.com/api/releases/latest`
- **When** `LatestRelease("org/repo", url)` is called
- **Then** the request URL is exactly `https://my-server.example.com/api/releases/latest`

### TC-03: Trailing Slash Stripped (Ref: UC-02)
- **Given** `selfUpdateURL` is `https://my-server.example.com/releases/`
- **When** `LatestRelease` is called
- **Then** the request URL has no trailing slash

### TC-04: Guard Allows SelfUpdateURL Only (Ref: UC-02)
- **Given** `brand.GitHubRepo` is empty and `brand.SelfUpdateURL` is set
- **When** user runs `self-update`
- **Then** the command proceeds (no "not configured" error)

### TC-05: Guard Blocks When Both Empty (Ref: UC-03)
- **Given** both `brand.GitHubRepo` and `brand.SelfUpdateURL` are empty
- **When** user runs `self-update`
- **Then** the command fails with "self-update is not configured"

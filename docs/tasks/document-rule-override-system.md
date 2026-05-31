---
title: Document and Implement Hub Main Branch Rule Override
status: done
created: 2026-05-30
updated: 2026-05-30
tags: [feature, rules, override, hub, brand]
---

# Document and Implement Hub Main Branch Rule Override

## Objective

Implement and document the Hub main branch rule override feature: when a global rule file exists on the `main` branch of the Hub Git repository, it should be used as a team-wide override for all modules, sitting between the global CLI override and the compiled-in default in the resolution hierarchy.

## Implementation Details

The implementation adds a third tier to the rule resolution hierarchy in `internal/brand/brand.go`. The Hub repository's working directory (which reflects the `main` branch) is at `~/.graphit/hub/`. The new check looks for rule files at `~/.graphit/hub/rules/<module>.md`, computed as `GlobalDir() + "/hub/rules/"` to avoid circular imports with the `config` package.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/brand/brand.go` | Modified | Added `HubRulesDir()` function and Hub main branch check in both `ResolveModuleRule` and `ResolveModuleSkill` |
| `internal/brand/brand_test.go` | Modified | Added tests for `HubRulesDir()`, Hub fallback resolution, placeholder substitution in Hub rules, and global-over-hub precedence |
| `docs/specs/rule_override.md` | Created | Full specification of the multi-layer rule override system |
| `docs/specs/hub_collaboration.md` | Modified | Added Hub-Based Rule Overrides section |
| `docs/guides/user_manual.md` | Modified | Expanded rule customization section with full override hierarchy |
| `docs/tasks/document-rule-override-system.md` | Replaced by this file | Superseded |

## Key Decisions

- Used `GlobalDir() + "/hub/rules/"` instead of importing `config.HubRepoDirPath()` to avoid circular dependency (`config` imports `brand`).
- Rule files live in the `rules/` directory on the Hub's `main` branch, matching the same naming convention as global CLI overrides.
- Placeholder substitution (`{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}`) works identically across all three override tiers.

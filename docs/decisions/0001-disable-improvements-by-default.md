# ADR 0001: Improvements Module Default State

## Status
Superseded — The `improvements` module is now **enabled by default**.

## Context
The `improvements` module implements the autonomous code improvement, auditing, and refactoring methodologies. It was initially configured as an opt-in module (disabled by default) to prevent unnecessary rules cluttering the developer's system and IDE workspace.

After usage feedback, the improvements module was found to be a core value proposition of Graphit Code. Requiring explicit opt-in created friction and meant most users never discovered the feature.

## Decision (Original — Superseded)
~~We will add `"improvements"` to the `OptInModules` list in the configuration management package (`internal/config/config.go`). This registers `improvements` as an opt-in module, which defaults its status to disabled unless a developer explicitly configures `modules.improvements` to `"true"` in their project or global configuration.~~

## Decision (Current)
The `improvements` module is **enabled by default** — it is no longer in the `OptInModules` list. It installs its rule blocks and IDE artifacts automatically upon project initialization. Developers who do not want it can explicitly disable it via `graphit config modules.improvements false`.

## Consequences
- **Positive**: All users benefit from code improvement analysis out of the box.
- **Positive**: Reduces onboarding friction — one less step to configure.
- **Negative**: IDE rules and skills for `improvements` are installed by default for new projects.
- **Mitigation**: Developers can disable with `graphit config modules.improvements false`.

---
title: "Rule Override Specification"
description: "Technical specification of the multi-layer rule and skill override system, covering project-level, global CLI, and Hub-based overrides with placeholder substitution."
content-type: reference
audience: developers
keywords:
  - rules
  - override
  - customization
  - skills
  - brand
  - hub
  - global rules
prerequisites:
  - "docs/specs/hub_collaboration.md"
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/improvements_module.md"
  - "docs/guides/user_manual.md"
  - "docs/guides/private_brand_customization.md"
---


# Rule and skill instruction overrides

Graphit maintains agent instructions as Markdown under
`templates/agent-instructions/`. Runtime captures candidate sources and effective
values, renders them, then the existing Hub native renderer encodes the artifact
for the selected host. Static template text does not grant authority or embed
live Memory, Task state, credentials or a machine checkout path.

## Override resolution hierarchy

For a logical Markdown template, precedence is project → global CLI → Hub →
compiled default. Input order does not change that choice. Empty Markdown is an
explicit override; absence permits the next source. Invalid selected syntax,
missing required variables and rendering errors fail without falling back to a
lower origin. See the [template engine](../architecture/rust-agent-instruction-templates.md).

The existing instruction publisher captures the selected raw sources and their
origin; it retains physical ownership and current authorization through
publication. Source selection and publication remain distinct from pure rendering.

## Default wrapping and placeholders

Overrides may replace the maintained body or include its compiled-default alias:

```jinja
Project guidance before the default.
{% include "defaults/roles/scout.md" %}
Project guidance after the default.
```

Templates use explicit values such as `{{ brand }}` and
`{{ mcp_tool_xpto }}` supplied by the actual consumer. The variable catalog also
carries effective module skills, brand markers, host and artifact intent. A tool
name is taken from the admitted catalog, never guessed from the brand.
Values are literal data and are not recursively interpreted as templates.
Native tokens with conflicting delimiters use Jinja raw blocks.

## Module rule architecture

The [policy catalog](../architecture/rust-policy-instructions.md) selects module
mandates and lifecycle fragments. The [skill catalog](../architecture/rust-skill-instructions.md)
selects Task, Memory, AST, Hub and Knowledge skills plus their companion files.
[Managed roles](managed_role_context.md) and
[hook context](rust_project_hook_context.md) use the same captured rendering
boundary. Dream selects its separate Memory-only consolidation prompt.

## Skill frontmatter

Managed `SKILL.md` files carry native `name` and `description` metadata and the
rendered Markdown body. Native metadata must be serialized by the existing
structured renderer, with correct quoting and validation; concatenating YAML
scalars can turn prose containing `: ` into an invalid mapping. Invalid names,
lengths or native schema fail publication instead of producing a skill the
host cannot discover.

Publication preserves foreign files, selected companions, user overrides and
native ownership rules. It does not adopt an arbitrary directory or silently
change host permissions. Native binding formats are specified in
[native host bindings](../development/native-host-bindings.md).

## Product commands and extension

Use the current [CLI reference](../guides/cli_reference.md) for instruction
publication and override selection. User templates extend the explicit variable
and include mechanism; no brand-specific placeholder compatibility layer or
old generated source-code rule store is maintained.

---
title: "Documentation Enhancements: Team Collaboration + AST Languages & Architecture"
status: complete
date: 2026-05-30
---

# Documentation Enhancements: Team Collaboration + AST Languages & Architecture

## Objective

Two major documentation enhancements:
1. Add prominent sections highlighting the project's key differentiator: private Git repositories for Hub and Memory enabling team collaboration.
2. Add comprehensive AST documentation: all 16 supported languages, entity/relationship mapping, and full vs incremental indexing architecture.

## Files Changed

- **README.md** — Added new section "🏆 The Ultimate Team Advantage: Private Collaborative Ecosystems" between "The Core Pillars" and "Installation". Includes subsections for Hub Repository, Memory Repository, ASCII diagram of the collaboration flow, and setup example.
- **docs/site/index.html** — Added new "Private Team Collaboration" section with "Game-Changer" pill badge, side-by-side bento cards for Hub and Memory repos, collaboration flow terminal, and privacy badges. Added `bento-span-6` CSS rule. Added "Team" navigation link.
- **docs/guides/private_brand_customization.md** — Restructured the "Setting Up Private Collaboration Ecosystems" section with elevated intro, "Two Pillars of Team Collaboration" subsections (Hub + Memory), artifact types table, and "Fully Self-Hosted Collaboration Loop" diagram.
- **docs/README.md** — Added highlighted link to the Private Team Collaboration section under the existing Private Branding guide entry.

### AST Documentation (Phase 2)

- **README.md** — Added "Supported Languages (16)" table, "What the AST Maps" (nodes/relationships/properties), and "Indexing: Full & Incremental" under the AST Graph Explorer section.
- **docs/site/index.html** — Replaced generic tags with 16 language badges, nodes/edges summary, and capability tags (Full & Incremental Indexing, Cypher Queries, Hybrid Search).
- **docs/specs/ast_module.md** — Added "Supported Languages" section with 16-language table and Cross-Language Extraction Capabilities matrix. Replaced "Incremental Indexing Pipeline" with comprehensive "Indexing Pipeline: Full & Incremental" covering full pipeline (7 steps), incremental pipeline (5 steps), and Performance Characteristics table.

### Other Changes

- **internal/brand/brand.go** — Cleared `DefaultHubRepoURL` to empty string (was `git@github.com:graphit-labs/graphit-code.git`).
- **Makefile** — Cleared `DEFAULT_HUB_REPO` to empty string.

## Key Decisions

1. **Positioned as "game-changer"** — Used pill badge and trophy emoji to signal this is the project's primary differentiator, not just another feature.
2. **Hub and Memory presented as dual pillars** — Each gets its own dedicated card/section rather than being lumped together, to clearly communicate the distinct value of each.
3. **Emphasis on team and collaboration** — Language consistently centers on team benefits (corrections propagate, conventions are learned once, institutional knowledge persists), not individual productivity.
4. **Privacy reinforced throughout** — Every section reiterates zero cloud, zero SaaS, zero data leaving the network, self-hosted.
5. **Practical examples included** — Terminal blocks showing exact `graphit setup` prompts and Git URL configuration make it actionable.

## Use Cases

### UC-01: Team Lead Evaluates Graphit Code for Team Adoption
- **Actor:** Engineering team lead
- **Preconditions:** Browsing the README or website for the first time
- **Main Flow:** Reads "The Ultimate Team Advantage" section → understands Hub + Memory Git repos → sees the collaboration diagram → runs `graphit setup`
- **Postconditions:** Understands the team collaboration model and how to set it up

### UC-02: Developer Configures Private Team Repos
- **Actor:** Developer on a team
- **Preconditions:** Graphit Code installed, team Git repos created
- **Main Flow:** Runs `graphit setup` → enters Hub Git URL → enters Memory Git URL → runs `graphit sync`
- **Postconditions:** Developer's agent now syncs with team knowledge

## Test Cases & Acceptance Criteria

### TC-01: README collaboration section renders correctly (Ref: UC-01)
- **Given:** A user views the README.md on GitHub
- **When:** They scroll past "The Core Pillars"
- **Then:** They see the "🏆 The Ultimate Team Advantage" section with Hub, Memory, and diagram subsections

### TC-02: GitHub Pages collaboration section renders (Ref: UC-01)
- **Given:** A user visits the GitHub Pages site
- **When:** They navigate to the "Team" nav link
- **Then:** They see the "Private Team Collaboration" section with side-by-side Hub/Memory bento cards and collaboration flow terminal

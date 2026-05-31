---
title: "Highlight Private Team Collaboration in Documentation"
status: complete
date: 2026-05-30
---

# Highlight Private Team Collaboration in Documentation

## Objective

Add prominent, highlighted sections across the main README, GitHub Pages site, and documentation guides to showcase the project's key differentiator: the ability to configure private Git repositories as the Hub (centralized team artifacts) and Memory (shared team intelligence), enabling fully private collaborative ecosystems for IT teams.

## Files Changed

- **README.md** — Added new section "🏆 The Ultimate Team Advantage: Private Collaborative Ecosystems" between "The Core Pillars" and "Installation". Includes subsections for Hub Repository, Memory Repository, ASCII diagram of the collaboration flow, and setup example.
- **docs/site/index.html** — Added new "Private Team Collaboration" section with "Game-Changer" pill badge, side-by-side bento cards for Hub and Memory repos, collaboration flow terminal, and privacy badges. Added `bento-span-6` CSS rule. Added "Team" navigation link.
- **docs/guides/private_brand_customization.md** — Restructured the "Setting Up Private Collaboration Ecosystems" section with elevated intro, "Two Pillars of Team Collaboration" subsections (Hub + Memory), artifact types table, and "Fully Self-Hosted Collaboration Loop" diagram.
- **docs/README.md** — Added highlighted link to the Private Team Collaboration section under the existing Private Branding guide entry.

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

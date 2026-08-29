---
title: Remove redundant "keep current value?" confirm in setup wizard
status: done
created: 2026-08-29
updated: 2026-08-29
---

# Remove redundant "keep current value?" confirm in setup wizard

## Objective
In `graphit setup`, `promptValue` showed the current value as the bracketed default
(`Enter hub bucket name [my-bucket]: `) and then, on blank input, asked a second
question — `Keep current hub bucket name "my-bucket"? [Y/n]: ` — before accepting it.
The user flagged this as redundant: the default was already offered once; asking to
confirm it again is asking the same question twice.

## Files Changed
| File | Change | Reason |
|---|---|---|
| `cmd/graphit/commands/setup.go` | Modified | `promptValue` now accepts blank input as "keep current" directly, no second Y/n prompt |

## Key Decisions
- Blank input on a prompt that already has a current value now keeps it immediately,
  matching how the compiled-default case already behaved (no second confirmation there
  either).
- The old flow's only way to unset an existing value was answering "n" to the
  confirm — removing the confirm would have removed that ability. Replaced it with an
  explicit `-` sentinel (`Enter hub bucket name [my-bucket] (- to clear): `), typed by
  the user when they want to clear rather than keep.

## Notes
- No existing tests exercised `promptValue`'s keep/clear behavior directly
  (`grep -rn "promptValue"` only found its call sites); `go build ./...` and
  `go test ./cmd/graphit/commands/...` pass after the change.

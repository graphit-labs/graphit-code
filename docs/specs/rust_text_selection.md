---
title: Rust Core text selection
type: specification
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-db2c}
    relation: implements
  - target: {type: task, id: tsk-d02c}
    relation: enables
---

Core exposes `text_slice::apply(text, request)` and `apply_window(text, request, window_start, window_end)` through the same selector, search and formatter. The window is a strict 1-based inclusive range within the original text. Zero, reversed or out-of-file window coordinates fail; entity metadata is never repaired by clamping.

Request start/end are relative to the window. Zero retains the existing unspecified-bound meaning; positive overshoot clamps within that window. Negative head, tail, start, end or context and reversed request ranges fail before clamping. Selection order is range, head, tail, then pattern and bounded context. Literal search escapes regex and ignores case; regex uses its explicit case semantics.

The result retains full-file total_lines and original global line numbers, including Match.line_number. Start/end follow existing apply selection semantics: pattern search with no explicit range/head/tail leaves those optional fields unset. Empty text represents one empty line; a trailing newline adds the final empty line. No formatter or authority is recreated by the consumer.

This SDK operates on provided text only. Entity uniqueness, coordinates and binding to the same source/generation belong to the AST producer. Main qualification: all 19 contracts tests passed, including six new window regressions; strict Clippy for the Core library and contracts target and scoped rustfmt/diff checks passed. No broad Core suite or SDK execution was repeated. This document does not claim Source acquisition, AST entity lookup or product cutover.

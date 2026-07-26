# DefaultErrorStrategy.ReportError writes to stdout, breaking any stdout-protocol process

**Project:** antlr/antlr4 — Go runtime
**Version:** v4.13.1
**File:** `error_strategy.go` (ReportError, around lines 105-108)

## Summary

`DefaultErrorStrategy.ReportError` writes diagnostics to **stdout** via `fmt.Println`, not to
stderr and not through a configurable sink. Any program that speaks a protocol over stdout —
an MCP server, an LSP server, a filter in a shell pipeline — has its output stream corrupted
by parse diagnostics.

The message we saw injected into a JSON-RPC stream was:

```
unknown recognition error type: ...
```

## Why a custom error listener is not enough

Removing the default listeners does not help, because this write happens in the error
*strategy*, not in a listener. A `SilentErrorStrategy` that embeds `DefaultErrorStrategy` and
forwards `ReportError` still reaches the `Println`. Suppressing it requires reimplementing
`ReportError` rather than delegating — which is easy to get wrong, since the method is also
where recovery bookkeeping happens.

It also has to be done for **every** prediction stage. Our first fix covered the LL stage
only and the corruption persisted, because the SLL stage constructs its own strategy.

## Bonus, same file

`BailErrorStrategy` does not panic. Its doc comment says it bails out of parsing by throwing,
and callers reasonably rely on that to detect failure cheaply; in v4.13.1 the error is
signalled by flagging the parser (`SetError`) instead. Code that treats "no panic" as "parse
succeeded" therefore accepts a failed parse. We hit this while using SLL-then-LL fallback: the
SLL result was always accepted because nothing panicked.

## Expected

Diagnostics should go to stderr, or better, to an injectable writer. At minimum the doc
comment for `BailErrorStrategy` should match its behaviour.

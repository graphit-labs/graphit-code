---
title: "MCP Stdio Module Specification"
description: "Technical specification of the MCP stdio server, tool registration, JSON-RPC message flow, request context resolution, and error handling."
content-type: reference
audience: developers
updated: 2026-10-06
keywords:
  - mcp
  - stdio
  - json-rpc
  - tools
  - server
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/daemon_module.md"
  - "docs/specs/ast_module.md"
references:
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-de72
    relation: supports
  - target:
      type: task
      id: tsk-9e3d
    relation: supports
---


# MCP Stdio Module Specification

`graphit mcp --stdio` exposes the Rust MCP transport. It uses the same Source
catalog and authorization as authenticated HTTP consumers. Protocol stdout
contains MCP frames only; diagnostics belong to the explicit diagnostic sink.
The [Source entrypoint](rust_source_entrypoint.md) defines proxy construction,
authenticated discovery and the distinct Root/Role ingress paths.

## Startup and transport

The caller captures its starting project/version, native environment and
original request. The proxy resolves the existing Runtime discovery and uses
its authenticated Source connection. A missing or invalid owner is a real
startup error. Role credentials cannot silently select or authorize the Root
ingress; Role calls use their dedicated transport.

Each job preserves its original cancellation, deadline and captured selection.
Response drop, shutdown and cancellation retain process/transport ownership
until the relevant jobs have joined. Broken protocol input, provider failures
and encoding failures are causal errors rather than empty successful results.

## Tool registration and categories

The current catalog is exposed through MCP discovery; names and schemas are
not inferred from a prefix or from an old static handler list. Supported tools
cover lifecycle/configuration, AST, Knowledge, Memory, Task/sessions, Hub,
cluster discovery, Dream and daemon operations as enabled by the active services.
See the [MCP tool reference](../guides/mcp_tools_reference.md).

Task lifecycle and search use the same [Task protocol](rust_task_protocol.md)
and [Task MCP](rust_task_mcp.md) as the CLI. AST and Knowledge tools retain
selected project/version or imported artifact pins. Knowledge Ask is advertised
only when the captured service offers it. Unknown tools and malformed inputs
are rejected before invoking an unrelated handler.

## Request context and authority

`project_dir` selects a call-local project context; `project_version` explicitly
selects that project's version. Neither is a stored identity or a permission.
Imported contexts retain their own exact artifact identity and authorization;
there is no fallback to the current project's files for imported reads.

Authentication verifies the credential. Each handler separately obtains its
actual module/operation grant, retains that admission through I/O and checks
it again before delivery when required. Discovery, profile metadata and a valid
JWT alone grant no PROJECT access. The Dream tool policy must agree between
`tools/list` and direct dispatch; aliases cannot bypass it.

## Structured output and errors

Structured Source Root and Role tools accept optional top-level `ai_optimized`,
a strict JSON boolean. Omission or `true` selects TOON text; `false` selects
JSON. Typed JSON results may expose both the SDK `structuredContent` object and
JSON text; this is not a guarantee for every handler or error. Null, strings and numbers are rejected instead of coerced. The flag is
outside a domain's `selection`/`input` envelope and is not an authorization,
query optimization or result-selection option.

In TOON mode the MCP result carries the encoded document in `TextContent` and
does not duplicate it as JSON `structuredContent`. A machine consumer requiring
JSON must explicitly request `ai_optimized: false`, for example:

```json
{"name":"graphit_task_get","arguments":{"ai_optimized":false,"input":{"id":"00000000000000000000000001"}}}
```

The ID is illustrative DATA. JSON-RPC frames, tool arguments, catalogue metadata
and JSON Schemas remain JSON. Native HTTP and CLI JSON responses keep their own
unchanged contracts; this MCP output selector does not belong in their domain
bodies. MCP failures retain `isError` and the actual domain outcome; a successful
HTTP status is not proof of successful tool execution. Plain textual errors remain text, even when JSON was requested. Protocol errors
remain JSON-RPC errors.
The stdio proxy preserves a remote tool-call RPC error code while replacing its
untrusted message and data with finite local diagnostics. A domain `isError`
result remains a tool result, independently of that protocol error path.

Root AST Source and SessionHook retain their original textual payloads. ModuleSkill,
Knowledge Source and Role Source (including Live MCP AST Source) are structured
envelopes containing content
strings: the envelope follows the selected output format while the original
Markdown/source string remains a value, without encoding its syntax tree or
rewriting it as TOON data.

The [Core codec](output_module.md#rust-structured-output-codec) encodes already
authorized, projected DTOs using TOON specification 4.1 defaults: comma delimiter,
two-space indentation, lexicographically ordered object keys and retained array
order. Signed/unsigned 64-bit integers stay exact. Integral floats and negative
zero normalize numerically; there is no two-decimal rounding. TOON may use more
or fewer tokens depending on the document and tokenizer.

Existing input, depth and output bounds still apply. Encoding errors produce no
partial document or silent JSON fallback. Output limits govern publication;
they do not guarantee a peak-allocation cap. Domain schemas, pagination and
redaction remain the producer's responsibility. Unknown counters are not
fabricated as zero.

Formatting can fail after a write has completed. The tool then returns `isError:
true` with a bounded textual diagnostic such as `completed; response_limit`
(or `tool_error` / `response_encoding_failed` for the corresponding outcome),
without JSON fallback or a claim that the write rolled back. Check the current
record before retrying a write. Only an invalid output policy, or a bound too
small for the diagnostic itself, becomes an internal JSON-RPC error carrying
`tool_completed`, `tool_reported_error` and `reason`; it is not a success receipt.

Untrusted provider bodies, credentials and private native session selectors do
not belong in ordinary diagnostics. Cancellation and late authorization denial
must suppress a final answer when its consumer contract requires it while
retaining truthful accepted progress and observed cleanup.

## Related product contracts

- [Source callers](rust_source_callers.md)
- [Managed Role MCP](rust_role_mcp.md)
- [Task and session lifecycle](task_module.md)
- [Knowledge Ask](../architecture/rust-knowledge-question.md)
- [Dream capability boundary](rust_dream_records.md#exact-dream-capability-boundary)

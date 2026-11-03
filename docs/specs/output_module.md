---
title: "Output Module Specification"
description: "Technical specification of the decoupled view/output layer: the Printer abstraction, view interfaces, formatters, structured output for CLI and MCP modes."
content-type: reference
audience: developers
keywords:
  - output
  - printer
  - view
  - formatter
  - decoupled rendering
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/mcpstdio_module.md"
  - "docs/specs/daemon_module.md"
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
  - target:
      type: task
      id: tsk-b6eb
    relation: supports
  - target:
      type: task
      id: tsk-c8cd
    relation: supports
  - target:
      type: task
      id: tsk-24fc
    relation: supports
  - target:
      type: task
      id: tsk-c29b
    relation: supports
---

# Output Module Specification

Rust output separates structured encoding, terminal presentation, diagnostics,
file sinks and log-tail reads. Callers provide explicit authorized DTOs and
output destinations; protocol stdout is reserved for its transport.

## Rust structured output codec

`crates/graphit-core/src/structured_output.rs`, implemented under `tsk-de72`,
serializes an explicit `serde_json::Value` into a complete JSON or TOON document:

```rust
encode(&value, OutputFormat::Json, limits) // Result<EncodedDocument, OutputError>
```

`OutputFormat` has `Json` and `Toon`. `EncodedDocument` owns a private string and
format, with explicit `as_str()`, `format()`, `len()` and `is_empty()` accessors;
`into_string()` transfers the encoded buffer to its consumer without cloning it.
Length is UTF-8 bytes. Debug exposes format/byte count, not payload, and the
wrapper has no automatic Serialize. `OutputError` uses fixed categories
`InvalidLimits`, `Depth`, `Nodes`, `InputBytes`, `OutputBytes`, `Encoding` and
`Allocation`; Debug/Display contain no raw keys/values/backend messages.

### Explicit limits and deterministic values

`Limits` is required with no product Default: `max_depth` is 0–64,
`max_nodes` is at least one, and input/output byte budgets can be zero. Root
depth is zero; root, containers and scalar values each count as nodes, while
object keys do not. Keys still count toward input bytes. The input byte limit is
the exact compact JSON UTF-8 size, including quoting/escaping, numbers and
punctuation, counted without retaining a serialized input copy. Shape and input
size are checked before backend rendering or canonical key-reference allocation.
Over-limit input fails before rendering.

Objects are recursively sorted lexicographically by UTF-8 keys in both formats;
arrays preserve order. The adapter borrows the input Value and sorts references
per object, without adding a Graphit whole-tree clone or changing input. Presence
is complete: `false`, `0`, `null`, empty strings/arrays/objects are preserved.
Signed/unsigned 64-bit integers remain exact, including I64_MIN/U64_MAX; supported
finite nonintegral f64 values retain numeric precision. TOON integral floats and negative zero normalize to their numeric
value without rounding to two decimal places; number lexeme, sign bit and integer-versus-integral-float type are not
TOON data promises.

### Rendering and allocation boundaries

JSON is compact and uses a bounded, fallibly grown Vec writer. TOON uses the
published official `toon-format = "=0.6.1"` with default features disabled,
specification 4.1 defaults: comma delimiter and two-space indentation. Upstream renders
a complete String with intermediate tree/line allocations. Its output byte cap
is checked **after rendering**, before publication: it is a publication limit,
not a peak-memory cap. There is no streaming or all-allocation-fallible guarantee.
Preflight depth/nodes/input bytes bound the admitted document, not every transient
allocation or allocator overhead. An output-budget/encoding error returns no
partial document; there is no truncation or automatic JSON fallback.

Callers must build already-authorized, projected/redacted DTOs and own response
schema, pagination, envelope, notices/reminders and sink selection. Passing a raw
Value containing a secret can encode that secret; this codec does not discover
or redact secrets automatically. It does not query environment/TTY, log, select
MCP content blocks or implement terminal printer/progress behavior.

### MCP output selection

Structured Root, Role and Live MCP results use this codec with optional top-level
strict boolean `ai_optimized`. Omitted/`true` selects TOON in a text block without
a duplicate JSON `structuredContent`; `false` selects JSON for machine consumers.
The JSON-RPC transport, arguments, catalogue and schemas remain JSON, as do the
existing native HTTP/CLI JSON consumers. Root AST Source and SessionHook remain text.
ModuleSkill, Knowledge Source and Role Source encode their metadata envelopes
while preserving original content strings. See [MCP output and errors](mcpstdio_module.md#structured-output-and-errors).

These are representation choices, not query optimizations. Neither format is
promised to always use fewer model tokens; content and tokenizer affect the
result. Existing limits and projection/redaction duties remain unchanged.

An MCP formatting failure can follow a completed write. The tool reports a
bounded textual `isError` diagnostic (for example `completed; response_limit`),
without partial output, JSON fallback or rollback claims. Verify the stored
state before repeating a write. Invalid policies or bounds too small to carry
the diagnostic produce an internal protocol error with `tool_completed`,
`tool_reported_error` and `reason`; domain errors remain tool errors.

### Focal token measurements

Five synthetic, representative payload shapes were encoded with the Core codec
and counted with `tiktoken 0.12.0`, tokenizer `o200k_base`. The comparison counts
compact JSON payloads versus TOON text only, excluding the MCP envelope.

| Synthetic payload | JSON tokens | TOON tokens | Change |
| --- | ---: | ---: | ---: |
| Task query, 24 rows as arrays | 383 | 465 | +21.4% |
| AST, 24 homogeneous objects | 730 | 496 | −32.1% |
| Nested Memory result | 96 | 106 | +10.4% |
| Source envelope with text | 762 | 765 | +0.4% |
| Heterogeneous collection | 26 | 40 | +53.8% |

These focal measurements are not a general token-saving promise or a comparison
of model quality. Shape and tokenizer matter. Format selection remains explicit:
the encoder does not switch formats automatically to hide an increase.

An isolated SDK MCP client also measured actual responses from the installed Linux
Release on 2026-10-29, using the same tokenizer. It verified JSON/TOON data
equivalence before counting. Payload counts exclude MCP framing; result counts
encode the complete SDK `CallToolResult` as compact JSON, excluding JSON-RPC and
HTTP envelopes.

| Installed tool/sample | JSON payload | TOON payload | Payload change | JSON result | TOON result |
| --- | ---: | ---: | ---: | ---: | ---: |
| Task schema | 270 | 162 | −40.0% | 580 | 184 |
| AST module skill envelope | 3046 | 3045 | ≈0% | 6326 | 3277 |
| Task query, three `id`/`status` rows | 108 | 130 | +20.4% | 246 | 151 |

The full-result reduction includes removal of duplicate JSON: JSON mode retains
the existing text plus `structuredContent`, while TOON has one text document.
That reduction is distinct from compression by the TOON syntax itself. A client
may project MCP results differently before sending them to a model; these are
identified tokenizer measurements, not a promise about every client's context.

## Rust terminal presentation

`crates/graphit-core/src/terminal.rs` implements explicit terminal presentation.
The library does not install an implicit global logger, signal handler or scheduler.

### Explicit printer, modes and messages

`Printer<W>::new(writer, prefix, Options { mode, limits })` receives a
`Write + Send` destination, immutable bounded prefix and mandatory options:
`Mode::Plain`, `Mode::Silent` or `Mode::Terminal { columns: NonZeroU16, color }`.
No Default invents product limits. Clones share sink/state through Arc/Mutex;
separate printers stay independent. `resize` changes captured terminal columns
without an OS query. Debug/errors reveal no prefix/message/sink payload and
there is no automatic Serialize.

`print(Message)` supports every declarative variant:

| Message | Meaning |
| --- | --- |
| `Line { severity, text }` | Info, Success, Error, Warning or Running line |
| `Step { severity, text }` | Step line with an optional severity |
| `Detail { key, value }` | Semantic key/value detail |
| `KeyValue { key, value }` | Key/value line |
| `Count { amount, singular, plural }` | u64 count with caller-supplied labels, without inventing an English suffix |
| `Bullet(text)` | Bulleted line |
| `Header(text)` | Title block |
| `Blank` | Blank line |
| `Data(text)` | Exact raw content plus newline, without prefix/style |
| `Table { headers, rows }` | Two columns aligned by cells |

`write_data(text)` is a separate raw chunk channel with no appended newline.
Semantic fields escape C0/C1 controls, including NL/CR/tab/ESC, rather than
executing data as terminal controls. Data/write_data deliberately retain raw
characters; Plain does not remove caller-supplied ANSI on those channels. Safe
DTOs/redaction remain the caller's responsibility.

ANSI styling uses the existing semantic palette: green/bold success,
red/bold error, yellow/bold warning, cyan running, magenta values, bold headers
and BrightBlack tags/steps. Colors come from the user's terminal palette.
Plain retains symbols/text without generated color/motion; Silent performs no
sink Write/flush and creates no workers.

### Cursor, budgets and write failures

`progress(text)` rewrites one transient Terminal line; Plain appends history.
The full prefix/status/text/spinner frame is measured in Unicode cells and
truncated at grapheme boundaries to `columns - 1`, with an ellipsis when space
allows. Width one draws an empty frame. Clipped frames sum grapheme widths
conservatively, which can leave cells unused for contextual ligatures; they do
not split a cluster or wrap on the right margin. Combining, CJK and emoji/ZWJ
sequences use pinned Unicode libraries rather than rune-count approximations.
`end_progress` is idempotent. Ordinary messages and completion clear a transient
line first. After an unterminated raw stream chunk, a semantic message/progress
starts a newline boundary so it does not overwrite streamed output.

`Limits` provides input bytes, output bytes, table rows and active tasks. Tasks
must be at least one; other caps may be zero. Aggregate input/prefix where
applicable, row counts and rendered output are validated before the operation's
first write. Header/Table remain a complete operation under the same mutex
through write_all/flush, so clones/tasks do not interleave blocks.
The reusable output buffer is bounded by `max_output_bytes`; semantic-frame
scratch is bounded by `6 * max_input_bytes + 32`, with checked arithmetic and
fallible buffer/text reservations. Prefix and active task messages are bounded;
updates may hold old and new messages until swapping. Arc/std/allocator overhead
is additional, so this is not a universal peak-RSS/all-allocation-fallible claim.

`TerminalError` has fixed InvalidLimits/InputBytes/OutputBytes/TableRows/TaskLimit/
Allocation/Poisoned categories and I/O kind/OS code without raw messages.
Logical validation failures publish no bytes. A sink failure can follow a partial
write or failed flush; the library does not roll back or automatically retry the
whole operation. Callers choose recovery/exit policy, and explicit cleanup can
still be best-effort after an I/O failure. `Write` can block: I/O deadlines and
backpressure belong to the caller/adapter; this library promises no write timeout.

### Progress tasks and consumer driver

`start_task` returns a cloned owned `ProgressTask` within `max_tasks`. Plain
prints Running once; Terminal renders a frame. `update` changes bounded stored
text without printing. `tick` manually advances Terminal output and does no I/O
in Plain/Silent. The library owns no worker, timer or Tokio runtime. A caller's
single scheduler **must drive ticks before animation consumers cut over**;
manual ticks/tests do not prove an integrated idle animation.

`finish(Success|Failure|Cancelled, message)` accepts the first valid completion
across clones, clears progress and prints status/monotonic elapsed. Invalid
input/render limits leave the task open for a smaller retry. An I/O error after
completion begins closes lifecycle and does not retry status. Later finish,
update and tick calls are no-ops. Finish releases the active slot and message
allocation even on sink failure while handles remain retained; retained handles
still carry Arc overhead outside the active-task budget. The last Drop of an
unfinished task releases its slot and requests best-effort clearing only when
that task owns the visible cursor, without inventing success or closing others.
Task→Printer lock order avoids reverse acquisition.

Pins are `anstyle = "=1.0.14"` and `unicode-width = "=0.2.2"`, both with default
features disabled, plus `unicode-segmentation = "=1.13.3"`. No new transitive
packages were added transitively; metadata confirms 105 workspace packages and
empty features/dependencies for each new pin. The final Linux gate passed 166
entries (163 regular tests plus three compile-fail doctests), including three
private terminal tests and eleven public integration cases. Formatting/Clippy,
Windows GNU all-target compilation/Clippy and actual negative Git hooks passed.
The cases cover modes/variants, blocks/concurrency, short/partial I/O/flush
failures, limits, Unicode/control/raw channels, concurrent finish, retained
finished-handle payload release and Drop/retry behavior. Release measurements
used a counting/discard sink. Terminal-device performance and native platform
behavior require their own measurements.

## Runtime tracing

The CLI installs the existing `tracing_subscriber` global subscriber in
`crates/graphit-app/src/main.rs`. Its environment filter defaults to OFF and its
formatter writes to stderr without ANSI, including span NEW/CLOSE events.
Core emits `tracing` events and spans through that subscriber. The custom Core
JSONL collector and its private dispatch, budget and health-counter APIs have
been removed because they had no production consumer.
The retired scoped collector also encountered first-callsite caching loss in
`tracing-core` 0.1.36; removing it does not claim to repair that upstream behavior.

Native writer instrumentation retains fixed phase labels and redacted caller
context; see [Native ArtifactWriter request ownership](native_artifact_writer_request.md).
Diagnostic output does not grant authority or change publication, cancellation
or recovery. The bounded LogFile sink below retains its separate file IO contract.

## Rust log-file sink

`crates/graphit-core/src/log_file.rs` implements a separate bounded line sink
with explicit native file ownership and error handling.

### Paths, options and records

`LogPaths::project(&ProjectPaths, ProjectId, channel)` derives
`<global>/projects/<PROJECT_ID>/logs/<channel>.log` using the captured identity;
`LogPaths::installation(&InstallationPaths, channel)` derives
`<installation>/logs/<channel>.log`. A channel is 1–64 lowercase ASCII alnum,
underscore or hyphen, with portable-device/traversal exclusions.
`LogPaths::explicit(absolute_path)` captures an existing canonical parent and
native basename. A basename starting `.log-`, compared ASCII case-insensitively,
is reserved and rejected before I/O. Callers resolve relative overrides and
prepare their parent explicitly; there is no legacy or alternate-sink fallback.
Branches share the selected channel; version is a producer field, not a subdir.

Project LogPaths additionally retains the explicit global PROJECT directory.
Preparation qualifies this slot through the shared private-directory helper
before the usual existing-directory skip: missing state uses exclusive SDK
private birth, while existing state is verified through the same no-follow FD
without chmod or repair. The returned File is held only during preparation,
not by LogPaths or LogFile afterward. A clone of prepared paths still qualifies
its runtime through read-only `open_private_directory`; absent runtime fails
without creation. Checkouts of the same PROJECT share these logs; another
PROJECT ID or global root selects a separate namespace. The helper does not
read or create project identity or configuration. Installation/explicit-directory
policies retain their previous behavior. Tail reads do not run preparation.

`LogFile::open(paths, Options { limits, durability, lock_timeout }, cancellation)`
validates options/cancellation before side effects and opens without truncating
or compacting old data. Clones retain context/policy/cancellation, never an open
file descriptor. Required limits satisfy:

- `max_record_bytes > 0` and `max_file_bytes > 0`;
- `max_record_bytes <= max_batch_bytes <= max_file_bytes`;
- `retain_bytes <= max_file_bytes - max_batch_bytes`, including zero retention.

Overflow, conversions and monotonic timeout overflow are checked before I/O.
`append(bytes)` accepts an explicit UTF-8 batch of complete LF-terminated lines;
each record's limit includes LF. Empty batches are no-ops without I/O. CR and
other controls are caller data, not parsed JSON or scrubbed secrets. The caller
must supply authorized/redacted content. `AppendOutcome` reports records,
compacted, recovered_bytes and discarded_bytes for that call.
`Write::write` delegates append and returns full length only on success;
`Write::flush` delegates `sync()`, with no hidden queue or Drop retry.

### Coordinated append, recovery and compaction

The canonical parent and SHA-256 of the basename's native encoded bytes select
one permanent `.log-<64-hex-digest>.lock`. `path()`/`lock_path()` expose explicit
locations; Debug/errors omit them. The sidecar must never be unlinked/replaced
while holders or contenders exist. Cooperating writers use the same target and
budgets. Directory aliases are canonicalized; hardlink/case aliases and hostile
replacement races are not certified coordination or sandbox guarantees.

Every operation acquires FileLock through the internal existing-parent route
and opens the active file fresh under that lock, avoiding stale descriptors
after another writer's atomic replacement. A parent removed after the
preliminary check is not recreated during acquisition. Observed target/lock
links and nonregular files fail, and the opened target is rechecked.

The project runtime has the qualified private boundary described above.
Other new Unix managed directories use 0700; their existing permissions remain
unchanged. Active/temp files retain the existing 0600-before-payload policy.
Writable active files are hardened to 0600, while read-only files fail.
Windows ACL inheritance for these other directories/files is no claim of
equivalent Unix permissions or executed native runtime.

A final LF needs only an EOF byte probe. Otherwise recovery reads at most
`max_record_bytes + 1`, removes the incomplete EOF fragment at a known LF
boundary, and preserves complete prefix lines. A first fragment within the cap
may truncate to zero; a fragment without a bounded known boundary is Corrupt and
is not erased. New batches and retained suffixes are validated as UTF-8; the
old complete prefix is not scanned/validated in full. Crash recovery is not a
batch transaction: complete lines from a
killed batch may survive. New records never concatenate with an old fragment.

When recovered length plus batch fits, append uses write_all under the lock.
Write errors attempt rollback to pre-append length once; rollback failure is
reported. Sync errors do not roll back completed bytes. If the cap would be
exceeded, retention reads at most `retain_bytes + 1`, drops a cut first line and
keeps recent complete UTF-8 LF records. A same-directory 0600 tempfile receives
suffix and batch separately, then atomically replaces the active file. The old
FD is closed before replacement; successful append leaves the active file
within `max_file_bytes`. No entire growing file or combined payload is cloned.

Only factory open and the compaction path clean abandoned regular tempfiles,
under the same lock. The reserved name is `.log-<same-digest>.compact-` followed
by exactly sixteen ASCII alnum characters. Cleanup streams directory entries
and preserves symlinks, other targets and user files; it does not scan for every
record/batch. A crash orphan can remain until the next factory/compaction, so
this is not a whole-directory/global disk cap. The lock is never cleanup output.

### Durability, failures and proof boundary

`Durability::Buffered` writes to the kernel without append fsync;
`SyncData` sync_data follows append or precedes temporary-file publication.
Explicit sync/flush reopens under the lock and calls sync_all. LogFile Drop
performs no I/O and owns no worker/buffer/FD. Lock waiting is cancellable and
bounded, while OS/filesystem calls are not hard interruptible deadlines.

Errors are fixed InvalidOptions/InvalidPath/RecordLimit/InvalidRecord/Corrupt/
Allocation or Io { stage, kind, os_code, rollback_failed }. Stages are
Directories/Lock/Open/Read/Recover/Append/Compact/Sync/Unlock. A primary error
wins over cleanup/unlock error; Sync/Unlock may report failure after bytes exist.
Errors/wrappers preserve I/O kind but omit paths, raw dependency messages and
payload. No whole-batch retry or fallback is automatic. Same-directory replace
is old/new visibility, not parent-directory fsync, power-loss durability or proof
of OS file-close error handling. Adapter failure policy remains explicit.

## Rust log-tail reader

`crates/graphit-core/src/log_tail.rs` implements an observational, read-only
reader under `tsk-24fc`, with library gates verified. It shares the explicit
[LogPaths contract](#rust-log-file-sink) with the writer, without calling the
writer's factory/prepare, acquiring a lock, creating state or changing permissions.
### Read API and result

`log_tail::read(&LogPaths, Limits { max_read_bytes, max_lines }, &Cancellation)`
returns `Result<Tail, LogError>`. Limits are required without defaults. Numeric
validation runs before I/O or the zero-budget shortcut: checked
`4 * max_read_bytes + min(max_lines, max_read_bytes) * size_of::<String>()` and
usize-to-u64 conversion reject InvalidOptions. Either zero budget returns empty
Tail without I/O even for cancellation/missing paths, after numeric validation.
A large max_lines remains bounded by the effective raw-byte row budget.

A missing file/parent returns NotFound, rather than empty success; an existing
empty file returns an empty Tail. Observed parent/target links and special files
are rejected through shared guards; the opened file's regular type is rechecked.
No complete ancestor/hostile-race/hardlink/case-alias sandbox is promised.

| Tail field | Meaning |
| --- | --- |
| `lines: Vec<String>` | Selected rows, chronological oldest to newest |
| `read_bytes` | Raw bytes actually read, including the boundary peek |
| `byte_windowed` | Initial file length exceeded the raw-byte budget; not a row-limit flag |
| `final_unterminated` | Last returned row had no final LF; not a writer-completion assertion |
| `replaced_utf8` | Invalid UTF-8 was replaced in returned rows only |

Tail Debug displays counts/flags, never lines/path, and there is no automatic
Serialize. The caller chooses authorized DTOs and safe rendering; the reader
neither redacts secrets nor interprets JSON/ANSI/control characters or auth policy.

### Bounded window and uniform text

The reader observes length on one opened read-only FD, seeks to the final bounded
window and reads only remaining allocated capacity. A short EOF returns actual
filled bytes, without padded zeros or a second whole observation. The first raw
byte of a clipped window is an in-budget boundary peek: LF admits the following
rows; otherwise the peek and remainder up to the next LF are discarded. No
byte outside the raw cap is read. Without a boundary, a cut starting fragment
is omitted; a clipped window can expose at most budget minus one eligible bytes.

Text removes one terminal LF separator, selects the newest max_lines, removes
one terminal CR per row and reverses the selection into chronological order.
Internal blank rows and a single-LF blank row are preserved. A final unterminated
row may be returned and flagged. Internal CR/C0/C1 are data, requiring caller
rendering policy. It is a coherent text contract without an old splitter/byte compatibility mode.

Only selected rows are decoded through standard `utf8_chunks`. Each invalid
fragment becomes U+FFFD; invalid omitted rows do not set replaced_utf8. Exact
encoded length is checked before fallible String reservation, with no whole-file
String/lossy Cow clone. Raw storage is at most max_read_bytes, total decoded text
at most three times that budget and row metadata at most
min(max_lines, max_read_bytes). The raw window can coexist with decoded results;
paths, File/stdlib/allocator overhead remain outside a hard-RSS guarantee.

### Observation, errors and delivery boundary

Append after the observed length is excluded. Atomic replacement can leave the
FD reading its old file; truncate between metadata/read can yield short/empty or
mixed observations. There is no lock, worker, whole-record guarantee or
transactional snapshot. Regular filesystem calls can block, and file-close
errors/hard filesystem deadlines are not certified.

Shared LogError categories/stages and I/O kind/OS code are fixed and sanitized,
without payload/path/raw dependency text or fallback. Invalid UTF-8 follows the
text replacement policy rather than Corrupt. Cancellation checks before/within/
after read return Stage::Read/Interrupted; an interrupted syscall can retry with
a cancellation check, without retrying the entire observation. Permission and
other read errors remain distinguishable from empty success.

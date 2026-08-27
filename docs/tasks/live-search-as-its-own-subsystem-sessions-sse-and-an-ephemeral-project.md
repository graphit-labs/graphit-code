# Live search becomes its own subsystem: server-side sessions, SSE, and a throwaway project per search

## Date

2026-08-13

## Problem

The live search worked by running an agent inside an HTTP handler. Every property of
the run was therefore a property of a TCP connection:

1. **Closing the tab killed the work.** So did a proxy timeout, a laptop lid, a
   flaky network. The run had no existence apart from the request that started it.
2. **The expensive part was thrown away on disconnect.** Preparing what the agent
   searches — downloading artifacts, compiling indexes — costs most of the wall clock.
   A disconnect discarded it and the next attempt paid again.
3. **Nothing could be watched.** A request that returns a string has nothing to show
   until it is over, and no way to tell a slow run from a hung one. For a run that
   lasts minutes, that is the whole experience.
4. **The agent ran in the wrong directory.** `exec.CommandContext` was invoked with no
   `cmd.Dir`, so the CLI inherited the server's working directory. An agent CLI
   discovers its rules, its skills and its MCP servers *from the working directory*, so
   it was answering with whatever project the server happened to be started in.
5. **The agent was told not to use tools.** The non-interactive preamble forbids tool
   use — correct for a one-shot question, fatal for a search whose premise is that the
   graphs and wikis are there to be queried. It answered "I would query the graph"
   instead of querying it.
6. **Only one CLI could stream, and none did.** `Complete` buffered everything.

## Root cause

The search was modelled as a request. It is a session: something with a lifetime, a
state, a transcript, and work that continues whether or not anyone is looking.

## Changes

### A session runtime that owns its work

`internal/livesearch/session.go`. A `Session` holds a context derived from
`context.Background()` — never from a request — and the goroutine that runs a turn
answers to the session. Clients may disconnect, reconnect, or never come back.

States are `preparing → ready → running`, with `failed` for a session that cannot work
at all and `closed` as terminal. A turn that *errors* returns to `ready`: the prepared
project is still valid and the user can ask again, so failing the session would throw
away the expensive part over a retryable error.

Five invariants hold the concurrency together, and each exists to prevent a specific
defect:

1. **`Subscribe` is one call, not two.** Reading history and then registering for new
   events loses whatever arrives in between; registering and then reading delivers it
   twice. So registration and the snapshot of "how far history goes" happen under one
   lock, and every event is attributed to exactly one phase: the file replay stops at
   the snapshot, the live channel skips anything at or below it.
2. **Two mutexes, in one order.** `mu` guards state and metadata; `subMu` guards the
   subscriber set *and* serialises appending to the log with broadcasting, which is
   what makes sequence order and delivery order the same order. They are separate
   because a state change persists under `mu` and *then* emits — one lock for both
   deadlocks on itself. `mu → subMu`, never the reverse.
3. **A slow subscriber is dropped, never waited for.** Broadcast is
   `select`/`default`; a full buffer means the channel is closed and removed. A
   stalled browser tab cannot stop the agent. The client reconnects and resumes from
   the log.
4. **`Close` waits for the work to stop.** Without it, `Remove` deletes the directory
   from under an active writer — which fails outright on Windows. The turn's
   `work.Add(1)` happens *inside* the `mu` section of `Send`, because a `Close`
   arriving in the gap would otherwise see no work pending, return, and let a turn
   start on a session already torn down. And since `Close` waits for a turn that ends
   by reporting itself `ready`, `closed` is terminal in `setState` — otherwise the
   unwinding turn resurrects the session that was just destroyed.
5. **Sequence recovery trusts the record, not the line count.** A process killed
   mid-write leaves a partial line; counting lines hands the next event a number that
   is already taken, and two events sharing one SSE id means a reconnecting client
   silently loses one.

### A durable event log

`internal/livesearch/event.go`. Append-only JSONL at
`~/.<brand>/sessions/<id>/events.jsonl`, one event per line, sequence number = SSE id.
A truncated final line is repaired on open and skipped on replay: one corrupt record
from an interrupted write costs that record and not the history.

Two agent events are deliberately **not** published. `ai.EventSession` carries the
CLI's own conversation ID, which the session records to resume the next turn and has no
reason to put on the wire. `ai.EventDone` ends one CLI invocation, which is not the same
event as the turn being over — the session emits `turn_done` itself, *after* recording
the outcome, so "done" always arrives after the error that explains it.

### Streaming for every CLI, by construction

`internal/ai/stream.go`, `internal/ai/cli_stream.go`. Two tiers:

- **Tier 1, universal.** Every CLI in `knownCLIs` writes its answer to stdout, so
  reading stdout incrementally streams all of them without knowing anything about any
  of them — and **without changing a single argument**, so no invocation that works
  today can break. Reads are byte-chunked rather than line-based, because a final
  paragraph is often one long line.
- **Tier 2, optional per CLI.** A CLI that can do better declares a structured mode and
  gets tool-call events and its own session ID. Only `claude` declares one today.

A structured CLI that returns plain text — an older build without the flag, a wrapper —
falls back to treating the output as text. This was found by a failing test: without it
the answer came back **empty, with no error**. An individual unparseable line is still
ignored, because formats gain event types between releases.

`CompleteStream` sets `cmd.Dir`. That single line is the pin the rest depends on.
`Complete` is untouched, so `wiki search` behaves exactly as before.

### SSE, not WebSocket

`internal/uiserver/live_handler.go`. Watching is one long `GET`; sending and cancelling
are small POSTs. WebSocket would also work and buys nothing here: there is no
high-rate client traffic to justify a second protocol, and reconnect-with-resume would
have to be designed instead of inherited.

Six SSE traps, each with a fix in the code:

1. `corsJSON` declares `application/json`, and a stream that says it is JSON is a
   stream `EventSource` refuses. `corsSSE` exists for this.
2. `http.Flusher` is mandatory; without flushing per event the response is buffered and
   nothing arrives until the end.
3. `X-Accel-Buffering: no`, or an intermediary may hold the whole response and deliver
   it at the end — indistinguishable from a hung server.
4. Resume takes **both** the `Last-Event-ID` header and a `last_event_id` query
   parameter. The browser sends the header by itself, but only on a reconnect *it*
   initiated; a page that was reloaded cannot set headers on `EventSource` at all.
5. A heartbeat comment every 25s. Preparation runs for minutes without an event, and an
   idle connection is what proxies reap.
6. `data:` is one line of JSON. SSE ends an event at a blank line, so a raw newline
   inside a value splits one event into two unparseable halves.

A stream for a session this process does not own is replayed **from disk**. The durable
log exists so a transcript survives its process; the session list already shows those
sessions, so refusing the stream would offer a session that cannot be opened.

### A throwaway project per search

`internal/livesearch/prep/workspace.go`. Layout:

```
~/.<brand>/sessions/<session-id>/
  session.json     metadata
  events.jsonl     the durable log
  workspace/       the ephemeral project
```

`workspace/` is a subdirectory rather than the session directory itself, because the
indexers run over the project root — sharing one directory would have the session
indexing its own transcript and then answering questions about it.

**`hub.OnInit` is not called**, though it performs most of the right steps. It
*tracks*: a `project.init` event into the Hub's git store, and baseline installs that
register themselves in the global lock. A throwaway project in a user's permanent
records is what "anonymous" has to rule out. The steps that do belong are performed
here: lockfile and identity, IDE list, the five module rules and skills, and the
mandate.

The project identity is written explicitly, for two independent reasons. It must not be
empty, because `reconcileMCPFile` treats the project ID as a claim on each MCP server
and deletes any server nothing claims — an empty ID has the entry written and removed
in one pass. And leaving it to `SaveLockfile` would call `resolveProjectIdentity`,
which runs `git remote get-url origin` in the directory; git walks *up*, so on a
machine where the home directory is a git repository the throwaway project would be
named after the user's dotfiles.

**MCP configuration is project-local.** Every `MCPFilePath` the adapters declare is
under the home directory. Writing there would be wrong twice: it adds a throwaway
project's claim to the user's real configuration, and `reconcileMCPFile` is an unlocked
read-modify-write — with concurrent sessions, two created in the same moment can drop
each other's claims, and a claim lost is a server deleted from under the user's own
project. Only IDEs whose project-level convention is known are written; one that is not
is **reported** rather than guessed at, because a config written to the wrong path is
indistinguishable from one never written and costs an hour to find.

Tool permissions are allowed at **server** level, not as a list of tool names: the
graphit server alone has seventy-odd tools and gains more each release, so a list would
be wrong by omission the first time one was added — and the failure is an agent that
stops to ask permission in a run nobody is watching.

### Any Hub artifact, installed without tracking

`hub.NewUntrackedHubService` is `&HubService{registry: registry}`. `Install` tracks in
exactly two places, both collaborators of the service: `lockMgr.RegisterInstall` is
already nil-guarded, and `TrackEvent` already tolerates a nil receiver. So leaving both
nil installs everything and records nothing. Resolving versions, cloning, placing
artifacts, writing the project's own lockfile and installing dependencies all happen
exactly as for a real project.

The global artifact clone and the shared AST store are **caches, not records**, and are
still used — that is how a second project gets an artifact without downloading it.

One artifact failing is reported and the rest are installed. **Zero** installable
artifacts fails the preparation, because an empty workspace makes the agent answer
"the sources say nothing about that" — which reads as a fact about the sources rather
than about the download.

### The daemon's work, done inline

`internal/livesearch/prep/index.go`. A real project has a daemon; a session created for
one search does not, and has no time for one.

- **Documentation: one pass over all of it.** A Hub knowledge artifact ships markdown,
  never a compiled database (`wiki.db` is gitignored, so it never travels), and the wiki
  search does not fall back to reading markdown — it opens `wiki.db` in exactly one
  place. The generator is incremental and prunes cache entries whose source has
  disappeared, so compiling each artifact in its own pass would make the second pass
  treat the first artifact's pages as deleted, leaving only the last selection
  searchable. Pointing the root at the parent directory makes every artifact a subtree
  of one build — which is also what was asked for: one search across everything
  selected. The output lives inside the input root and is excluded explicitly, because
  relying on it being empty during the walk works today and breaks the first time
  anything rebuilds an existing session.
- **Code graphs: nothing to build.** The install already builds the shared versioned
  store and writes the lockfile entry that resolves it. Preparation only says which
  graphs are reachable, so that "resolved nothing" and "nothing was chosen" are
  distinguishable.
- **The user's memory, with an explicit destination.** `MemoryService` resolves its own
  copy destination with `gitProjectDir()` — `git rev-parse --show-toplevel` or the
  process working directory, both process-global. In a server running several sessions
  that is not imprecise, it is a different project's directory. So the service builds
  the global wiki and the destination is chosen here.

There is no project wiki, project memory or project AST: the workspace has no code and
no documentation of its own.

### An interactive CLI

`cmd/graphit/commands/live.go`. `graphit live [question] -a [<type>:]<id>[@<version>]`,
plus `live sessions` and `live remove <id>`. It runs the manager in-process: a server is
a thing to start, a port to find and a process to leave running, and a question asked
from a terminal should not need any of it. Both front ends share everything that decides
behaviour, so an answer in the terminal and an answer in the browser are the same answer.

Three defects here were found only by running the binary, not by its unit tests:

1. **`main()` installs a global SIGINT handler that exits the process.** `signal.Notify`
   delivers to every registered channel, so the process would die before the turn was
   cancelled — the behaviour promised in `--help` could never happen. Fixed with
   `signal.Reset(os.Interrupt)` before registering. The unit tests inject a fake signal
   channel and never exercised the real path.
2. An artifact with no explicit type printed as `name ()`. The type is optional
   *because* the registry resolves it, so an absent one is a normal choice, not a
   missing value.
3. The failure reason was printed twice — once as an event, once as the returned error.

An interrupt during an answer cancels and **keeps reading**, because cancelling is what
produces the events that say it was cancelled; returning immediately would leave the
screen looking like the agent stopped for no reason.

### UI

`internal/ui/src/api/live.ts` and `internal/ui/src/components/live/LiveSearchPage.tsx`,
at `/live`. The picker lists every artifact type the registry carries, and each session
has a remove button.

The IDE is **not** selected here. It is one application-wide setting, held in the app
store, chosen in the project switcher and persisted with the rest of the state — the
same one the Hub pages install with. A selector on this page would be a second source
of truth for one question, and the moment the two disagreed a session would be prepared
for conventions the user is not working in. The page reads the setting, shows which one
will be used, and says where to change it.

The transcript is **derived** from the event list by one function, so a live stream and
a replay after a reload produce the same screen — two code paths for that would drift,
and the one that drifted would be the one nobody watches. Answers render pre-wrapped
rather than as markdown, because chunks arrive mid-token and a half-written table
renders as noise.

`EventSource` has no wildcard listener: a named event reaches only the listener
registered for that name, never `onmessage`. The kinds are enumerated in the client,
which is safe because the UI is compiled into the same binary as the server.

## Not changed

`graphit wiki search` and its `/api/wiki/*` routes: the page browser and the
single-wiki AI search behave exactly as before. `ai.Complete` is untouched.

## Removed

The previous model in full — `internal/astprobe`, `wiki/search_briefing.go`,
`wikisvc/ast_sources.go`, `uiserver/code_graphs.go`, `uiserver/search_handler.go`,
`api/search.ts`, `WikiSearchPage.tsx`, and the wiki explorer's follow-up chat, which
called endpoints that no longer exist. Two mechanisms for one job is how the wrong one
survives.

`uiserver.NewLiveManager` was replaced by `livesearch.NewManagerFromConfig`, so the CLI
and the server construct the manager by one path.

## Cross-platform

`Close` waits for writers before `Remove` deletes, because Windows refuses to remove a
file with an open handle. Session IDs are validated as ULIDs before being used as a path
segment, since every ID arrives from a URL. Tests isolate `HOME` **and** `USERPROFILE`.

Verified: `GOOS=windows GOARCH=amd64 CGO_ENABLED=1` builds `./internal/...`. No
osxcross is available locally, and `GOOS=darwin CGO_ENABLED=0 go vet` cannot cover these
packages — `onnxruntime_go` excludes all files under that combination, which is
pre-existing and applies equally to `internal/ai` and `internal/wikisvc`.

## Testing

Go: 20 tests for the streaming clients, 30 for the session runtime and log, 32 for the
ephemeral project, 24 for the HTTP surface, 20 for the CLI. The concurrency-sensitive
ones were run at `-count=15` under `-race`.

Tests worth naming:

- `TestTurnOutlivesTheSubscriber` and `TestTheRunSurvivesTheClientDisconnecting` close
  the stream mid-run, release the agent, then reconnect and read from the log what was
  produced while nobody was watching.
- `TestSubscribeMidRunHasNoGapAndNoDuplicate` subscribes while events are being
  appended and asserts the delivered sequence is unbroken and duplicate-free.
- `TestNothingOutsideTheSessionEverLearnsItsID` walks the whole home tree, names and
  file contents, for the session ID. Its first version checked `global.lock.json` by
  name and passed trivially — asserting on named files only proves the files the test
  thought of are absent.
- `TestEveryChosenDocumentationSetSurvivesTheSameBuild` guards the pruning trap.
- `TestATranscriptFromAnotherProcessIsStillReadable` forgets a session as a restart
  would and reads it back over HTTP.
- `TestCompleteStream_CoversEveryKnownCLI` iterates `knownCLIs` plus an unknown binary,
  so streaming coverage is by construction rather than by enumeration.

End-to-end against a running server: session creation, `400` for a missing or
unsupported IDE, `404` for an unknown session, the live stream with its `retry` frame
and per-event ids, resume at a given `last_event_id`, replay of a session created by the
**CLI** in a different process, `cancel`, and `DELETE` followed by `404`.

Frontend: `tsc` clean, `eslint` clean, production build, 42/42 vitest.

## Known limitations

- Project-level MCP configuration is written for `claude`, `cursor` and `kiro`. Other
  IDEs are reported, not guessed: documentation and memory are still readable as files,
  but a code graph needs the MCP server.
- Tool permissions are written for `claude` only, the one format that is known.
- `MemoryService.EnsureInitialised` also performs its own copy into whatever project the
  server was started in. That is the same write a plain sync in that project performs,
  so it cannot leave the project in a state its own tooling would not produce. Avoiding
  it would mean reaching past the service into its private sequencing.
- `TestMemoryGitStore_CreateOrphanBranch_Full` flakes on `t.TempDir` cleanup under load
  (git's background gc racing the removal). Pre-existing, not a regression; filed in
  `docs/tasks/backlog/`.

## Progress Log

### 2026-08-14 — the inline compile was removed

Two of the three jobs described under *The daemon's work, done inline* are gone, and the
claim at the end of that section — *"There is no project wiki, project memory or project
AST"* — turned out to be an intention rather than a fact. The workspace has a lockfile
with an identity, which is what every store resolver keys on, so the session was
indistinguishable from a real project and acquired all three: the documentation wiki
eagerly, the memory scope and the code graph on first use.

What changed:

- **Documentation is no longer compiled.** A knowledge context now arrives already
  compiled, so `compileKnowledgeWikis` was deleted along with `WikiScope.Subdirs`, which
  existed only to serve it. A session reads each set where it was installed, by name.
- **There is no project-level read at all**, for documentation or code. Both require
  naming a context, which is what the session actually has.
- **The memory scope is redirected to the user's** rather than created.
- **`live remove` reclaims** what earlier runs of this code left behind.

Full detail, including the measured chain that created the orphan git branch:
[An ephemeral session owns no store](an-ephemeral-session-owns-no-store.md).

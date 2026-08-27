package ai

import (
	"context"
	"time"
)

// Streaming exists because the live search stopped being one question and one answer.
// It runs an agent in a prepared project for minutes: downloading artifacts, compiling
// indexes, then a multi-turn agentic session. Buffering all of that into a single
// string — which Complete does — means the caller has nothing to show until it is over,
// and no way to tell a slow run from a hung one.
//
// Coverage is by construction, not by enumeration. Every CLI in knownCLIs writes its
// answer to stdout, so reading stdout incrementally streams *all* of them without
// knowing anything about any of them, and without changing a single argument — so no
// invocation that works today can break. A CLI that can do better declares a
// structured mode in its spec and gets tool-call events and a captured session ID on
// top. Nothing is required to declare one.

// EventKind classifies a stream event.
type EventKind string

const (
	// EventText is a chunk of the assistant's answer. Chunks concatenate to the
	// full text; no chunk is a complete unit of anything.
	EventText EventKind = "text"
	// EventThinking is reasoning the CLI chose to surface separately from the answer.
	EventThinking EventKind = "thinking"
	// EventToolUse reports that the agent invoked a tool. Only CLIs with a
	// structured mode emit these.
	EventToolUse EventKind = "tool_use"
	// EventToolResult reports what a tool returned.
	EventToolResult EventKind = "tool_result"
	// EventSession carries the CLI's own conversation ID, learned rather than
	// assumed — see StreamResult.SessionID.
	EventSession EventKind = "session"
	// EventStderr is diagnostic output. Surfaced rather than swallowed: for most
	// CLIs it is where the reason for a failure appears.
	EventStderr EventKind = "stderr"
	// EventError reports a failure. It does not necessarily end the stream.
	EventError EventKind = "error"
	// EventDone is the last event of a run, always emitted, success or failure.
	EventDone EventKind = "done"
)

// Event is one thing that happened during a run.
type Event struct {
	Kind EventKind `json:"kind"`
	// Text carries the payload for text, thinking, stderr and error events.
	Text string `json:"text,omitempty"`
	// Tool is the tool name for tool_use and tool_result events.
	Tool string `json:"tool,omitempty"`
	// Detail carries a tool's input or output, already rendered for display.
	Detail string `json:"detail,omitempty"`
	// SessionID is set on session events.
	SessionID string    `json:"session_id,omitempty"`
	At        time.Time `json:"at"`
}

// StreamRequest is one turn of a streamed conversation.
type StreamRequest struct {
	SystemPrompt string
	UserPrompt   string

	// SessionID resumes a conversation the CLI itself is keeping. Empty starts a
	// new one. Only meaningful when the CLI reports SupportsSession.
	SessionID string

	// WorkDir is the directory the CLI runs in, and it is load-bearing rather than
	// cosmetic: an agent CLI discovers its rules, its skills and its MCP servers
	// from the working directory. Left empty the process inherits the caller's,
	// which for a server means "whichever project it was started in" — the wrong
	// project, silently, with a plausible answer.
	WorkDir string

	// AllowTools lets the agent use its tools instead of only describing what it
	// would do. It selects a different preamble; see agenticPreamble for why this
	// is a deliberate choice and not a default.
	AllowTools bool

	// Env adds environment variables to the child process.
	Env map[string]string
}

// StreamResult is what a completed run amounts to.
type StreamResult struct {
	// Text is every text chunk concatenated, so a caller that ignored the events
	// still gets the answer.
	Text string
	// SessionID is the conversation ID to pass back for the next turn. It is the
	// CLI's own ID when the CLI told us one, and otherwise the ID we were given —
	// never invented, because a made-up ID passed to --resume fails or, worse,
	// silently starts a fresh conversation that looks continuous.
	SessionID string
	// Structured reports whether the run came from a CLI's structured mode. False
	// means text-only: the answer is there, tool activity was not observable.
	Structured bool

	// Binary is the CLI that actually ran, and AgentArgsConfigured whether it was
	// given the operator's extra arguments for agentic runs.
	//
	// They are here so a caller can explain a run that produced nothing. Several CLIs
	// need a flag of their own before they will edit a file unprompted, that flag is
	// `ai.agent_args` and it defaults to empty on purpose — it differs per CLI, moves
	// between releases, and grants real authority. Without it the run still costs a
	// full model call and simply writes no file. Reporting "no tools were used" is a
	// symptom; naming the CLI and whether it got its arguments is the cause, and only
	// this package knows which CLI was chosen.
	Binary              string
	AgentArgsConfigured bool
}

// EventFunc receives events as they happen. It is called from the reader
// goroutine, in order, and must not block for long.
type EventFunc func(Event)

// StreamClient is a Client that can report progress while it works.
//
// Callers should type-assert rather than require it, so a future non-CLI client
// without streaming stays usable:
//
//	if sc, ok := client.(ai.StreamClient); ok { ... }
type StreamClient interface {
	Client
	CompleteStream(ctx context.Context, req StreamRequest, emit EventFunc) (*StreamResult, error)
	// SupportsStructuredStream reports whether this CLI surfaces tool activity and
	// its own session ID, as opposed to plain incremental text.
	SupportsStructuredStream() bool
}

// agenticPreamble replaces nonInteractivePreamble when AllowTools is set.
//
// The two exist separately on purpose. nonInteractivePreamble forbids tool use to
// keep an agent that has no business acting from acting — it was chosen over
// --yolo/--dangerously-skip-permissions precisely so that a prompt, not a flag,
// draws the line. That is right for a one-shot question against a real project.
//
// It is wrong for the live search, whose entire premise is an agent working inside
// a throwaway project that was prepared for it: the graphs, the wikis and the MCP
// server are there to be queried, and an agent told not to use tools answers "I
// would query the graph" instead of querying it. So this preamble permits the
// tools and keeps every other constraint — no questions, no TUI, no waiting for a
// human — because the run is still unattended.
const agenticPreamble = `You are running in non-interactive, autonomous mode inside a prepared workspace.
Constraints you MUST follow:
- Do NOT ask the user any questions or request clarification. Nobody is watching this run.
- Do NOT attempt to open a TUI or interactive interface.
- Do NOT wait for approval. If an action needs it, choose a different action.
- DO use your available tools to investigate before answering. The workspace was
  prepared for this: its documentation, code graphs and memory are indexed and
  reachable through them.
- Prefer reading over guessing. An answer you verified and an answer you assumed
  read the same to whoever receives it, which is what makes assuming expensive.
- Finish with your answer as prose. Do not end on a tool call.

`

func preambleFor(allowTools bool) string {
	if allowTools {
		return agenticPreamble
	}
	return nonInteractivePreamble
}

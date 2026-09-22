package sessionhook

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const (
	FormatSessionStart       = "session-start"
	FormatAdditionalContext  = "additional-context"
	FormatFirstInvocation    = "first-invocation"
	FormatPlainContext       = "plain-context"
	FormatBeforeAgent        = "before-agent"
	FormatToolContext        = "tool-context"
	FormatSubagentStart      = "subagent-start"
	FormatCursorSubagentTask = "cursor-subagent-task"
	FormatUserPrompt         = "user-prompt"
	FormatSessionPrompt      = "session-prompt"
	FormatPostToolUse        = "post-tool-use"
	FormatAfterTool          = "after-tool"
	FormatCursorUnit         = "cursor-unit"
	FormatPlainUnit          = "plain-unit"
	FormatPostInvocation     = "post-invocation"
	FormatStop               = "stop"
	FormatCursorStop         = "cursor-stop"
	FormatAfterAgent         = "after-agent"
	FormatAntigravityStop    = "antigravity-stop"
	FormatSessionEnd         = "session-end"
	FormatNoOutput           = "no-output"
)

const SubagentProtocolMarker = "GRAPHIT_SUBAGENT_PROTOCOL_V1"

// Context is assembled at hook execution time from the active project. Skills
// are deliberately absent: hosts discover and load them only when a mandate
// trigger matches.
type Context struct {
	Mandatory       string
	MandatoryLoaded bool
	MemoryDisabled  bool
	TaskDisabled    bool
	Instructions    string
	// BootstrapDelivered reports that this logical host session already received
	// the full protocol. Only the caller can know it, because the decision needs
	// project state the renderer deliberately does not reach for.
	BootstrapDelivered bool
}

// Role is a delegated Graphit specialist. Modules select which mandate rules the
// role carries, so a reader is never handed the whole router; Writes names the
// modules it may mutate, and an empty Writes means read-only.
//
// The same definition serves two readers: the host spawns a subagent with this
// role, or, where the host has no subagent, the main agent reads the role
// document and performs it directly. Every string below is therefore addressed
// to whoever performs the role, never to "the subagent".
type Role struct {
	Name    string
	Summary string
	Modules []string
	Writes  []string
	Output  string
}

var (
	RoleScout = Role{
		Name:    "scout",
		Summary: "Recall and locate: answer questions from Memory, Task, Knowledge, AST and Hub without changing anything.",
		Modules: []string{"memory", "task", "ast", "knowledge", "hub"},
		Output:  "Answer with the conclusion plus identifiers another agent can reopen: record ids and `file:line`. Never paraphrase evidence you did not anchor.",
	}
	RoleTracker = Role{
		Name:    "tracker",
		Summary: "Assess impact: from changed files and the stated requirements, report what else is affected.",
		Modules: []string{"ast", "knowledge", "task"},
		Output:  "Answer with the affected dependents, tests and documentation as `file:line` or page, and name each divergence you found. Do not judge whether the delivery is acceptable: that needs requirement context the coordinator holds.",
	}
	RoleScribe = Role{
		Name:    "scribe",
		Summary: "Transcribe decided content into Task and Memory records with the right structure.",
		Modules: []string{"task", "memory"},
		Writes:  []string{"task", "memory"},
		Output:  "Answer with the ids you wrote. Record what you were given; when it is incomplete, say so instead of inventing the missing part.",
	}
)

func Roles() []Role { return []Role{RoleScout, RoleTracker, RoleScribe} }

// RoleByName resolves a role by its stable name, with or without the branded
// prefix the installed agent files carry. The second result is false for an
// unknown name so callers fall back to the generic worker protocol rather than
// inventing a role.
func RoleByName(name string) (Role, bool) {
	candidate := strings.TrimPrefix(strings.TrimSpace(name), brand.Brand+"-")
	for _, role := range Roles() {
		if strings.EqualFold(candidate, role.Name) {
			return role, true
		}
	}
	return Role{}, false
}

// RoleFileName is the installed document for a role. The mandate points the
// main agent at this name when the host cannot spawn the role as a subagent.
func (r Role) RoleFileName() string { return brand.Brand + "-" + r.Name + ".md" }

func (r Role) readOnly() bool { return len(r.Writes) == 0 }

// Uses reports whether the role carries a module's rules. The generic path
// declares no modules and therefore carries whatever the project enabled.
func (r Role) Uses(module string) bool {
	if len(r.Modules) == 0 {
		return true
	}
	for _, candidate := range r.Modules {
		if candidate == module {
			return true
		}
	}
	return false
}

// CoreInvariant is intentionally small because adapters may reinject it after
// compaction, for subagents, or at another model boundary. Procedures belong in
// the just-in-time skills; this text only preserves routing and precedence.
func CoreInvariant() string {
	return "Graphit invariant: reapply module routing before every new/resumed action. Read matching skills before first use; reuse per target/overrides, reload on their change; reload after compaction if lost. Cluster neighbors remain Graphit-managed: use their returned `dir` as `project_dir` for target MCP reads, not native discovery. Use Graphit MCP before native equivalents; `ai_optimized: true` when supported. If a required tool is unavailable, use default native tools, never the Graphit CLI. With Task enabled, resume durable session/task state; revise changed intent before acting; close delivered sessions explicitly. New questions trigger needed Memory/Task recall; reuse evidence. `project_dir` is call-local; persist identity and relative paths, never checkout roots. Delegation: an applicable instruction that explicitly assigns bounded work to a delegated role authorizes and requires only that role/work (recall→scout, impact→tracker, transcription→scribe); otherwise no subagents. If the host cannot run it, the coordinator performs only that role. Acceptance, checkpoints, Task/session claims, revisions, completion and lifecycle stay with the coordinator. Persistence boundary: project records contain only non-sensitive, project-inherent facts, decisions and evidence. Never persist personal/sensitive data, secrets/credentials, security-risk material or transient feelings/speculation. Durable user-only preferences or corrections belong only in private user memory; otherwise do not persist them."
}

func cursorLifecycleCompensation() string {
	return "Cursor-specific hook compensation: `beforeSubmitPrompt` cannot inject context; Cloud may omit `sessionStart`. Reapply Graphit routing before new/resumed work. Post-tool reminders govern subsequent actions; they cannot enforce the first action."
}

func antigravityLifecycleCompensation() string {
	return "Antigravity-specific hook compensation: `PostToolUse` cannot inject context; apply checkpoints at completed work units without waiting for `PostInvocation`. No subagent-start hook: the role document installed under the agents directory carries the protocol, so pass the assigned session and task ids when delegating and read that document yourself when you cannot delegate."
}

// UnitCompletionReminder is injected after the smallest objective work boundary
// the host exposes. The hook cannot decide whether a semantic unit is complete,
// so it asks the agent to make that judgment immediately instead of at turn end.
func UnitCompletionReminder() string {
	return "Graphit task checkpoint (if enabled): after meaningful work use `" + brand.MCPToolName("task", "progress") + "`; coordinator: `" + brand.MCPToolName("task", "session", "checkpoint") + "` with evidence, problems, decisions, strategy, next step. Reads/bookkeeping alone need none. Revise changed intent; close delivered sessions explicitly. " + DocumentationConsistencyReminder()
}

// DocumentationConsistencyReminder is delivered before the agent can decide
// that a task is complete. Final stop hooks are too late for semantic review,
// so adapters use this through their last context-capable checkpoint boundary.
func DocumentationConsistencyReminder() string {
	return "Verify acceptance checks and code/documentation consistency in both directions; record targets/evidence. Resolve divergence before closing."
}

// SubagentProtocol is self-contained because a delegated performer may start
// with neither the parent's conversation nor its project instructions.
func SubagentProtocol(mandatory ...string) string {
	context := Context{}
	if len(mandatory) > 0 {
		context.Mandatory = mandatory[0]
		context.MandatoryLoaded = true
	}
	return subagentProtocolWithContext(context)
}

func subagentProtocolWithContext(context Context) string {
	return RoleProtocol(Role{}, context)
}

// RoleProtocol renders what a delegated performer works from. It deliberately
// omits every coordination step of the session bootstrap: the performer already
// has the session and task ids, so telling it to open or claim a session would
// only produce a duplicate one.
func RoleProtocol(role Role, context Context) string {
	marker := SubagentProtocolMarker
	if role.Name != "" {
		marker += " role=" + role.Name
	}
	return marker + "\n" + workerProtocol(role, context)
}

func workerProtocol(role Role, context Context) string {
	mandatoryTool := brand.MCPToolName("memory", "mandatory")
	lines := []string{routingContext(context.Instructions)}
	if strings.TrimSpace(context.Instructions) == "" {
		lines = append(lines, "If module routing was not injected, use `"+brand.MCPToolName("mandates")+"` once when available. Read matching installed skills first; fetch a missing skill with `"+brand.MCPToolName("module", "skill")+"`. Project instructions remain authoritative.")
	}
	if !context.MemoryDisabled && role.Uses("memory") {
		switch {
		case !context.MandatoryLoaded:
			lines = append(lines, "Read `"+brand.SkillDirName("memory")+"` and call `"+mandatoryTool+"` once per available scope before acting.")
		case strings.TrimSpace(context.Mandatory) != "":
			lines = append(lines, "Standing context already read from the authoritative memory table; do not call `"+mandatoryTool+"` again:\n"+strings.TrimSpace(context.Mandatory))
		}
		lines = append(lines, "When a question about the system, rationale or learned behavior exceeds what you were given, read `"+brand.SkillDirName("memory")+"` and query `"+brand.MCPToolName("memory", "search")+"` with `exclude_mandatory: true`, `top_k: 5`, `ai_optimized: true`, then read the selected ids with `"+brand.MCPToolName("memory", "source")+"`.")
	}
	if !context.TaskDisabled && role.Uses("task") {
		lines = append(lines, "Read the ids the coordinator gave you with `"+brand.MCPToolName("task", "get")+"`. Search `"+brand.MCPToolName("task", "search")+"` with `top_k: 5`, `ai_optimized: true` only while a relevant gap remains.")
	}
	lines = append(lines, delegatedContracts(role))
	return strings.Join(lines, "\n")
}

// delegatedContracts separate a performer from a coordinator. They hold for
// every role and for the generic path, because the failures they prevent — a
// performer opening its own session, or closing work the coordinator still
// owns — do not depend on which role is running.
//
// Turn completion and resumption are host capabilities, separate from Graphit
// ownership. Reporting must not require an active waiting loop or imply that
// another agent's durable work is complete.
func delegatedContracts(role Role) string {
	lines := []string{
		"Delegated work contract:",
		"- The coordinator owns the session. It hands you the `session_id` and the task ids you need. Claim at most your own task, and never create, claim or close a coordination session.",
		"- Finishing a turn does not complete Graphit work. Once the requested work is done, report it: do not complete or cancel a task, do not close a session, and do not release anything you were not asked to release. Leave your findings recorded so the next instruction continues from them. The coordinator decides when this work ends.",
		"- Deliver the complete answer with findings anchored to an id or `file:line`, then finish your turn. Do not run a waiting loop. The coordinator may later send you another instruction through the host's follow-up/resume mechanism when supported; that continuation uses your recorded evidence. Reuse is optional: no keep-alive, reuse or explicit dismissal is required.",
		"- Supporting delegates have the same optional follow-up capability. Fold their findings into your own answer. Host resource handling follows the host's contract and never authorizes completing or releasing Graphit records owned by another agent.",
		"- Exception: where the host cannot run agents separately, whoever performs this role is the coordinator itself. Apply the role scope and evidence requirements directly; separate-agent coordination instructions do not apply.",
	}
	if role.Name == "" {
		return strings.Join(lines, "\n")
	}
	lines = append(lines, "- You are performing the "+role.Name+" role. "+role.Summary, "- "+role.Output)
	if role.readOnly() {
		lines = append(lines, "- This role never writes. Read, conclude and report; when the answer requires a change, say so and leave the change to the coordinator.")
	} else {
		lines = append(lines, "- This role writes only to "+strings.Join(role.Writes, " and ")+", and only content the coordinator already decided. It never edits code.")
	}
	return strings.Join(lines, "\n")
}

// Protocol is the context injected before the first model response. When
// mandatory is supplied, even as an empty string, the hook has already read the
// authoritative memory table; otherwise the agent receives the MCP fallback.
func Protocol(mandatory ...string) string {
	context := Context{}
	if len(mandatory) > 0 {
		context.Mandatory = mandatory[0]
		context.MandatoryLoaded = true
	}
	return protocolWithContext(context)
}

func protocolWithContext(context Context) string {
	mandatoryTool := brand.MCPToolName("memory", "mandatory")
	search := brand.MCPToolName("memory", "search")
	memorySource := brand.MCPToolName("memory", "source")
	taskSearch := brand.MCPToolName("task", "search")
	taskGet := brand.MCPToolName("task", "get")

	lines := []string{routingContext(context.Instructions)}
	if strings.TrimSpace(context.Instructions) == "" {
		lines = append(lines, "If module routing was not injected, use `"+brand.MCPToolName("mandates")+"` once when available. Read matching installed skills first; fetch a missing skill with `"+brand.MCPToolName("module", "skill")+"`. Project instructions remain authoritative.")
	}
	if context.MemoryDisabled && context.TaskDisabled {
		return strings.Join(lines, "\n")
	}
	lines = append(lines, "Graphit session bootstrap:")
	step := 1
	appendStep := func(text string) {
		lines = append(lines, fmt.Sprintf("%d. %s", step, text))
		step++
	}
	if !context.MemoryDisabled {
		if !context.MandatoryLoaded {
			appendStep("If available, read `" + brand.SkillDirName("memory") + "`; call `" + mandatoryTool + "` with `ai_optimized: true` once per missing available scope (`project` for a resolved project, and `user`). Consume all standing context before acting.")
		} else if strings.TrimSpace(context.Mandatory) == "" {
			appendStep("The hook read the authoritative memory table; it contains no mandatory memories.")
		} else {
			appendStep("The hook read the authoritative memory table. Treat the following as standing context; do not call `" + mandatoryTool + "` again:\n" + strings.TrimSpace(context.Mandatory))
		}
		appendStep("Whenever a question about the system, rationale or learned behavior needs context, including during work, read `" + brand.SkillDirName("memory") + "`; query `" + search + "` with `exclude_mandatory: true`, `top_k: 5`, `ai_optimized: true`, and the decision topic. Read selected ids with `" + memorySource + "`. Reuse sufficient context; new questions can require recall in the same session and scope.")
	}
	if !context.TaskDisabled {
		appendStep("Read `" + brand.SkillDirName("task") + "` before session work. Use `" + brand.MCPToolName("task", "session", "list") + "` for active sessions and `" + brand.MCPToolName("task", "session", "search") + "` for relevant history; `" + brand.MCPToolName("task", "session", "get") + "` reads the chosen description, strategy, checkpoints and linked task IDs. Then decide between continuing one and opening a new one: an open session whose demand this request continues is the one to resume, even across interruption or compaction; changed scope revises it rather than starting a second; only a different demand justifies a new session. With no match, `" + brand.MCPToolName("task", "session", "create") + "` records the detailed user request, scope, constraints and strategy, then claim coordination. Link all agent-created tasks to that durable session_id; native host session IDs are not Graphit session IDs. Delegated workers receive session_id and task IDs, read them, claim only their task and never take or close the coordinator's session.")
		appendStep("At meaningful progress, problems or decisions the coordinator calls `" + brand.MCPToolName("task", "session", "checkpoint") + "` with evidence, rationale, strategy and exact next step. For added requests or changed direction, use `" + brand.MCPToolName("task", "session", "revise") + "` before affected work and reconcile its tasks. Before reporting delivery, explicitly `" + brand.MCPToolName("task", "session", "complete") + "` with a final summary after linked tasks are terminal; explain cancelled scope. Interrupted work needs a descriptive checkpoint and explicit release, not completion. Stop hooks never close sessions; absent or uncorrelated host identity cannot release ownership safely, so do not rely on them for handoff.")
		appendStep("Before project work, read `" + brand.SkillDirName("task") + "`; search `" + taskSearch + "` with `top_k: 5`, `ai_optimized: true`, focused on this request, or get an assigned id directly with `" + taskGet + "`. Read the chosen task, parent specification and relevant dependencies/precedents. During work, new doubts also trigger focused search of prior investigations, decisions and evidence; do not wait for restart or a new plan. Follow `next_cursor` only while a relevant gap remains. Reuse recalled context.")
		if strings.TrimSpace(context.Instructions) == "" {
			appendStep("For multi-step work, persist specification, acceptance criteria, plan and dependency-ordered tasks before execution. Resume from recorded progress/evidence; revise affected tasks when scope changes. A single umbrella task is not an executable project plan.")
		}
	}
	return strings.Join(lines, "\n")
}

func routingContext(instructions string) string {
	instructions = strings.TrimSpace(instructions)
	if instructions == "" {
		return CoreInvariant()
	}
	return instructions
}

// Render returns a native stdout payload for the output format selected by an
// adapter. Adapter names and lifecycle paths remain owned by the adapters.
func Render(format string, input []byte) ([]byte, error) {
	return RenderWithMandatory(format, input)
}

// RenderWithMandatory renders a native hook payload. Supplying mandatory marks
// phase one as completed by the hook and injects its content directly.
func RenderWithMandatory(format string, input []byte, mandatory ...string) ([]byte, error) {
	context := Context{}
	if len(mandatory) > 0 {
		context.Mandatory = mandatory[0]
		context.MandatoryLoaded = true
	}
	return RenderWithContext(format, input, context)
}

// RenderWithContext renders a native hook payload with current project
// instructions. Full bootstrap formats include memory and current project
// mandates. Repeated model boundaries receive only compact invariant/reminder
// text so long-lived sessions do not accumulate the startup context.
func RenderWithContext(format string, input []byte, context Context) ([]byte, error) {
	reminder := UnitCompletionReminder()
	if context.TaskDisabled {
		reminder = ""
	}
	switch strings.ToLower(format) {
	case FormatSessionStart:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "SessionStart",
				"additionalContext": protocolWithContext(context),
			},
		})
	case FormatAdditionalContext:
		return json.Marshal(map[string]any{"additional_context": protocolWithContext(context) + "\n" + cursorLifecycleCompensation()})
	case FormatPlainContext:
		return []byte(protocolWithContext(context)), nil
	// FormatSessionPrompt serves a host whose session-start output never reaches
	// the model, so the turn boundary has to carry the bootstrap. Repeating it
	// every turn would cost the whole protocol per turn, so only the first prompt
	// of a session pays it and the rest receive the compact invariant.
	case FormatSessionPrompt:
		if context.BootstrapDelivered {
			return []byte(CoreInvariant()), nil
		}
		return []byte(protocolWithContext(context)), nil
	case FormatBeforeAgent:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "BeforeAgent",
				"additionalContext": CoreInvariant(),
			},
		})
	case FormatSubagentStart:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "SubagentStart",
				"additionalContext": subagentProtocolWithContext(context),
			},
		})
	case FormatCursorSubagentTask:
		return renderCursorSubagentTask(input, context)
	case FormatUserPrompt:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "UserPromptSubmit",
				"additionalContext": CoreInvariant(),
			},
		})
	case FormatToolContext:
		return json.Marshal(map[string]any{"additional_context": routingContext(context.Instructions)})
	case FormatPostToolUse:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "PostToolUse",
				"additionalContext": reminder,
			},
		})
	case FormatAfterTool:
		return json.Marshal(map[string]any{
			"hookSpecificOutput": map[string]any{
				"hookEventName":     "AfterTool",
				"additionalContext": reminder,
			},
		})
	case FormatCursorUnit:
		return json.Marshal(map[string]any{"additional_context": strings.TrimSpace(reminder + " Reapply Graphit routing before the next action.")})
	case FormatPlainUnit:
		return []byte(reminder), nil
	case FormatPostInvocation:
		return json.Marshal(map[string]any{
			"injectSteps": []any{map[string]any{"ephemeralMessage": reminder}},
		})
	case FormatStop, FormatCursorStop, FormatAfterAgent, FormatSessionEnd:
		return []byte(`{}`), nil
	case FormatAntigravityStop:
		return json.Marshal(map[string]any{"decision": "stop"})
	case FormatNoOutput:
		return nil, nil
	case FormatFirstInvocation:
		var event struct {
			InvocationNum *int `json:"invocationNum"`
		}
		if err := json.Unmarshal(input, &event); err != nil {
			return nil, fmt.Errorf("decoding first-invocation hook input: %w", err)
		}
		if event.InvocationNum == nil {
			return nil, fmt.Errorf("decoding first-invocation hook input: invocationNum is missing")
		}
		var injected string
		if *event.InvocationNum == 0 {
			injected = protocolWithContext(context) + "\n" + antigravityLifecycleCompensation()
		} else {
			injected = CoreInvariant() + "\n" + antigravityLifecycleCompensation()
		}
		return json.Marshal(map[string]any{
			"injectSteps": []any{map[string]any{"ephemeralMessage": injected}},
		})
	default:
		return nil, fmt.Errorf("unsupported session hook format %q", format)
	}
}

func renderCursorSubagentTask(input []byte, context Context) ([]byte, error) {
	var event map[string]any
	if err := json.Unmarshal(input, &event); err != nil {
		return nil, fmt.Errorf("decoding Cursor Task hook input: %w", err)
	}
	toolInput, ok := event["tool_input"].(map[string]any)
	if !ok {
		return nil, fmt.Errorf("decoding Cursor Task hook input: tool_input is missing")
	}
	role, _ := RoleByName(delegatedRoleName(toolInput))
	for _, field := range []string{"prompt", "task", "description"} {
		value, ok := toolInput[field].(string)
		if !ok || strings.TrimSpace(value) == "" {
			continue
		}
		// The protocol now varies per role, so the prefix check has to compare the
		// protocol of THIS role. A prompt already carrying another role's protocol
		// is a different delegation and still needs its own.
		protocol := RoleProtocol(role, context) + "\n\nTask:\n"
		if strings.HasPrefix(value, protocol) {
			return json.Marshal(map[string]any{"permission": "allow"})
		}
		toolInput[field] = protocol + value
		return json.Marshal(map[string]any{"permission": "allow", "updated_input": toolInput})
	}
	return json.Marshal(map[string]any{"permission": "allow"})
}

// delegatedRoleName reads whichever field the host used to name the target
// agent. An unknown or absent name resolves to the generic worker protocol
// rather than to a guessed role.
func delegatedRoleName(toolInput map[string]any) string {
	for _, field := range []string{"subagent_type", "subagentType", "agent", "agent_type", "agentType", "subagent"} {
		if value, ok := toolInput[field].(string); ok && strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}

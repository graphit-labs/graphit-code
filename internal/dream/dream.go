package dream

import (
	"context"
	crand "crypto/rand"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

const (
	checkInterval = 10 * time.Minute

	defaultIdleTimeout = 2 * time.Hour

	defaultMaxDuration = 8 * time.Hour

	stateFile = "dream.state"
)

type DreamConfig struct {
	Enabled bool

	IdleTimeout time.Duration

	MaxDuration time.Duration
}

type dreamState struct {
	CurrentSessionID string `json:"current_session_id,omitempty"`

	// LastUserModTime is the newest file mtime observed by the most recent tick.
	// It is telemetry: it changes on every tick whether or not anything happened.
	LastUserModTime time.Time `json:"last_user_mod_time"`

	// SessionModWatermark is the mtime that opened the current session, and it is
	// the only thing "has the developer done something since?" can be compared
	// against. These were one field, and tick overwrote it before
	// resolveSessionID read it — so the comparison was always mtime against
	// itself, the session never rotated, and Exhausted (reset only on rotation)
	// became permanent after the first deep sleep.
	SessionModWatermark time.Time `json:"session_mod_watermark,omitempty"`

	Exhausted bool `json:"exhausted,omitempty"`

	Dreaming bool `json:"dreaming,omitempty"`

	DreamStartedAt time.Time `json:"dream_started_at,omitempty"`

	SleepingSince time.Time `json:"sleeping_since,omitempty"`

	LastDreamAt time.Time `json:"last_dream_at,omitempty"`
}

type Runner struct {
	projectDir string
	ide        string
	projectCfg func() config.ConfigMap

	mu       sync.Mutex
	running  bool
	cancelFn context.CancelFunc
	logFn    func(string, ...any)

	state dreamState
}

func NewRunner(projectDir, ide string, projectCfgLoader func() config.ConfigMap) *Runner {
	r := &Runner{
		projectDir: projectDir,
		ide:        ide,
		projectCfg: projectCfgLoader,
	}

	r.loadState()

	r.mu.Lock()
	if !r.state.Dreaming && r.state.SleepingSince.IsZero() {
		r.state.SleepingSince = time.Now()
		r.saveStateLocked()
	}
	r.mu.Unlock()

	return r
}

func (r *Runner) log(format string, args ...any) {
	if r.logFn != nil {
		r.logFn(format, args...)
	}
}

func (r *Runner) IsRunning() bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.running
}

func (r *Runner) Run(ctx context.Context) error {

	r.tick(ctx)

	ticker := time.NewTicker(checkInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:
			r.tick(ctx)
		}
	}
}

func (r *Runner) tick(ctx context.Context) {
	if ctx.Err() != nil {
		return
	}

	cfg := r.resolveConfig()

	if !cfg.Enabled {
		return
	}

	if r.IsRunning() {
		return
	}

	lastMod, err := LastModifiedTime(r.projectDir)
	if err != nil {
		r.log("dream: failed to check idle time: %v", err)
		return
	}

	r.mu.Lock()
	r.state.LastUserModTime = lastMod
	r.saveStateLocked()
	r.mu.Unlock()

	idleDuration := time.Since(lastMod)
	if idleDuration < cfg.IdleTimeout {
		return
	}

	sessionID := r.resolveSessionID(lastMod)

	r.mu.Lock()
	exhausted := r.state.Exhausted
	r.mu.Unlock()
	if exhausted {
		return
	}

	r.log("dream: all preconditions met (idle=%s, session=%s), starting dream",
		idleDuration.Truncate(time.Second), sessionID)

	r.mu.Lock()
	r.running = true
	r.state.Dreaming = true
	r.state.DreamStartedAt = time.Now()
	r.state.SleepingSince = time.Time{}
	r.saveStateLocked()
	r.mu.Unlock()

	go func() {
		defer func() {
			r.mu.Lock()
			r.running = false
			r.state.Dreaming = false
			r.state.LastDreamAt = time.Now()
			r.state.DreamStartedAt = time.Time{}
			r.state.SleepingSince = time.Now()
			r.saveStateLocked()
			r.cancelFn = nil
			r.mu.Unlock()
		}()

		var dreamCtx context.Context
		var cancel context.CancelFunc

		if cfg.MaxDuration > 0 {
			dreamCtx, cancel = context.WithTimeout(ctx, cfg.MaxDuration)
		} else {
			dreamCtx, cancel = context.WithCancel(ctx)
		}
		defer cancel()

		r.mu.Lock()
		r.cancelFn = cancel
		r.mu.Unlock()

		if err := r.executeDream(dreamCtx, sessionID); err != nil {
			r.log("dream: session failed: %v", err)
		} else {
			r.log("dream: session completed successfully")
		}

		r.checkDeepSleep(sessionID)
	}()
}

func (r *Runner) resolveSessionID(currentModTime time.Time) string {
	r.mu.Lock()
	defer r.mu.Unlock()

	needsNew := r.state.CurrentSessionID == "" ||
		currentModTime.After(r.state.SessionModWatermark)

	if needsNew {
		wasExhausted := r.state.Exhausted
		r.state.CurrentSessionID = generateDreamID()
		r.state.SessionModWatermark = currentModTime
		// New activity ends deep sleep. This is the only place Exhausted clears,
		// which is why the watermark has to be a field tick does not clobber:
		// otherwise the module never wakes up again.
		r.state.Exhausted = false
		r.saveStateLocked()
		if wasExhausted {
			r.log("dream: waking from deep sleep — new session %s (user activity since the exhausted cycle)", r.state.CurrentSessionID)
		} else {
			r.log("dream: new session %s (user activity detected since last dream)", r.state.CurrentSessionID)
		}
	} else {
		r.log("dream: resuming session %s (no user changes since last dream)", r.state.CurrentSessionID)
	}

	return r.state.CurrentSessionID
}

const exhaustedSentinel = ".exhausted"

func (r *Runner) checkDeepSleep(sessionID string) {
	sentinelPath := filepath.Join(ReportsDir(r.projectDir), sessionID+exhaustedSentinel)
	if !fileExists(sentinelPath) {
		return
	}

	r.log("dream: deep sleep signal detected for session %s — no more skills or patterns to extract", sessionID)

	r.mu.Lock()
	r.state.Exhausted = true
	r.saveStateLocked()
	r.mu.Unlock()
}

func DeepSleepSentinelName() string {
	return exhaustedSentinel
}

func ResolveDreamConfig(projectCfg config.ConfigMap) DreamConfig {
	cfg := DreamConfig{
		IdleTimeout: defaultIdleTimeout,
		MaxDuration: defaultMaxDuration,
	}

	cfg.Enabled = !config.IsModuleDisabled("dream", nil, projectCfg)

	if val := config.ResolveConfig("dream.idle_timeout", nil, projectCfg); val != "" {
		if secs, err := strconv.Atoi(val); err == nil && secs > 0 {
			cfg.IdleTimeout = time.Duration(secs) * time.Second
		}
	}

	if val := config.ResolveConfig("dream.max_duration", nil, projectCfg); val != "" {
		if secs, err := strconv.Atoi(val); err == nil && secs >= 0 {
			cfg.MaxDuration = time.Duration(secs) * time.Second
		}
	}

	return cfg
}

func (r *Runner) resolveConfig() DreamConfig {
	var projectCfg config.ConfigMap
	if r.projectCfg != nil {
		projectCfg = r.projectCfg()
	}
	return ResolveDreamConfig(projectCfg)
}

// runMemoryConsolidation is the guaranteed half of a dream session: it sanitises
// the memory store deterministically, in Go, before the agent runs.
//
// It is deliberately not delegated to the agent. Whether the agent can write
// files at all depends on which CLI is installed and how it is configured, and
// "the memories were consolidated" must not be contingent on that. The model is
// used for judgement — which memories duplicate or contradict which — and every
// mutation is applied and constrained here.
func (r *Runner) runMemoryConsolidation(ctx context.Context) []*memory.ConsolidationOutcome {
	var projectCfg config.ConfigMap
	if r.projectCfg != nil {
		projectCfg = r.projectCfg()
	}
	if config.IsModuleDisabled("memory", nil, projectCfg) {
		return nil
	}

	aiClient, err := ai.NewClientFromConfig()
	if err != nil || aiClient == nil {
		r.log("dream: memory consolidation skipped — no AI client available")
		return nil
	}

	var outcomes []*memory.ConsolidationOutcome

	for _, scope := range []string{"project", "user"} {
		// The gate is the scope's IDENTITY, not a directory: an empty URI means the scope has no id
		// to resolve yet, which is the only case where there is nothing to consolidate.
		if memory.TableURIForScope(scope) == "" {
			continue
		}

		r.log("dream: running memory consolidation for %s scope", scope)

		report, err := memory.RunConsolidation(ctx, scope, aiClient)
		if err != nil {
			r.log("dream: memory consolidation (%s) error: %v", scope, err)
			continue
		}
		if report == nil {
			continue
		}
		if report.AIFailed {
			r.log("dream: memory consolidation (%s) — analysis failed, only deterministic checks ran", scope)
		}
		if !report.HasActions() {
			r.log("dream: memory consolidation (%s) — nothing to do (%d memories analysed)", scope, report.TotalMemories)
			continue
		}

		svc, err := memory.NewMemoryAppService(r.projectDir).NewMemorySvc(scope == "user")
		if err != nil {
			r.log("dream: memory consolidation (%s) skipped — %v", scope, err)
			continue
		}

		outcome, err := memory.ApplyConsolidation(ctx, scope, report, svc)
		_ = svc.Close()
		if err != nil {
			r.log("dream: memory consolidation (%s) apply error: %v", scope, err)
			continue
		}

		// Counting what was APPLIED, not what was proposed. The previous message
		// reported the size of the plan and called it "applied", which read as
		// success for a run that changed nothing.
		r.log("dream: memory consolidation (%s) — %d applied, %d refused, %d failed (of %d proposed)",
			scope, len(outcome.Applied), len(outcome.Skipped), len(outcome.Failed), report.TotalActions())
		outcomes = append(outcomes, outcome)
	}

	return outcomes
}

func (r *Runner) executeDream(ctx context.Context, sessionID string) error {
	dreamArtifactDir := ReportsDir(r.projectDir)

	r.log("dream: session %s starting", sessionID)

	if err := os.MkdirAll(dreamArtifactDir, 0o755); err != nil {
		return fmt.Errorf("creating dream artifact dir: %w", err)
	}

	// Memory sanitisation runs first and unconditionally, so a session that fails
	// later still leaves the store consolidated.
	outcomes := r.runMemoryConsolidation(ctx)

	artifactPath := filepath.Join(dreamArtifactDir, sessionID+reportExt)

	// Recorded before the agent runs: if the agent writes the report itself, that
	// file is the deliverable and must not be overwritten by a wrapper around its
	// stdout. Both instructions used to be live at once, and the runner always won.
	agentReportBefore := reportFingerprint(artifactPath)

	r.log("dream: executing AI agent locally for %s", r.projectDir)
	prompt := buildDreamPrompt(r.projectDir, sessionID, r.ide, outcomes)
	result, err := r.executeLocal(ctx, prompt, sessionID)
	if err != nil {
		// A failed agent does not discard the consolidation that already happened.
		if writeErr := r.writeConsolidationOnlyReport(artifactPath, sessionID, outcomes, err); writeErr != nil {
			r.log("dream: could not record the failed session: %v", writeErr)
		}
		return fmt.Errorf("executing dream agent: %w", err)
	}

	if fp := reportFingerprint(artifactPath); fp != "" && fp != agentReportBefore {
		// The agent wrote its own report. Keep it and append the audit trail of the
		// deterministic half, which the agent has no way to know.
		if err := appendConsolidationAudit(artifactPath, outcomes); err != nil {
			r.log("dream: could not append the consolidation audit: %v", err)
		}
		r.log("dream: session %s completed — agent-written report at %s", sessionID, artifactPath)
		return nil
	}

	body := result + "\n" + consolidationAudit(outcomes)
	if err := os.WriteFile(artifactPath, []byte(body), 0o644); err != nil {
		return fmt.Errorf("writing dream artifact: %w", err)
	}

	r.log("dream: session %s completed — report at %s", sessionID, artifactPath)
	return nil
}

// reportFingerprint identifies the report file's current content cheaply, so the
// runner can tell "the agent wrote this" from "this is what I wrote last cycle".
func reportFingerprint(path string) string {
	info, err := os.Stat(path)
	if err != nil {
		return ""
	}
	return fmt.Sprintf("%d:%d", info.Size(), info.ModTime().UnixNano())
}

func consolidationAudit(outcomes []*memory.ConsolidationOutcome) string {
	if len(outcomes) == 0 {
		return ""
	}
	var b strings.Builder
	b.WriteString("\n## Memory Consolidation (applied by the runner)\n\n")
	b.WriteString("These changes were applied deterministically before the agent ran.\n\n")
	for _, o := range outcomes {
		if o == nil {
			continue
		}
		b.WriteString(o.Markdown())
	}
	return b.String()
}

func appendConsolidationAudit(path string, outcomes []*memory.ConsolidationOutcome) error {
	audit := consolidationAudit(outcomes)
	if audit == "" {
		return nil
	}
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer func() { _ = f.Close() }()
	_, err = f.WriteString(audit)
	return err
}

// writeConsolidationOnlyReport records a session whose agent failed. The
// consolidation still happened, and a developer needs to see it — silence here
// would hide real changes to the memory store behind an agent error.
func (r *Runner) writeConsolidationOnlyReport(path, sessionID string, outcomes []*memory.ConsolidationOutcome, agentErr error) error {
	audit := consolidationAudit(outcomes)
	if audit == "" {
		return nil
	}
	var b strings.Builder
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "title: Dream Session %s\n", sessionID)
	_, _ = fmt.Fprintf(&b, "created: %s\n", time.Now().UTC().Format(time.RFC3339))
	b.WriteString("type: dream-report\n")
	b.WriteString("status: agent-failed\n")
	b.WriteString("---\n\n")
	b.WriteString("# Dream Report\n\n")
	_, _ = fmt.Fprintf(&b, "The agent step failed: %v\n\n", agentErr)
	b.WriteString("The memory consolidation below had already been applied and is unaffected.\n")
	b.WriteString(audit)
	return os.WriteFile(path, []byte(b.String()), 0o644)
}

func (r *Runner) executeLocal(ctx context.Context, prompt, sessionID string) (string, error) {
	dreamArtifactDir := ReportsDir(r.projectDir)

	agentInstruction := fmt.Sprintf(`%s

IMPORTANT INSTRUCTIONS:
- You are working DIRECTLY in the project directory — there is no git worktree or branch
- This may be a progressive session — check past dream reports for continuity
- Your mission is to generate skills, evaluate existing skills, create integration patterns, and generate memories
- Skills and memories are written in-place to the directories determined by installed adapters
- Write the dream report to %s/%s%s. If you cannot write files, return it as your answer instead and the runner will save it.
- Do NOT make code changes — only generate/improve skills, rules, commands, and memories
- Use IDE context: %s
`, prompt, dreamArtifactDir, sessionID, reportExt, r.ide)

	client, err := ai.NewClientFromConfig()
	if err != nil {
		return "", fmt.Errorf("creating AI client: %w", err)
	}

	// The agentic path, not Complete. Complete carries a preamble that forbids
	// file edits and shell commands — correct for an analytical call, fatal here,
	// where every deliverable is a file. It also runs in the caller's working
	// directory, which for the daemon is not the project: an agent CLI discovers
	// its rules, skills and MCP servers from the working directory, so without
	// WorkDir the session would run with the wrong project's configuration.
	streamer, ok := client.(ai.StreamClient)
	if !ok {
		r.log("dream: WARNING — the configured AI client cannot run in agentic mode; " +
			"the agent will be unable to create artifacts and only the memory consolidation will have any effect")
		out, completeErr := client.Complete(ctx, "", agentInstruction)
		if completeErr != nil {
			return "", fmt.Errorf("agent execution failed: %w", completeErr)
		}
		return buildDreamArtifact(sessionID, out, nonAgenticDiagnostic()), nil
	}

	var tools []string
	seenTool := map[string]bool{}
	result, err := streamer.CompleteStream(ctx, ai.StreamRequest{
		UserPrompt: agentInstruction,
		WorkDir:    r.projectDir,
		AllowTools: true,
	}, func(ev ai.Event) {
		if ev.Kind == ai.EventToolUse && ev.Tool != "" && !seenTool[ev.Tool] {
			seenTool[ev.Tool] = true
			tools = append(tools, ev.Tool)
		}
	})
	if err != nil {
		return "", fmt.Errorf("agent execution failed: %w", err)
	}

	diagnostic := ""
	if len(tools) > 0 {
		r.log("dream: agent used %d distinct tool(s): %s", len(tools), strings.Join(tools, ", "))
	} else if result.Structured {
		// Only meaningful when the CLI reports tool activity at all. Without a
		// structured mode, "no tools observed" says nothing about what happened.
		diagnostic = toollessRunDiagnostic(result)
		r.log("dream: WARNING — the agent completed without using any tool, so it " +
			"almost certainly produced prose instead of artifacts")
	}

	return buildDreamArtifact(sessionID, result.Text, diagnostic), nil
}

func StatePath(projectDir string) string {
	return brand.ProjectRuntimePath(projectDir, "daemon", stateFile)
}

func (r *Runner) statePath() string {
	return StatePath(r.projectDir)
}

func LoadStateFromDir(projectDir string) (currentSessionID string, lastUserMod, lastDreamAt, dreamStartedAt, sleepingSince time.Time, exhausted, dreaming bool) {
	data, err := os.ReadFile(StatePath(projectDir))
	if err != nil {
		return
	}
	var s dreamState
	if err := json.Unmarshal(data, &s); err != nil {
		return
	}
	return s.CurrentSessionID, s.LastUserModTime, s.LastDreamAt, s.DreamStartedAt, s.SleepingSince, s.Exhausted, s.Dreaming
}

func (r *Runner) loadState() {
	data, err := os.ReadFile(r.statePath())
	if err != nil {
		return
	}
	_ = json.Unmarshal(data, &r.state)
}

func (r *Runner) saveStateLocked() {
	dir := filepath.Dir(r.statePath())
	_ = os.MkdirAll(dir, 0o755)

	data, err := json.MarshalIndent(r.state, "", "  ")
	if err != nil {
		return
	}
	_ = os.WriteFile(r.statePath(), data, 0o644)
}

// generateDreamID returns a session identifier of the form 20060102T150405-abcd:
// a sortable timestamp plus two random bytes.
//
// The format is deliberate. These ids become report filenames, so sorting by name
// sorts by time, which a ULID would also give — but a ULID is not readable in a
// directory listing and this is.
//
// Everything here used to call it a ULID anyway: the state field, its JSON tag, the
// parameters, the tests and the specification. That is now fixed, including the
// on-disk `current_session_id` tag, which means a dream.state written by an older
// build loses its session id and the next tick opens a new session. That is the
// correct trade while the format is still moving.
func generateDreamID() string {
	now := time.Now().UTC()
	ts := now.Format("20060102T150405")
	b := make([]byte, 2)
	_, _ = crand.Read(b)
	suffix := fmt.Sprintf("%04x", b)
	return ts + "-" + suffix
}

func LastModifiedTime(projectDir string) (time.Time, error) {
	var latest time.Time
	brandDir := brand.DotDir()

	ic := ignorer.New(projectDir, projectDir, "", nil)

	err := filepath.Walk(projectDir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil
		}

		name := info.Name()
		if info.IsDir() {

			if name == ".git" || name == brandDir {
				return filepath.SkipDir
			}

			if rel, relErr := filepath.Rel(projectDir, path); relErr == nil && rel != "." {
				if ic.IsIgnored(rel, true) && !ic.ShouldDescend(rel) {
					return filepath.SkipDir
				}
			}
			return nil
		}

		if rel, relErr := filepath.Rel(projectDir, path); relErr == nil {
			if ic.IsIgnored(rel, false) {
				return nil
			}
		}

		if info.ModTime().After(latest) {
			latest = info.ModTime()
		}
		return nil
	})

	if err != nil {
		return time.Time{}, err
	}
	if latest.IsZero() {
		return time.Time{}, fmt.Errorf("no files found in %s", projectDir)
	}
	return latest, nil
}

func fileExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}

// toollessRunDiagnostic explains a run that used no tool, for the REPORT.
//
// The warning used to exist only in the daemon log, which is the one place the person
// who wanted the artifact does not look. Meanwhile the run had already spent a full
// model call and written nothing, and nothing on the way out said why — so the failure
// repeated every cycle, silently, and looked like the agent simply had nothing to say.
//
// The likely cause is nameable, which is what makes this worth writing down: several
// CLIs need an explicit flag before they will edit files unprompted, that flag is
// `ai.agent_args`, and it is empty by default on purpose — it differs per CLI, moves
// between releases, and grants real authority, so guessing it either fails to parse or
// hands the agent more than was intended.
//
// It stays a hypothesis and is worded as one. A CLI can be configured correctly and
// still decide a session needs no tools, and asserting the cause would send someone to
// fix a setting that was never wrong.
func toollessRunDiagnostic(result *ai.StreamResult) string {
	var b strings.Builder
	b.WriteString("The agent finished without using a single tool, so this session " +
		"almost certainly produced prose instead of artifacts. The model call was spent either way.\n\n")

	if result.AgentArgsConfigured {
		fmt.Fprintf(&b, "`ai.agent_args` IS configured for `%s`, so the usual cause is ruled out. "+
			"Either the CLI needs a different flag than the one configured, or it genuinely "+
			"decided this session needed no tools.\n", result.Binary)
		return b.String()
	}

	fmt.Fprintf(&b, "`ai.agent_args` is NOT set for `%s`. Most agent CLIs refuse to edit files "+
		"without an explicit flag, and that flag is what this setting carries. It ships empty "+
		"deliberately: it differs per CLI, changes between releases, and grants real authority, "+
		"so it is the operator's to choose rather than something to guess.\n\n", result.Binary)
	fmt.Fprintf(&b, "    %s config ai.agent_args.%s \"<the flag your CLI needs>\"\n\n",
		brand.BinName(), result.Binary)
	b.WriteString("Check that CLI's own documentation for the flag that allows unattended edits.\n")
	return b.String()
}

// nonAgenticDiagnostic is the report's version of "this client cannot create anything".
//
// A client with no agentic mode cannot edit a file at all, so the session was prose
// before it started. That is a harder fact than the toolless case and deserves saying
// in the report rather than only in the daemon log, for the same reason: the person
// waiting for artifacts reads the report.
func nonAgenticDiagnostic() string {
	return "The configured AI client has no agentic mode, so it cannot create or edit " +
		"files at all. Nothing below is an artifact, and only the memory consolidation " +
		"in this session had any effect.\n\n" +
		"Configure a CLI that supports tool use (`ai.cli`) if artifacts are the point.\n"
}

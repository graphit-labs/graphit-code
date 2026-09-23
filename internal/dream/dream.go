package dream

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
	"github.com/oklog/ulid/v2"
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
	agent      string
	projectCfg func() config.ConfigMap
	newClient  func() (ai.Client, error)
	newLedger  func(context.Context, string) (runLedgerWriter, error)

	mu       sync.Mutex
	running  bool
	cancelFn context.CancelFunc
	logFn    func(string, ...any)

	state dreamState
}

type runLedgerWriter interface {
	Put(context.Context, RunRecord) error
	Close() error
}

func NewRunner(projectDir, agent string, projectCfgLoader func() config.ConfigMap) *Runner {
	r := &Runner{
		projectDir: projectDir,
		agent:      agent,
		projectCfg: projectCfgLoader,
		newClient:  ai.NewClientFromConfig,
		newLedger: func(ctx context.Context, projectDir string) (runLedgerWriter, error) {
			return openRunLedger(ctx, projectDir)
		},
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
			r.checkDeepSleep(sessionID)
		}
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

func (r *Runner) checkDeepSleep(sessionID string) {
	r.log("dream: session %s exhausted after its Memory consolidation run", sessionID)

	r.mu.Lock()
	r.state.Exhausted = true
	r.saveStateLocked()
	r.mu.Unlock()
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

func (r *Runner) executeDream(ctx context.Context, sessionID string) error {
	r.log("dream: session %s starting", sessionID)
	ledger, err := r.newLedger(ctx, r.projectDir)
	if err != nil {
		return err
	}
	defer func() { _ = ledger.Close() }()

	runID := generateDreamID()
	record := newRunRecord(r.projectDir, runID, r.agent, "", time.Now())
	if err := ledger.Put(ctx, record); err != nil {
		return fmt.Errorf("starting Dream run ledger: %w", err)
	}

	client, err := r.newClient()
	if err != nil {
		return finishFailedRun(ctx, ledger, record, fmt.Errorf("creating AI client: %w", err))
	}
	if identified, ok := client.(interface{ AgentCLI() string }); ok {
		record.CLI = identified.AgentCLI()
	}

	streamer, ok := client.(ai.StreamClient)
	if !ok {
		return finishFailedRun(ctx, ledger, record, fmt.Errorf("configured AI client does not support agentic streaming"))
	}
	stats, err := r.executeLocal(ctx, streamer, buildDreamPrompt(sessionID, r.agent), runID)
	record.ToolCalls = stats.toolCalls
	record.MemoryMutationAttempts = stats.memoryMutationAttempts
	record.TargetIDs = stats.targetIDs
	if err != nil {
		return finishFailedRun(ctx, ledger, record, fmt.Errorf("executing Dream agent: %w", err))
	}
	record.Status = "completed"
	if !stats.structured {
		// Exit success is observable, but tool use is not. Zero counters cannot
		// be interpreted as proof that no Memory mutation was attempted.
		record.Status = "completed_unobserved"
	}
	record.FinishedAt = time.Now().UTC().Format(time.RFC3339Nano)
	if err := ledger.Put(ctx, record); err != nil {
		return fmt.Errorf("finishing Dream run ledger: %w", err)
	}
	if stats.structured {
		r.log("dream: session %s completed with %d tool calls and %d Memory mutation attempts", sessionID, stats.toolCalls, stats.memoryMutationAttempts)
	} else {
		r.log("dream: session %s CLI exited successfully without structured tool telemetry; Memory mutation attempts are unknown", sessionID)
	}
	return nil
}

type executionStats struct {
	structured             bool
	toolCalls              int64
	memoryMutationAttempts int64
	targetIDs              []string
}

var memoryMutationTools = map[string]bool{
	"graphit_memory_insert": true, "graphit_memory_update": true,
	"graphit_memory_delete": true, "graphit_memory_promote": true,
	"graphit_memory_demote": true,
}

var memoryIDPattern = regexp.MustCompile(`\b[0-9A-HJKMNP-TV-Z]{26}\b`)

func (r *Runner) executeLocal(ctx context.Context, streamer ai.StreamClient, prompt, runID string) (executionStats, error) {
	var stats executionStats
	changed := map[string]bool{}
	mutationCalls := map[string]bool{}

	result, err := streamer.CompleteStream(ctx, ai.StreamRequest{
		UserPrompt:     prompt,
		PersistSession: false,
		WorkDir:        r.projectDir,
		AllowTools:     true,
		Capabilities:   ai.DreamMemoryCapabilities(),
		Env: map[string]string{
			agentpolicy.EnvRunID: runID,
			"GRAPHIT_UNIT_ID":    "dream:" + runID,
		},
	}, func(ev ai.Event) {
		if ev.Kind == ai.EventToolUse {
			stats.toolCalls++
			if memoryMutationTools[ev.Tool] {
				stats.memoryMutationAttempts++
				mutationCalls[ev.ToolCallID] = true
				collectMemoryIDs(changed, ev.Detail)
			}
		}
		if ev.Kind == ai.EventToolResult && mutationCalls[ev.ToolCallID] {
			collectMemoryIDs(changed, ev.Detail)
		}
	})
	if err != nil {
		return stats, err
	}
	stats.structured = result != nil && result.Structured
	if result != nil && result.Structured && stats.toolCalls == 0 {
		return stats, fmt.Errorf("dream agent completed without using any MCP tools")
	}
	for id := range changed {
		stats.targetIDs = append(stats.targetIDs, id)
	}
	sort.Strings(stats.targetIDs)
	return stats, nil
}

func collectMemoryIDs(dst map[string]bool, detail string) {
	for _, id := range memoryIDPattern.FindAllString(detail, -1) {
		dst[id] = true
	}
}

func finishFailedRun(ctx context.Context, ledger runLedgerWriter, record RunRecord, runErr error) error {
	record.Status = "failed"
	record.FinishedAt = time.Now().UTC().Format(time.RFC3339Nano)
	record.ErrorSummary = runErr.Error()
	if err := ledger.Put(context.WithoutCancel(ctx), record); err != nil {
		return fmt.Errorf("%w; recording failed Dream run: %w", runErr, err)
	}
	return runErr
}

func StatePath(projectDir string) string {
	return brand.ProjectRuntimePath(projectDir, "dream", stateFile)
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

func generateDreamID() string {
	return ulid.Make().String()
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

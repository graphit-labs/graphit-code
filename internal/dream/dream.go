package dream

import (
	"context"
	"encoding/json"
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	gitmod "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
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
	CurrentULID string `json:"current_ulid,omitempty"`

	LastUserModTime time.Time `json:"last_user_mod_time"`

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

func (r *Runner) SetLogger(fn func(string, ...any)) {
	r.logFn = fn
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

	ulid := r.resolveSessionULID(lastMod)

	r.mu.Lock()
	exhausted := r.state.Exhausted
	r.mu.Unlock()
	if exhausted {
		return
	}

	r.log("dream: all preconditions met (idle=%s, session=%s), starting dream",
		idleDuration.Truncate(time.Second), ulid)

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

		if err := r.executeDream(dreamCtx, ulid); err != nil {
			r.log("dream: session failed: %v", err)
		} else {
			r.log("dream: session completed successfully")
		}

		r.checkDeepSleep(ulid)
	}()
}

func (r *Runner) resolveSessionULID(currentModTime time.Time) string {
	r.mu.Lock()
	defer r.mu.Unlock()

	needsNew := r.state.CurrentULID == "" ||
		currentModTime.After(r.state.LastUserModTime)

	if needsNew {
		r.state.CurrentULID = generateDreamID()
		r.state.LastUserModTime = currentModTime
		r.state.Exhausted = false
		r.saveStateLocked()
		r.log("dream: new session %s (user activity detected since last dream)", r.state.CurrentULID)
	} else {
		r.log("dream: resuming session %s (no user changes since last dream)", r.state.CurrentULID)
	}

	return r.state.CurrentULID
}

func (r *Runner) Stop() {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.cancelFn != nil {
		r.cancelFn()
	}
}

const exhaustedSentinel = ".exhausted"

func (r *Runner) checkDeepSleep(ulid string) {
	sentinelPath := filepath.Join(r.projectDir, brand.DotDir(), "dream", ulid+exhaustedSentinel)
	if !fileExists(sentinelPath) {
		return
	}

	r.log("dream: deep sleep signal detected for session %s — no more improvements to make", ulid)

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

func (r *Runner) executeDream(ctx context.Context, ulid string) error {
	branchName := "dream/" + ulid
	worktreeDir := filepath.Join(r.projectDir, brand.DotDir(), "dream", "worktrees", ulid)

	dreamArtifactDir := filepath.Join(r.projectDir, brand.DotDir(), "dream")

	r.log("dream: session %s starting (branch=%s)", ulid, branchName)

	if err := r.ensureWorktree(ctx, branchName, worktreeDir); err != nil {
		return fmt.Errorf("ensuring dream worktree: %w", err)
	}

	if err := os.MkdirAll(dreamArtifactDir, 0o755); err != nil {
		return fmt.Errorf("creating dream artifact dir: %w", err)
	}

	var subject *Subject
	if s, err := PickSubject(r.projectDir); err == nil && s != nil {
		subject = s
		r.log("dream: picked subject %q (%s)", s.Title, s.Slug)
	}

	r.log("dream: executing AI agent locally for %s", worktreeDir)
	prompt := buildDreamPrompt(r.projectDir, ulid, r.ide, subject)
	result, err := r.executeLocal(ctx, worktreeDir, prompt, ulid)
	if err != nil {
		return fmt.Errorf("executing dream agent: %w", err)
	}

	artifactPath := filepath.Join(dreamArtifactDir, ulid+".md")
	if err := os.WriteFile(artifactPath, []byte(result), 0o644); err != nil {
		return fmt.Errorf("writing dream artifact: %w", err)
	}

	if err := r.commitDream(ctx, worktreeDir, ulid); err != nil {
		return fmt.Errorf("committing dream: %w", err)
	}

	r.log("dream: session %s completed — report at %s, code on branch %s", ulid, artifactPath, branchName)
	return nil
}

func (r *Runner) ensureWorktree(ctx context.Context, branch, dir string) error {

	if _, err := os.Stat(dir); err == nil {
		r.log("dream: reusing existing worktree at %s", dir)
		return nil
	}

	if err := os.MkdirAll(filepath.Dir(dir), 0o755); err != nil {
		return err
	}

	g := gitmod.Default()
	if err := g.Run(r.projectDir, "worktree", "add", dir, "-b", branch); err != nil {
		return fmt.Errorf("creating dream worktree: %w", err)
	}

	r.log("dream: created new worktree at %s on branch %s", dir, branch)
	return nil
}



func (r *Runner) executeLocal(ctx context.Context, worktreeDir, prompt, ulid string) (string, error) {
	dreamArtifactDir := filepath.Join(r.projectDir, brand.DotDir(), "dream")

	agentInstruction := fmt.Sprintf(`%s

IMPORTANT INSTRUCTIONS:
- You are working in a git worktree on branch dream/%s
- This may be a progressive session — check for existing work on this branch
- Code changes (improvements, refactors) go into the worktree and will be committed
- The dream report MUST be written to %s/%s.md
- Write your findings in the report describing:
  1. What you analyzed and reflected on
  2. What you learned about the system
  3. What improvements you made (if any) and why
  4. How the changes impact the codebase
  5. What memories or context you based your decisions on
- Commit all CODE changes with a descriptive message
- Focus on code quality, reuse, patterns, and best practices
- Use IDE context: %s
`, prompt, ulid, dreamArtifactDir, ulid, r.ide)

	client, err := ai.NewClientFromConfig()
	if err != nil {
		return "", fmt.Errorf("creating AI client: %w", err)
	}

	out, err := client.Complete(ctx, "", agentInstruction)
	if err != nil {
		return "", fmt.Errorf("agent execution failed: %w", err)
	}

	artifact := buildDreamArtifact(ulid, out)
	return artifact, nil
}

func (r *Runner) commitDream(ctx context.Context, worktreeDir, ulid string) error {
	g := gitmod.Default()

	if err := g.Run(worktreeDir, "add", "-A"); err != nil {
		return fmt.Errorf("git add: %w", err)
	}

	if err := g.Run(worktreeDir, "diff", "--cached", "--quiet"); err == nil {

		r.log("dream: no changes to commit for session %s", ulid)
		return nil
	}

	msg := fmt.Sprintf("dream(%s): autonomous reflection and improvement", ulid)
	authorVal := fmt.Sprintf("%s Dream <dream@%s>", brand.DisplayName, brand.Brand)
	if err := g.Run(worktreeDir, "commit", "-m", msg, "--author", authorVal); err != nil {
		return fmt.Errorf("git commit: %w", err)
	}

	return nil
}

func StatePath(projectDir string) string {
	return filepath.Join(projectDir, brand.DotDir(), "daemon", stateFile)
}

func (r *Runner) statePath() string {
	return StatePath(r.projectDir)
}

func LoadStateFromDir(projectDir string) (currentULID string, lastUserMod, lastDreamAt, dreamStartedAt, sleepingSince time.Time, exhausted, dreaming bool) {
	data, err := os.ReadFile(StatePath(projectDir))
	if err != nil {
		return
	}
	var s dreamState
	if err := json.Unmarshal(data, &s); err != nil {
		return
	}
	return s.CurrentULID, s.LastUserModTime, s.LastDreamAt, s.DreamStartedAt, s.SleepingSince, s.Exhausted, s.Dreaming
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
	now := time.Now().UTC()
	ts := now.Format("20060102T150405")
	r := rand.New(rand.NewSource(now.UnixNano()))
	suffix := fmt.Sprintf("%04x", r.Intn(0xFFFF))
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
				if ic.IsIgnored(rel, true) {
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

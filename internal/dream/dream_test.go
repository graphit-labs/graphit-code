package dream

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestStatePath(t *testing.T) {
	p := StatePath("/tmp/myproject")
	if !strings.Contains(p, filepath.Join("runtime", "dream", "dream.state")) {
		t.Errorf("unexpected state path: %q", p)
	}
	if strings.Contains(p, filepath.Join("runtime", "daemon")) {
		t.Errorf("Dream state must not remain under daemon runtime: %q", p)
	}
}

func TestGenerateDreamID(t *testing.T) {
	id1 := generateDreamID()
	id2 := generateDreamID()
	if id1 == "" {
		t.Error("generated dream ID should not be empty")
	}
	if len(id1) < 10 {
		t.Error("dream ID seems too short")
	}
	if id1 == id2 {
		t.Error("two generated IDs should differ")
	}
}

func TestLastModifiedTime(t *testing.T) {
	dir := t.TempDir()

	_, err := LastModifiedTime(dir)
	if err == nil {
		t.Error("expected error for empty directory")
	}

	filePath := filepath.Join(dir, "file.txt")
	_ = os.WriteFile(filePath, []byte("hello"), 0644)

	modTime, err := LastModifiedTime(dir)
	if err != nil {
		t.Fatalf("LastModifiedTime failed: %v", err)
	}
	if modTime.IsZero() {
		t.Error("expected non-zero mod time")
	}
}

func TestLastModifiedTimeSkipsGitDir(t *testing.T) {
	dir := t.TempDir()
	_ = os.MkdirAll(filepath.Join(dir, ".git"), 0o755)
	_ = os.WriteFile(filepath.Join(dir, ".git", "HEAD"), []byte("ref: refs/heads/main"), 0644)

	_ = os.WriteFile(filepath.Join(dir, "src.go"), []byte("package main"), 0644)

	modTime, err := LastModifiedTime(dir)
	if err != nil {
		t.Fatalf("LastModifiedTime failed: %v", err)
	}
	if modTime.IsZero() {
		t.Error("expected non-zero mod time")
	}
}

func TestLastModifiedTimeSkipsBrandDir(t *testing.T) {
	dir := t.TempDir()
	brandDir := brand.DotDir()
	_ = os.MkdirAll(filepath.Join(dir, brandDir), 0o755)
	_ = os.WriteFile(filepath.Join(dir, brandDir, "config.json"), []byte("{}"), 0644)

	_ = os.WriteFile(filepath.Join(dir, "main.go"), []byte("package main"), 0644)

	modTime, err := LastModifiedTime(dir)
	if err != nil {
		t.Fatalf("LastModifiedTime failed: %v", err)
	}
	if modTime.IsZero() {
		t.Error("expected non-zero mod time")
	}
}

func TestLastModifiedTimeNestedFiles(t *testing.T) {
	dir := t.TempDir()
	subDir := filepath.Join(dir, "subdir")
	_ = os.MkdirAll(subDir, 0o755)

	filePath := filepath.Join(subDir, "nested.txt")
	_ = os.WriteFile(filePath, []byte("nested"), 0644)
	time.Sleep(10 * time.Millisecond)

	rootFile := filepath.Join(dir, "root.txt")
	_ = os.WriteFile(rootFile, []byte("root"), 0644)

	modTime, err := LastModifiedTime(dir)
	if err != nil {
		t.Fatalf("LastModifiedTime failed: %v", err)
	}

	rootInfo, _ := os.Stat(rootFile)
	if !modTime.Equal(rootInfo.ModTime()) {
		t.Errorf("expected latest mod time to match root file, got %v vs %v", modTime, rootInfo.ModTime())
	}
}

func TestLoadStateFromDir(t *testing.T) {
	dir := t.TempDir()

	sessionID, lastMod, lastDream, dreamStarted, sleepingSince, exhausted, dreaming := LoadStateFromDir(dir)
	if sessionID != "" || !lastMod.IsZero() || !lastDream.IsZero() || !dreamStarted.IsZero() || !sleepingSince.IsZero() || exhausted || dreaming {
		t.Error("expected zero values when no state file exists")
	}

	stateDir := filepath.Dir(StatePath(dir))
	_ = os.MkdirAll(stateDir, 0o755)

	state := dreamState{
		CurrentSessionID: "test-session",
		LastUserModTime:  time.Now().Add(-1 * time.Hour),
		Exhausted:        true,
		Dreaming:         false,
		DreamStartedAt:   time.Time{},
		SleepingSince:    time.Now().Add(-30 * time.Minute),
		LastDreamAt:      time.Now().Add(-2 * time.Hour),
	}
	data, _ := json.MarshalIndent(state, "", "  ")
	_ = os.WriteFile(StatePath(dir), data, 0644)

	sessionID, lastMod, lastDream, _, sleepingSince, exhausted, dreaming = LoadStateFromDir(dir)
	if sessionID != "test-session" {
		t.Errorf("expected session id 'test-session', got %q", sessionID)
	}
	if lastMod.IsZero() {
		t.Error("expected non-zero last mod time")
	}
	if lastDream.IsZero() {
		t.Error("expected non-zero last dream time")
	}
	if sleepingSince.IsZero() {
		t.Error("expected non-zero sleeping since")
	}
	if !exhausted {
		t.Error("expected exhausted=true")
	}
	if dreaming {
		t.Error("expected dreaming=false")
	}
}

func TestLoadStateFromDirInvalidJSON(t *testing.T) {
	dir := t.TempDir()
	stateDir := filepath.Dir(StatePath(dir))
	_ = os.MkdirAll(stateDir, 0o755)
	_ = os.WriteFile(StatePath(dir), []byte("invalid json{{{"), 0644)

	sessionID, _, _, _, _, _, _ := LoadStateFromDir(dir)
	if sessionID != "" {
		t.Error("expected empty sessionID for invalid JSON")
	}
}

func TestNewRunner(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "test-agent", nil)
	if r == nil {
		t.Fatal("NewRunner returned nil")
	}
	if r.projectDir != dir {
		t.Errorf("expected projectDir=%q, got %q", dir, r.projectDir)
	}
	if r.agent != "test-agent" {
		t.Errorf("expected agent='test-agent', got %q", r.agent)
	}
}

func TestNewRunnerWithExistingState(t *testing.T) {
	dir := t.TempDir()

	stateDir := filepath.Dir(StatePath(dir))
	_ = os.MkdirAll(stateDir, 0o755)
	state := dreamState{
		Dreaming:      true,
		SleepingSince: time.Now(),
	}
	data, _ := json.MarshalIndent(state, "", "  ")
	_ = os.WriteFile(StatePath(dir), data, 0644)

	r := NewRunner(dir, "agent", nil)
	if r.state.Dreaming != true {
		t.Error("expected dreaming=true from loaded state")
	}
}

func TestNewRunnerSetsInitialSleepingSince(t *testing.T) {
	dir := t.TempDir()

	r := NewRunner(dir, "agent", nil)
	if r.state.SleepingSince.IsZero() {
		t.Error("expected SleepingSince to be set for new runner")
	}
}

func TestRunnerLogNoLogger(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)
	r.log("test message")
}

func TestRunnerIsRunning(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)
	if r.IsRunning() {
		t.Error("expected not running initially")
	}
}

func TestRunnerResolveConfig(t *testing.T) {
	dir := t.TempDir()

	t.Run("nil project config", func(t *testing.T) {
		r := NewRunner(dir, "agent", nil)
		cfg := r.resolveConfig()
		if cfg.IdleTimeout != defaultIdleTimeout {
			t.Errorf("expected default idle timeout, got %v", cfg.IdleTimeout)
		}
		if cfg.MaxDuration != defaultMaxDuration {
			t.Errorf("expected default max duration, got %v", cfg.MaxDuration)
		}
	})

	t.Run("with project config", func(t *testing.T) {
		loader := func() map[string]any {
			return map[string]any{
				"dream": map[string]any{
					"idle_timeout": "300",
					"max_duration": "600",
				},
			}
		}
		r := NewRunner(dir, "agent", loader)
		cfg := r.resolveConfig()
		if cfg.IdleTimeout != 300*time.Second {
			t.Errorf("expected 300s idle timeout, got %v", cfg.IdleTimeout)
		}
		if cfg.MaxDuration != 600*time.Second {
			t.Errorf("expected 600s max duration, got %v", cfg.MaxDuration)
		}
	})
}

func TestResolveDreamConfig(t *testing.T) {
	tests := []struct {
		name     string
		cfg      map[string]any
		wantIdle time.Duration
		wantMax  time.Duration
	}{
		{
			name:     "nil config",
			cfg:      nil,
			wantIdle: defaultIdleTimeout,
			wantMax:  defaultMaxDuration,
		},
		{
			name:     "empty config",
			cfg:      map[string]any{},
			wantIdle: defaultIdleTimeout,
			wantMax:  defaultMaxDuration,
		},
		{
			name: "custom idle timeout",
			cfg: map[string]any{
				"dream": map[string]any{"idle_timeout": "120"},
			},
			wantIdle: 120 * time.Second,
			wantMax:  defaultMaxDuration,
		},
		{
			name: "custom max duration",
			cfg: map[string]any{
				"dream": map[string]any{"max_duration": "3600"},
			},
			wantIdle: defaultIdleTimeout,
			wantMax:  3600 * time.Second,
		},
		{
			name: "max_duration zero means disabled",
			cfg: map[string]any{
				"dream": map[string]any{"max_duration": "0"},
			},
			wantIdle: defaultIdleTimeout,
			wantMax:  0,
		},
		{
			name: "invalid idle timeout (non-numeric)",
			cfg: map[string]any{
				"dream": map[string]any{"idle_timeout": "abc"},
			},
			wantIdle: defaultIdleTimeout,
			wantMax:  defaultMaxDuration,
		},
		{
			name: "negative idle timeout",
			cfg: map[string]any{
				"dream": map[string]any{"idle_timeout": "-5"},
			},
			wantIdle: defaultIdleTimeout,
			wantMax:  defaultMaxDuration,
		},
		{
			name: "negative max duration",
			cfg: map[string]any{
				"dream": map[string]any{"max_duration": "-10"},
			},
			wantIdle: defaultIdleTimeout,
			wantMax:  defaultMaxDuration,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			cfg := ResolveDreamConfig(tc.cfg)
			if cfg.IdleTimeout != tc.wantIdle {
				t.Errorf("IdleTimeout = %v, want %v", cfg.IdleTimeout, tc.wantIdle)
			}
			if cfg.MaxDuration != tc.wantMax {
				t.Errorf("MaxDuration = %v, want %v", cfg.MaxDuration, tc.wantMax)
			}
		})
	}
}

func TestRunnerCheckDeepSleep(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)
	r.checkDeepSleep("test-session")
	if !r.state.Exhausted {
		t.Error("a completed consolidation must exhaust the current idle session")
	}
	if _, err := os.Stat(filepath.Join(brand.ProjectRuntimePath(dir, "dream"), "test-session.exhausted")); !os.IsNotExist(err) {
		t.Error("deep sleep must be represented only in dream.state")
	}
}

func TestRunnerResolveSessionID(t *testing.T) {
	dir := t.TempDir()

	t.Run("new session - empty session id", func(t *testing.T) {
		r := NewRunner(dir, "agent", nil)
		var logged []string
		r.logFn = func(format string, args ...any) {
			logged = append(logged, format)
		}
		sessionID := r.resolveSessionID(time.Now())
		if sessionID == "" {
			t.Error("expected non-empty session id")
		}
		if r.state.CurrentSessionID != sessionID {
			t.Error("state should be updated with new session id")
		}
	})

	t.Run("resume session - same mod time", func(t *testing.T) {
		r := NewRunner(dir, "agent", nil)
		var logged []string
		r.logFn = func(format string, args ...any) {
			logged = append(logged, format)
		}
		modTime := time.Now()
		session1 := r.resolveSessionID(modTime)
		session2 := r.resolveSessionID(modTime.Add(-1 * time.Second))
		if session1 != session2 {
			t.Errorf("expected same session id for resume, got %q vs %q", session1, session2)
		}
	})

	t.Run("new session - newer mod time", func(t *testing.T) {
		r := NewRunner(dir, "agent", nil)
		modTime := time.Now()
		session1 := r.resolveSessionID(modTime)
		session2 := r.resolveSessionID(modTime.Add(1 * time.Second))
		if session1 == session2 {
			t.Error("expected different session id for new session")
		}
	})
}

func TestRunnerSaveAndLoadState(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)
	r.mu.Lock()
	r.state.CurrentSessionID = "saved-session"
	r.state.Exhausted = true
	r.saveStateLocked()
	r.mu.Unlock()

	r2 := NewRunner(dir, "agent", nil)
	if r2.state.CurrentSessionID != "saved-session" {
		t.Errorf("expected loaded session id='saved-session', got %q", r2.state.CurrentSessionID)
	}
	if !r2.state.Exhausted {
		t.Error("expected loaded exhausted=true")
	}
}

func TestRunnerTickCancelledContext(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	r.tick(ctx)
}

func TestRunnerTickDisabled(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)
	ctx := context.Background()
	r.tick(ctx)
}

func TestRunnerTickAlreadyRunning(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)
	r.mu.Lock()
	r.running = true
	r.mu.Unlock()
	ctx := context.Background()
	r.tick(ctx)
}

func TestRunnerTickNoFiles(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
		}
	})
	var logged []string
	r.logFn = func(format string, args ...any) {
		logged = append(logged, format)
	}
	ctx := context.Background()
	r.tick(ctx)
	foundError := false
	for _, log := range logged {
		if strings.Contains(log, "failed to check idle time") {
			foundError = true
		}
	}
	if !foundError {
		t.Error("expected log about failed idle time check")
	}
}

func TestRunnerTickNotIdleEnough(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "recent.txt"), []byte("data"), 0644)

	r := NewRunner(dir, "agent", func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
		}
	})
	ctx := context.Background()
	r.tick(ctx)
	if r.IsRunning() {
		t.Error("should not be running when idle time is insufficient")
	}
}

func TestRunnerTickExhausted(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "old.txt")
	_ = os.WriteFile(filePath, []byte("data"), 0644)
	oldTime := time.Now().Add(-5 * time.Hour)
	_ = os.Chtimes(filePath, oldTime, oldTime)

	loader := func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
			"dream":   map[string]any{"idle_timeout": "1"},
		}
	}
	r := NewRunner(dir, "agent", loader)
	r.mu.Lock()
	r.state.Exhausted = true
	r.state.CurrentSessionID = "existing-session"
	r.state.SessionModWatermark = time.Now()
	r.mu.Unlock()

	ctx := context.Background()
	r.tick(ctx)
	if r.IsRunning() {
		t.Error("should not start dream when exhausted")
	}
}

// The other half of deep sleep, which had no test and did not work: exhaustion has
// to END. Exhausted was only ever cleared on session rotation, and rotation
// compared the newest mtime against a field tick had already overwritten with that
// same mtime — so the comparison was never true, and the first deep sleep was
// permanent for the life of the project.
func TestRunnerTickWakesFromDeepSleepOnNewActivity(t *testing.T) {
	dir := t.TempDir()

	oldFile := filepath.Join(dir, "old.txt")
	if err := os.WriteFile(oldFile, []byte("data"), 0644); err != nil {
		t.Fatal(err)
	}
	oldTime := time.Now().Add(-5 * time.Hour)
	if err := os.Chtimes(oldFile, oldTime, oldTime); err != nil {
		t.Fatal(err)
	}

	loader := func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
			"dream":   map[string]any{"idle_timeout": "1"},
		}
	}

	r := NewRunner(dir, "agent", loader)
	r.mu.Lock()
	r.state.Exhausted = true
	r.state.CurrentSessionID = "exhausted-session"
	r.state.SessionModWatermark = oldTime
	r.mu.Unlock()

	newFile := filepath.Join(dir, "new.txt")
	if err := os.WriteFile(newFile, []byte("new work"), 0644); err != nil {
		t.Fatal(err)
	}
	idleButNewer := time.Now().Add(-2 * time.Hour)
	if err := os.Chtimes(newFile, idleButNewer, idleButNewer); err != nil {
		t.Fatal(err)
	}

	sessionID := r.resolveSessionID(idleButNewer)

	r.mu.Lock()
	exhausted := r.state.Exhausted
	watermark := r.state.SessionModWatermark
	r.mu.Unlock()

	if exhausted {
		t.Error("new activity must clear Exhausted — otherwise the first deep sleep is permanent")
	}
	if sessionID == "exhausted-session" {
		t.Error("new activity must open a new session, not resume the exhausted one")
	}
	if !watermark.Equal(idleButNewer) {
		t.Errorf("watermark = %v; want it advanced to the mtime that opened the session (%v)", watermark, idleButNewer)
	}
}

// And the inverse, which is what the watermark protects: a tick with no new
// activity must resume the same session rather than rotating. Rotation resets
// Exhausted, so a runner that rotated on every tick could never stay asleep.
func TestRunnerResumesSameSessionWithoutNewActivity(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)

	modTime := time.Now().Add(-3 * time.Hour)
	first := r.resolveSessionID(modTime)
	second := r.resolveSessionID(modTime)

	if first != second {
		t.Errorf("same mtime must resume the same session: %q then %q", first, second)
	}
}

func TestRunnerRunContextCancel(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := r.Run(ctx)
	if err != nil {
		t.Errorf("Run should return nil on context cancel, got %v", err)
	}
}

func TestRunnerRunLoop(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)

	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()

	err := r.Run(ctx)
	if err != nil {
		t.Errorf("Run should return nil on context cancel, got %v", err)
	}
}

func TestLastModifiedTimeWithIgnoredDir(t *testing.T) {
	dir := t.TempDir()

	gitignorePath := filepath.Join(dir, ".gitignore")
	_ = os.WriteFile(gitignorePath, []byte("build/\n"), 0644)

	buildDir := filepath.Join(dir, "build")
	_ = os.MkdirAll(buildDir, 0o755)
	_ = os.WriteFile(filepath.Join(buildDir, "output.bin"), []byte("binary"), 0644)

	_ = os.WriteFile(filepath.Join(dir, "src.go"), []byte("package main"), 0644)

	modTime, err := LastModifiedTime(dir)
	if err != nil {
		t.Fatalf("LastModifiedTime failed: %v", err)
	}
	if modTime.IsZero() {
		t.Error("expected non-zero mod time")
	}
}

func TestLastModifiedTimeWithIgnoredFile(t *testing.T) {
	dir := t.TempDir()

	_ = os.WriteFile(filepath.Join(dir, ".gitignore"), []byte("*.log\n"), 0644)
	_ = os.WriteFile(filepath.Join(dir, "app.log"), []byte("log data"), 0644)
	_ = os.WriteFile(filepath.Join(dir, "main.go"), []byte("package main"), 0644)

	modTime, err := LastModifiedTime(dir)
	if err != nil {
		t.Fatalf("LastModifiedTime failed: %v", err)
	}
	if modTime.IsZero() {
		t.Error("expected non-zero mod time")
	}
}

func TestBuildDreamPrompt(t *testing.T) {
	result := buildDreamPrompt("test-session", "vscode")
	if result == "" {
		t.Error("expected non-empty prompt")
	}
	for _, want := range []string{
		"graphit_memory_insert", "graphit_memory_update", "graphit_memory_delete",
		"graphit_memory_promote", "graphit_memory_demote", "expected_revision",
		"Task/session", "Knowledge/Wiki", "AST", "Hub", "References",
	} {
		if !strings.Contains(result, want) {
			t.Errorf("prompt should contain %q", want)
		}
	}
	for _, forbidden := range []string{"Dream Report", "skill generation", "create skills", "/tmp/project"} {
		if strings.Contains(strings.ToLower(result), strings.ToLower(forbidden)) {
			t.Errorf("Memory-only prompt contains forbidden concept %q", forbidden)
		}
	}
}

func TestRunnerStatePath(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)
	p := r.statePath()
	if p != StatePath(dir) {
		t.Errorf("expected statePath() == StatePath(), got %q vs %q", p, StatePath(dir))
	}
}

func TestRunnerLoadStateInvalidJSON(t *testing.T) {
	dir := t.TempDir()
	stateDir := filepath.Dir(StatePath(dir))
	_ = os.MkdirAll(stateDir, 0o755)
	_ = os.WriteFile(StatePath(dir), []byte("{{invalid"), 0644)

	r := NewRunner(dir, "agent", nil)
	if r.state.CurrentSessionID != "" {
		t.Error("expected empty session id after invalid JSON load")
	}
}

func TestRunnerSaveStateLocked(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)

	r.mu.Lock()
	r.state.CurrentSessionID = "save-test"
	r.state.Dreaming = true
	r.saveStateLocked()
	r.mu.Unlock()

	data, err := os.ReadFile(r.statePath())
	if err != nil {
		t.Fatalf("expected state file to exist: %v", err)
	}
	if !strings.Contains(string(data), "save-test") {
		t.Error("state file should contain the session id")
	}
}

type fakeDreamClient struct {
	mu     sync.Mutex
	calls  int
	reqs   []ai.StreamRequest
	events []ai.Event
	err    error
}

func (c *fakeDreamClient) Complete(context.Context, string, string) (string, error) {
	return "", c.err
}

func (c *fakeDreamClient) CompleteStream(_ context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
	c.mu.Lock()
	c.calls++
	c.reqs = append(c.reqs, req)
	c.mu.Unlock()
	for _, ev := range c.events {
		emit(ev)
	}
	return &ai.StreamResult{Structured: true, Binary: "codex", Text: "discard me"}, c.err
}

func (c *fakeDreamClient) SupportsStructuredStream() bool { return true }
func (c *fakeDreamClient) AgentCLI() string               { return "codex" }

type fakeNonStreamingClient struct{}

func (fakeNonStreamingClient) Complete(context.Context, string, string) (string, error) {
	return "must not run", nil
}

type fakeUnstructuredDreamClient struct{ fakeDreamClient }

func (*fakeUnstructuredDreamClient) SupportsStructuredStream() bool { return false }
func (c *fakeUnstructuredDreamClient) CompleteStream(ctx context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
	result, err := c.fakeDreamClient.CompleteStream(ctx, req, emit)
	if result != nil {
		result.Structured = false
	}
	return result, err
}

type fakeRunLedger struct {
	mu      sync.Mutex
	records []RunRecord
}

func (l *fakeRunLedger) Put(_ context.Context, record RunRecord) error {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.records = append(l.records, record)
	return nil
}
func (l *fakeRunLedger) Close() error { return nil }

func newTestDreamRunner(t *testing.T, client ai.Client) (*Runner, *fakeRunLedger) {
	t.Helper()
	r := NewRunner(t.TempDir(), "test-agent", nil)
	ledger := &fakeRunLedger{}
	r.newClient = func() (ai.Client, error) { return client, nil }
	r.newLedger = func(context.Context, string) (runLedgerWriter, error) { return ledger, nil }
	return r, ledger
}

func TestExecuteDreamRunsOneMemoryOnlyAgenticCallAndWritesNoResultFile(t *testing.T) {
	const memoryID = "01AAAAAAAAAAAAAAAAAAAAAAAA"
	client := &fakeDreamClient{events: []ai.Event{
		{Kind: ai.EventToolUse, Tool: "graphit_memory_update", ToolCallID: "call-1", Detail: `{"id":"` + memoryID + `"}`},
		{Kind: ai.EventToolResult, ToolCallID: "call-1", Detail: `{"id":"` + memoryID + `","revision":2}`},
	}}
	r, ledger := newTestDreamRunner(t, client)

	if err := r.executeDream(context.Background(), "test-session"); err != nil {
		t.Fatalf("executeDream: %v", err)
	}
	if client.calls != 1 {
		t.Fatalf("CompleteStream calls = %d, want exactly 1", client.calls)
	}
	req := client.reqs[0]
	if req.PersistSession || !req.AllowTools || req.WorkDir != r.projectDir {
		t.Fatalf("unexpected request: %+v", req)
	}
	if req.Capabilities == nil || !req.Capabilities.RestrictNativeToolsWhenAvailable || req.Capabilities.MCPProfile != agentpolicy.ProfileDreamMemory {
		t.Fatalf("missing exact Dream capability policy: %+v", req.Capabilities)
	}
	if req.Env["GRAPHIT_DREAM_RUN_ID"] == "" || req.Env["GRAPHIT_DREAM_RUN_ID"] == "test-session" ||
		req.Env["GRAPHIT_UNIT_ID"] != "dream:"+req.Env["GRAPHIT_DREAM_RUN_ID"] {
		t.Fatalf("missing provenance env: %#v", req.Env)
	}
	if len(ledger.records) != 2 || ledger.records[1].Status != "completed" || ledger.records[1].MemoryMutationAttempts != 1 {
		t.Fatalf("unexpected run ledger: %#v", ledger.records)
	}
	if ledger.records[0].RunID != req.Env["GRAPHIT_DREAM_RUN_ID"] || ledger.records[1].RunID != ledger.records[0].RunID {
		t.Fatalf("run provenance and ledger disagree: env=%#v ledger=%#v", req.Env, ledger.records)
	}
	if len(ledger.records[1].TargetIDs) != 1 || ledger.records[1].TargetIDs[0] != memoryID {
		t.Fatalf("target IDs were not captured: %#v", ledger.records[1].TargetIDs)
	}
	entries, err := os.ReadDir(brand.ProjectRuntimePath(r.projectDir, "dream"))
	if err != nil {
		t.Fatal(err)
	}
	for _, entry := range entries {
		if strings.HasSuffix(entry.Name(), ".md") || strings.HasSuffix(entry.Name(), ".exhausted") {
			t.Fatalf("Dream wrote a forbidden result artifact: %s", entry.Name())
		}
	}
}

func TestExecuteDreamFailsClosedForNonStreamingClient(t *testing.T) {
	r, ledger := newTestDreamRunner(t, fakeNonStreamingClient{})
	err := r.executeDream(context.Background(), "test-session")
	if err == nil || !strings.Contains(err.Error(), "does not support agentic streaming") {
		t.Fatalf("unexpected error: %v", err)
	}
	if got := ledger.records[len(ledger.records)-1].Status; got != "failed" {
		t.Fatalf("ledger status = %q, want failed", got)
	}
}

func TestExecuteDreamAllowsUnstructuredCLIWithHonestTelemetry(t *testing.T) {
	client := &fakeUnstructuredDreamClient{}
	r, ledger := newTestDreamRunner(t, client)
	err := r.executeDream(context.Background(), "test-session")
	if err != nil {
		t.Fatalf("unstructured CLI should run: %v", err)
	}
	if client.calls != 1 {
		t.Fatalf("unstructured CLI was invoked %d times, want once", client.calls)
	}
	last := ledger.records[len(ledger.records)-1]
	if last.Status != "completed_unobserved" || last.ToolCalls != 0 || last.MemoryMutationAttempts != 0 {
		t.Fatalf("ledger did not distinguish unavailable telemetry: %#v", last)
	}
}

func TestExecuteDreamRecordsAgentFailureWithoutPersistingOutput(t *testing.T) {
	client := &fakeDreamClient{err: context.DeadlineExceeded}
	r, ledger := newTestDreamRunner(t, client)
	err := r.executeDream(context.Background(), "test-session")
	if err == nil {
		t.Fatal("expected agent failure")
	}
	last := ledger.records[len(ledger.records)-1]
	if last.Status != "failed" || !strings.Contains(last.ErrorSummary, "deadline") {
		t.Fatalf("unexpected failed ledger record: %#v", last)
	}
}

func TestExecuteDreamRejectsStructuredProseOnlyRun(t *testing.T) {
	client := &fakeDreamClient{}
	r, ledger := newTestDreamRunner(t, client)
	err := r.executeDream(context.Background(), "test-session")
	if err == nil || !strings.Contains(err.Error(), "without using any MCP tools") {
		t.Fatalf("unexpected error: %v", err)
	}
	if client.calls != 1 {
		t.Fatalf("CompleteStream calls = %d, want 1", client.calls)
	}
	if got := ledger.records[len(ledger.records)-1].Status; got != "failed" {
		t.Fatalf("ledger status = %q, want failed", got)
	}
}

func TestTickGoroutineSuccessWithFakeCLI(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "old.txt")
	_ = os.WriteFile(filePath, []byte("data"), 0644)
	oldTime := time.Now().Add(-5 * time.Hour)
	_ = os.Chtimes(filePath, oldTime, oldTime)

	loader := func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
			"dream":   map[string]any{"idle_timeout": "1", "max_duration": "0"},
		}
	}
	r := NewRunner(dir, "agent", loader)
	client := &fakeDreamClient{events: []ai.Event{{Kind: ai.EventToolUse, Tool: "graphit_memory_list", ToolCallID: "read-1"}}}
	ledger := &fakeRunLedger{}
	r.newClient = func() (ai.Client, error) { return client, nil }
	r.newLedger = func(context.Context, string) (runLedgerWriter, error) { return ledger, nil }
	var mu sync.Mutex
	var logged []string
	r.logFn = func(format string, args ...any) {
		mu.Lock()
		logged = append(logged, fmt.Sprintf(format, args...))
		mu.Unlock()
	}

	ctx := context.Background()
	r.tick(ctx)

	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		if !r.IsRunning() {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}

	if r.IsRunning() {
		time.Sleep(200 * time.Millisecond)
	}

	mu.Lock()
	foundSuccess := false
	for _, l := range logged {
		if strings.Contains(l, "session completed successfully") {
			foundSuccess = true
		}
	}
	mu.Unlock()
	if !foundSuccess {
		t.Error("expected 'session completed successfully' log message")
	}
}

func TestTickGoroutineCompletesWithError(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "old.txt")
	_ = os.WriteFile(filePath, []byte("data"), 0644)
	oldTime := time.Now().Add(-5 * time.Hour)
	_ = os.Chtimes(filePath, oldTime, oldTime)

	loader := func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
			"dream":   map[string]any{"idle_timeout": "1", "max_duration": "0"},
		}
	}
	r := NewRunner(dir, "agent", loader)
	var mu sync.Mutex
	var logged []string
	r.logFn = func(format string, args ...any) {
		mu.Lock()
		logged = append(logged, fmt.Sprintf(format, args...))
		mu.Unlock()
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	r.tick(ctx)

	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		if !r.IsRunning() {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}

	if r.IsRunning() {
		time.Sleep(200 * time.Millisecond)
		if r.IsRunning() {
			t.Fatal("goroutine did not complete within timeout")
		}
	}

	r.mu.Lock()
	if r.state.Dreaming {
		t.Error("expected Dreaming=false after goroutine defer")
	}
	if r.state.LastDreamAt.IsZero() {
		t.Error("expected LastDreamAt to be set after goroutine defer")
	}
	if r.state.SleepingSince.IsZero() {
		t.Error("expected SleepingSince to be set after goroutine defer")
	}
	if r.cancelFn != nil {
		t.Error("expected cancelFn to be nil after goroutine defer")
	}
	r.mu.Unlock()

	mu.Lock()
	foundFailed := false
	for _, l := range logged {
		if strings.Contains(l, "session failed") {
			foundFailed = true
		}
	}
	mu.Unlock()
	if !foundFailed {
		t.Error("expected 'session failed' log message from goroutine error path")
	}
}

func TestTickGoroutineChecksDeepSleep(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "old.txt")
	_ = os.WriteFile(filePath, []byte("data"), 0644)
	oldTime := time.Now().Add(-5 * time.Hour)
	_ = os.Chtimes(filePath, oldTime, oldTime)

	loader := func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
			"dream":   map[string]any{"idle_timeout": "1", "max_duration": "0"},
		}
	}
	r := NewRunner(dir, "agent", loader)
	client := &fakeDreamClient{events: []ai.Event{{Kind: ai.EventToolUse, Tool: "graphit_memory_list", ToolCallID: "read-1"}}}
	ledger := &fakeRunLedger{}
	r.newClient = func() (ai.Client, error) { return client, nil }
	r.newLedger = func(context.Context, string) (runLedgerWriter, error) { return ledger, nil }
	var mu sync.Mutex
	var logged []string
	r.logFn = func(format string, args ...any) {
		mu.Lock()
		logged = append(logged, fmt.Sprintf(format, args...))
		mu.Unlock()
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	r.tick(ctx)

	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		if !r.IsRunning() {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}

	if r.IsRunning() {
		time.Sleep(200 * time.Millisecond)
	}

	r.mu.Lock()
	if !r.state.Exhausted {
		t.Error("expected Exhausted=true after a successful run without any sentinel")
	}
	r.mu.Unlock()
}

func TestRunnerRunTickerFires(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "file.txt"), []byte("data"), 0644)
	r := NewRunner(dir, "agent", nil)
	var tickCount int
	var mu sync.Mutex
	r.logFn = func(format string, args ...any) {
		mu.Lock()
		tickCount++
		mu.Unlock()
	}

	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()

	err := r.Run(ctx)
	if err != nil {
		t.Errorf("Run should return nil, got %v", err)
	}
}

func TestTickAlreadyRunningWithEnabledDream(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "file.txt"), []byte("data"), 0644)

	loader := func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
		}
	}
	r := NewRunner(dir, "agent", loader)

	r.mu.Lock()
	r.running = true
	r.mu.Unlock()

	ctx := context.Background()
	r.tick(ctx)

}

func TestLastModifiedTimeWalkError(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "file.txt"), []byte("hello"), 0644)

	subdir := filepath.Join(dir, "restricted")
	_ = os.MkdirAll(subdir, 0o755)
	_ = os.WriteFile(filepath.Join(subdir, "inner.txt"), []byte("data"), 0644)
	_ = os.Chmod(subdir, 0o000)
	defer func() { _ = os.Chmod(subdir, 0o755) }()

	modTime, err := LastModifiedTime(dir)
	if err != nil {
		t.Fatalf("LastModifiedTime should succeed despite walk errors: %v", err)
	}
	if modTime.IsZero() {
		t.Error("expected non-zero mod time")
	}
}

func TestLastModifiedTimeNonExistentDir(t *testing.T) {
	_, err := LastModifiedTime("/nonexistent/path/that/does/not/exist")
	if err == nil {
		t.Error("expected error for non-existent directory")
	}
}

func TestRunWithTickerC(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "agent", nil)

	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(10 * time.Millisecond)
		cancel()
	}()

	err := r.Run(ctx)
	if err != nil {
		t.Errorf("Run should return nil: %v", err)
	}
}

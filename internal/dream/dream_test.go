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

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestDeepSleepSentinelName(t *testing.T) {
	name := DeepSleepSentinelName()
	if name != ".exhausted" {
		t.Errorf("expected '.exhausted', got %q", name)
	}
}

func TestStatePath(t *testing.T) {
	p := StatePath("/tmp/myproject")
	if !strings.Contains(p, "daemon") || !strings.Contains(p, "dream.state") {
		t.Errorf("unexpected state path: %q", p)
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

func TestFileExists(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "exists.txt")
	_ = os.WriteFile(filePath, []byte("data"), 0644)

	if !fileExists(filePath) {
		t.Error("expected fileExists to return true for existing file")
	}

	if fileExists(filepath.Join(dir, "nope.txt")) {
		t.Error("expected fileExists to return false for non-existing file")
	}

	if fileExists(dir) {
		t.Error("expected fileExists to return false for directory")
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
	r := NewRunner(dir, "test-ide", nil)
	if r == nil {
		t.Fatal("NewRunner returned nil")
	}
	if r.projectDir != dir {
		t.Errorf("expected projectDir=%q, got %q", dir, r.projectDir)
	}
	if r.ide != "test-ide" {
		t.Errorf("expected ide='test-ide', got %q", r.ide)
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

	r := NewRunner(dir, "ide", nil)
	if r.state.Dreaming != true {
		t.Error("expected dreaming=true from loaded state")
	}
}

func TestNewRunnerSetsInitialSleepingSince(t *testing.T) {
	dir := t.TempDir()

	r := NewRunner(dir, "ide", nil)
	if r.state.SleepingSince.IsZero() {
		t.Error("expected SleepingSince to be set for new runner")
	}
}

func TestRunnerLogNoLogger(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)
	r.log("test message")
}

func TestRunnerIsRunning(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)
	if r.IsRunning() {
		t.Error("expected not running initially")
	}
}

func TestRunnerResolveConfig(t *testing.T) {
	dir := t.TempDir()

	t.Run("nil project config", func(t *testing.T) {
		r := NewRunner(dir, "ide", nil)
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
		r := NewRunner(dir, "ide", loader)
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

	t.Run("no sentinel file", func(t *testing.T) {
		r := NewRunner(dir, "ide", nil)
		r.checkDeepSleep("test-session")
		if r.state.Exhausted {
			t.Error("expected exhausted=false when no sentinel")
		}
	})

	t.Run("with sentinel file", func(t *testing.T) {
		r := NewRunner(dir, "ide", nil)
		var logged []string
		r.logFn = func(format string, args ...any) {
			logged = append(logged, format)
		}
		sentinelDir := ReportsDir(dir)
		_ = os.MkdirAll(sentinelDir, 0o755)
		sentinelPath := filepath.Join(sentinelDir, "test-session"+exhaustedSentinel)
		_ = os.WriteFile(sentinelPath, nil, 0644)

		r.checkDeepSleep("test-session")
		if !r.state.Exhausted {
			t.Error("expected exhausted=true when sentinel exists")
		}
		if len(logged) == 0 {
			t.Error("expected log message about deep sleep")
		}
	})
}

func TestRunnerResolveSessionID(t *testing.T) {
	dir := t.TempDir()

	t.Run("new session - empty session id", func(t *testing.T) {
		r := NewRunner(dir, "ide", nil)
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
		r := NewRunner(dir, "ide", nil)
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
		r := NewRunner(dir, "ide", nil)
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
	r := NewRunner(dir, "ide", nil)
	r.mu.Lock()
	r.state.CurrentSessionID = "saved-session"
	r.state.Exhausted = true
	r.saveStateLocked()
	r.mu.Unlock()

	r2 := NewRunner(dir, "ide", nil)
	if r2.state.CurrentSessionID != "saved-session" {
		t.Errorf("expected loaded session id='saved-session', got %q", r2.state.CurrentSessionID)
	}
	if !r2.state.Exhausted {
		t.Error("expected loaded exhausted=true")
	}
}

func TestRunnerTickCancelledContext(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	r.tick(ctx)
}

func TestRunnerTickDisabled(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)
	ctx := context.Background()
	r.tick(ctx)
}

func TestRunnerTickAlreadyRunning(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)
	r.mu.Lock()
	r.running = true
	r.mu.Unlock()
	ctx := context.Background()
	r.tick(ctx)
}

func TestRunnerTickNoFiles(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", func() map[string]any {
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

	r := NewRunner(dir, "ide", func() map[string]any {
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
	r := NewRunner(dir, "ide", loader)
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

	r := NewRunner(dir, "ide", loader)
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
	r := NewRunner(dir, "ide", nil)

	modTime := time.Now().Add(-3 * time.Hour)
	first := r.resolveSessionID(modTime)
	second := r.resolveSessionID(modTime)

	if first != second {
		t.Errorf("same mtime must resume the same session: %q then %q", first, second)
	}
}

func TestRunnerRunContextCancel(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := r.Run(ctx)
	if err != nil {
		t.Errorf("Run should return nil on context cancel, got %v", err)
	}
}

func TestRunnerTickStartsDream(t *testing.T) {
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
	r := NewRunner(dir, "ide", loader)
	var logged []string
	r.logFn = func(format string, args ...any) {
		logged = append(logged, format)
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
	_ = r.state.Dreaming
	_ = r.state.LastDreamAt
	r.mu.Unlock()
}

func TestRunnerTickStartsDreamWithMaxDuration(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "old.txt")
	_ = os.WriteFile(filePath, []byte("data"), 0644)
	oldTime := time.Now().Add(-5 * time.Hour)
	_ = os.Chtimes(filePath, oldTime, oldTime)

	loader := func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
			"dream":   map[string]any{"idle_timeout": "1", "max_duration": "1"},
		}
	}
	r := NewRunner(dir, "ide", loader)

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
}

func TestRunnerTickCallsCheckDeepSleep(t *testing.T) {
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
	r := NewRunner(dir, "ide", loader)

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

}

func TestRunnerRunLoop(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

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
	projectDir := "/tmp/project"
	result := buildDreamPrompt(projectDir, "test-session", "vscode", nil)
	if result == "" {
		t.Error("expected non-empty prompt")
	}
	if !strings.Contains(result, "test-session") {
		t.Error("prompt should contain the session id")
	}
	if !strings.Contains(result, projectDir) {
		t.Error("prompt should contain the project dir")
	}
	if !strings.Contains(result, "vscode") {
		t.Error("prompt should contain the IDE name")
	}
	if !strings.Contains(result, ReportsDir(projectDir)) {
		t.Errorf("prompt should contain resolved reports directory %q", ReportsDir(projectDir))
	}
	if strings.Contains(strings.ToLower(result), "backlog") {
		t.Error("dream prompt must not reference the task backlog")
	}
}

func TestBuildDreamContext(t *testing.T) {
	result := buildDreamContext("/tmp/project", "session1", "ide1")
	if !strings.Contains(result, "session1") {
		t.Error("context should contain the session id")
	}
	if !strings.Contains(result, "Phase 1") {
		t.Error("context should contain mission phases")
	}
	if strings.Contains(strings.ToLower(result), "backlog") {
		t.Error("dream context must not reference the task backlog")
	}
}

func TestBuildDreamEnvelope(t *testing.T) {
	projectDir := "/tmp/project"
	result := buildDreamEnvelope(projectDir, "session1")
	if !strings.Contains(result, "session1") {
		t.Error("envelope should contain the session id")
	}
	if !strings.Contains(result, "Dream Report") {
		t.Error("envelope should contain Dream Report section")
	}
	if !strings.Contains(result, "Deep Sleep") {
		t.Error("envelope should contain deep sleep section")
	}
	if strings.Contains(strings.ToLower(result), "backlog") {
		t.Error("dream report envelope must not reference the task backlog")
	}
	if !strings.Contains(result, filepath.Join(ReportsDir(projectDir), "session1"+reportExt)) {
		t.Error("envelope should contain the resolved runtime report path")
	}
}

func TestBuildDreamArtifact(t *testing.T) {
	result := buildDreamArtifact("test-session", "Agent did things.\nMore details.", "")
	if !strings.Contains(result, "test-session") {
		t.Error("artifact should contain the session id")
	}
	if !strings.Contains(result, "Agent did things") {
		t.Error("artifact should contain agent output")
	}
	if !strings.Contains(result, "---") {
		t.Error("artifact should contain frontmatter")
	}
	if !strings.Contains(result, "Dream Report") {
		t.Error("artifact should contain Dream Report header")
	}
}

func TestRunnerStatePath(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)
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

	r := NewRunner(dir, "ide", nil)
	if r.state.CurrentSessionID != "" {
		t.Error("expected empty session id after invalid JSON load")
	}
}

func TestRunnerSaveStateLocked(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

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

func TestExecuteDreamMkdirError(t *testing.T) {
	dir := t.TempDir()
	dreamDir := ReportsDir(dir)
	dreamParent := filepath.Dir(dreamDir)
	_ = os.MkdirAll(dreamParent, 0o755)
	_ = os.WriteFile(dreamDir, []byte("blocker"), 0o644)

	r := NewRunner(dir, "ide", nil)

	err := r.executeDream(context.Background(), "test-session")
	if err == nil {
		t.Error("expected error when MkdirAll fails for dream artifact dir")
	}
	if !strings.Contains(err.Error(), "creating dream artifact dir") {
		t.Errorf("unexpected error message: %v", err)
	}
}

func TestExecuteDreamExecuteLocalError(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := r.executeDream(ctx, "test-session")
	if err == nil {
		t.Error("expected error from executeDream")
	}
	if !strings.Contains(err.Error(), "executing dream agent") && !strings.Contains(err.Error(), "creating") {
		t.Errorf("unexpected error message: %v", err)
	}
}

func TestExecuteLocalCancelledContext(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

	dreamDir := ReportsDir(dir)
	_ = os.MkdirAll(dreamDir, 0o755)

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	result, err := r.executeLocal(ctx, "test prompt", "test-session")
	if err == nil {
		t.Error("expected error from executeLocal")
	}
	if result != "" {
		t.Error("expected empty result on error")
	}
}

func TestExecuteLocalNoAIClientOnPath(t *testing.T) {
	oldPath := os.Getenv("PATH")
	t.Setenv("PATH", "")
	defer os.Setenv("PATH", oldPath)

	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

	dreamDir := ReportsDir(dir)
	_ = os.MkdirAll(dreamDir, 0o755)

	result, err := r.executeLocal(context.Background(), "test prompt", "test-session")
	if err == nil {
		t.Error("expected error when no AI CLI is on PATH")
	}
	if !strings.Contains(err.Error(), "creating AI client") {
		t.Errorf("unexpected error: %v", err)
	}
	if result != "" {
		t.Error("expected empty result on error")
	}
}

func TestExecuteLocalSuccessWithFakeCLI(t *testing.T) {
	fakeBinDir := filepath.Join(t.TempDir(), "bin")
	_ = os.MkdirAll(fakeBinDir, 0o755)
	fakeCLI := filepath.Join(fakeBinDir, "gemini")
	_ = os.WriteFile(fakeCLI, []byte("#!/bin/sh\necho 'Dream report output'\n"), 0o755)

	t.Setenv("PATH", fakeBinDir)

	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

	dreamDir := ReportsDir(dir)
	_ = os.MkdirAll(dreamDir, 0o755)

	result, err := r.executeLocal(context.Background(), "test prompt", "test-session")
	if err != nil {
		t.Fatalf("executeLocal failed unexpectedly: %v", err)
	}
	if result == "" {
		t.Error("expected non-empty result from executeLocal")
	}
}

func TestExecuteDreamSuccessWithFakeCLI(t *testing.T) {
	fakeBinDir := filepath.Join(t.TempDir(), "bin")
	_ = os.MkdirAll(fakeBinDir, 0o755)
	fakeCLI := filepath.Join(fakeBinDir, "gemini")
	_ = os.WriteFile(fakeCLI, []byte("#!/bin/sh\necho 'Dream session complete'\n"), 0o755)

	t.Setenv("PATH", fakeBinDir)

	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)
	var logged []string
	r.logFn = func(format string, args ...any) {
		logged = append(logged, fmt.Sprintf(format, args...))
	}

	err := r.executeDream(context.Background(), "test-session")
	if err != nil {
		t.Fatalf("executeDream failed: %v", err)
	}

	dreamDir := ReportsDir(dir)
	artifactPath := filepath.Join(dreamDir, "test-session.md")
	if !fileExists(artifactPath) {
		t.Error("expected dream artifact file to be created")
	}
}

func TestExecuteDreamWriteFileError(t *testing.T) {
	fakeBinDir := filepath.Join(t.TempDir(), "bin")
	_ = os.MkdirAll(fakeBinDir, 0o755)
	fakeCLI := filepath.Join(fakeBinDir, "gemini")
	_ = os.WriteFile(fakeCLI, []byte("#!/bin/sh\necho 'Dream output'\n"), 0o755)

	t.Setenv("PATH", fakeBinDir)

	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

	dreamDir := ReportsDir(dir)
	_ = os.MkdirAll(dreamDir, 0o755)
	_ = os.Chmod(dreamDir, 0o555)
	defer func() { _ = os.Chmod(dreamDir, 0o755) }()

	err := r.executeDream(context.Background(), "test-session")
	if err == nil {
		t.Error("expected error from WriteFile")
	}
	if !strings.Contains(err.Error(), "writing dream artifact") {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestTickGoroutineSuccessWithFakeCLI(t *testing.T) {
	fakeBinDir := filepath.Join(t.TempDir(), "bin")
	_ = os.MkdirAll(fakeBinDir, 0o755)
	fakeCLI := filepath.Join(fakeBinDir, "gemini")
	_ = os.WriteFile(fakeCLI, []byte("#!/bin/sh\necho 'Dream output'\n"), 0o755)

	t.Setenv("PATH", fakeBinDir)

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
	r := NewRunner(dir, "ide", loader)
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
	r := NewRunner(dir, "ide", loader)
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

func TestTickGoroutineWithMaxDuration(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "old.txt")
	_ = os.WriteFile(filePath, []byte("data"), 0644)
	oldTime := time.Now().Add(-5 * time.Hour)
	_ = os.Chtimes(filePath, oldTime, oldTime)

	loader := func() map[string]any {
		return map[string]any{
			"modules": map[string]any{"dream": "true"},
			"dream":   map[string]any{"idle_timeout": "1", "max_duration": "1"},
		}
	}
	r := NewRunner(dir, "ide", loader)

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
	r := NewRunner(dir, "ide", loader)
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
	if r.state.Exhausted {
		t.Error("expected Exhausted=false without sentinel file")
	}
	r.mu.Unlock()
}

func TestRunnerRunTickerFires(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "file.txt"), []byte("data"), 0644)
	r := NewRunner(dir, "ide", nil)
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
	r := NewRunner(dir, "ide", loader)

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

func TestExecuteDreamWriteArtifactError(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := r.executeDream(ctx, "test-session")
	if err == nil {
		t.Error("expected error")
	}
}

func TestRunWithTickerC(t *testing.T) {
	dir := t.TempDir()
	r := NewRunner(dir, "ide", nil)

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

func TestToollessRunNamesTheLikelyCauseInTheReport(t *testing.T) {
	d := toollessRunDiagnostic(&ai.StreamResult{Binary: "claude", Structured: true})

	for _, want := range []string{"ai.agent_args", "claude", "model call was spent"} {
		if !strings.Contains(d, want) {
			t.Errorf("diagnostic does not mention %q:\n%s", want, d)
		}
	}

	report := buildDreamArtifact("s1", "some prose", d)
	if !strings.Contains(report, "No artifacts were produced") {
		t.Fatalf("diagnostic never reached the report:\n%s", report)
	}
	if strings.Index(report, "ai.agent_args") > strings.Index(report, "## Agent Output") {
		t.Error("the diagnostic sits below the output it is warning about")
	}
}

// The mirror, and the reason this stays a hypothesis: with the setting already in
// place, the report must NOT send someone to fix it. A correctly configured CLI can
// still decide a session needs no tools.
func TestToollessRunDoesNotBlameAConfiguredSetting(t *testing.T) {
	d := toollessRunDiagnostic(&ai.StreamResult{
		Binary: "claude", Structured: true, AgentArgsConfigured: true,
	})

	if !strings.Contains(d, "IS configured") {
		t.Errorf("diagnostic does not acknowledge the setting is present:\n%s", d)
	}
	if strings.Contains(d, "config ai.agent_args.") {
		t.Errorf("it still tells the operator to set what is already set:\n%s", d)
	}
}

// A healthy session must carry no warning at all — the section exists to be rare.
func TestAReportWithNoDiagnosticHasNoWarningSection(t *testing.T) {
	report := buildDreamArtifact("s1", "did real work", "")
	if strings.Contains(report, "No artifacts were produced") {
		t.Errorf("a clean session carries a warning:\n%s", report)
	}
}

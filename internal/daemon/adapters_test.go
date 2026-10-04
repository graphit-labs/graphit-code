package daemon

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/clirun"
)

// recordingRunner replaces the process launcher: it records every invocation and
// answers with the configured result instead of starting a process.
type recordingRunner struct {
	mu     sync.Mutex
	specs  []clirun.Spec
	stdins []string
	calls  chan clirun.Spec
	result func(clirun.Spec) (string, error)
}

func (r *recordingRunner) Run(_ context.Context, spec clirun.Spec) error {
	stdin := ""
	if spec.Stdin != nil {
		data, _ := io.ReadAll(spec.Stdin)
		stdin = string(data)
	}
	var out string
	var err error
	if r.result != nil {
		out, err = r.result(spec)
	}
	if out != "" && spec.Stdout != nil {
		_, _ = io.WriteString(spec.Stdout, out)
	}
	r.mu.Lock()
	r.specs = append(r.specs, spec)
	r.stdins = append(r.stdins, stdin)
	r.mu.Unlock()
	if r.calls != nil {
		r.calls <- spec
	}
	return err
}

func (r *recordingRunner) recorded() ([]clirun.Spec, []string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]clirun.Spec(nil), r.specs...), append([]string(nil), r.stdins...)
}

func useRunner(t *testing.T, r clirun.Runner) {
	t.Helper()
	old := cliRunner
	cliRunner = r
	t.Cleanup(func() { cliRunner = old })
}

func runOneTick(t *testing.T, m WatchModule) clirun.Spec {
	t.Helper()
	runner := &recordingRunner{calls: make(chan clirun.Spec, 4)}
	useRunner(t, runner)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- m.Start(ctx) }()
	var spec clirun.Spec
	select {
	case spec = <-runner.calls:
	case <-time.After(5 * time.Second):
		t.Fatal("module did not run its command")
	}
	cancel()
	if err := <-done; err != nil {
		t.Fatalf("Start after cancel: %v", err)
	}
	return spec
}

func TestScheduledProjectModulesRunTheirCLICommandInTheProject(t *testing.T) {
	project := t.TempDir()
	for _, tc := range []struct {
		module WatchModule
		args   []string
	}{
		{NewEmbeddingModule(project, time.Hour, nil), []string{"ast", "embed", "--if-idle"}},
		{NewWikiEmbeddingModule(project, time.Hour, nil), []string{"wiki", "embed"}},
		{NewMemoryEmbeddingModule(project, time.Hour), []string{"memory", "embed"}},
		{NewMemoryMaintenanceModule(project, time.Hour), []string{"memory", "maintain"}},
		{NewTaskMaintenanceModule(project, time.Hour), []string{"task", "maintain"}},
	} {
		t.Run(tc.module.Name(), func(t *testing.T) {
			spec := runOneTick(t, tc.module)
			if spec.Dir != project {
				t.Errorf("child dir = %q, want the project %q", spec.Dir, project)
			}
			if !reflect.DeepEqual(spec.Args, tc.args) {
				t.Errorf("child args = %v, want %v", spec.Args, tc.args)
			}
			if !spec.Background {
				t.Error("daemon children must run as background work")
			}
		})
	}
	if _, err := os.Stat(filepath.Join(project, brand.DotDir(), brand.RuntimeSubdir(), "daemon", "daemon.log")); err != nil {
		t.Errorf("children must log to the project's daemon log: %v", err)
	}
}

func TestUserMemoryModulesRunWithUserScopeOutsideAnyProject(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	for _, tc := range []struct {
		module WatchModule
		args   []string
	}{
		{NewMemoryEmbeddingModule("", time.Hour), []string{"memory", "embed", "--user"}},
		{NewMemoryMaintenanceModule("", time.Hour), []string{"memory", "maintain", "--user"}},
	} {
		spec := runOneTick(t, tc.module)
		if spec.Dir != brand.GlobalDir() {
			t.Errorf("%s dir = %q, want the global directory", tc.module.Name(), spec.Dir)
		}
		if !reflect.DeepEqual(spec.Args, tc.args) {
			t.Errorf("%s args = %v, want %v", tc.module.Name(), spec.Args, tc.args)
		}
	}
}

// A failed run is logged and retried on the next tick; it must not stop the module.
func TestScheduledModuleKeepsRunningAfterAFailedChild(t *testing.T) {
	runner := &recordingRunner{calls: make(chan clirun.Spec, 4), result: func(clirun.Spec) (string, error) {
		return "", errors.New("exit 1")
	}}
	useRunner(t, runner)
	m := NewTaskMaintenanceModule(t.TempDir(), 10*time.Millisecond)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- m.Start(ctx) }()
	for range 2 {
		select {
		case <-runner.calls:
		case <-time.After(5 * time.Second):
			t.Fatal("module stopped after a failed child")
		}
	}
	cancel()
	if err := <-done; err != nil {
		t.Fatalf("Start after cancel: %v", err)
	}
}

func TestDreamModuleRunsADueSessionAsACLIChild(t *testing.T) {
	runner := &recordingRunner{}
	useRunner(t, runner)
	project := t.TempDir()
	m := NewDreamModule(project, "claude")
	if err := m.runSession(context.Background(), "drm-1"); err != nil {
		t.Fatal(err)
	}
	specs, _ := runner.recorded()
	if len(specs) != 1 || specs[0].Dir != project ||
		!reflect.DeepEqual(specs[0].Args, []string{"dream", "run", "--session", "drm-1", "--agent", "claude"}) {
		t.Fatalf("dream child = %+v", specs)
	}
}

func TestLoadProjectConfigFromDir_NoLockfile(t *testing.T) {
	if cfg := loadProjectConfigFromDir(t.TempDir()); cfg != nil {
		t.Errorf("expected nil config when no lockfile exists, got %v", cfg)
	}
}

func TestLoadProjectConfigFromDir_InvalidLockfile(t *testing.T) {
	tmpDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmpDir, brand.LockFileName()), []byte("{invalid json"), 0o644); err != nil {
		t.Fatal(err)
	}
	if cfg := loadProjectConfigFromDir(tmpDir); cfg != nil {
		t.Errorf("expected nil config for invalid lockfile, got %v", cfg)
	}
}

func TestLoadProjectConfigFromDir_ValidLockfile(t *testing.T) {
	tmpDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmpDir, brand.LockFileName()), []byte(`{"config":{"key":"value"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	cfg := loadProjectConfigFromDir(tmpDir)
	if cfg == nil || cfg["key"] != "value" {
		t.Fatalf("config = %v, want key=value", cfg)
	}
}

// Embedding follows indexing: the trigger runs it at once, and the long timer does not
// add runs of its own before its interval.
func TestEmbeddingRunsWhenTriggeredAndOtherwiseWaitsForItsInterval(t *testing.T) {
	runner := &recordingRunner{calls: make(chan clirun.Spec, 8)}
	useRunner(t, runner)
	trigger := NewTrigger()
	m := NewEmbeddingModule(t.TempDir(), time.Hour, trigger)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- m.Start(ctx) }()

	select {
	case <-runner.calls: // the start-up run
	case <-time.After(5 * time.Second):
		t.Fatal("no start-up run")
	}
	select {
	case spec := <-runner.calls:
		t.Fatalf("ran %v without a trigger before the interval", spec.Args)
	case <-time.After(300 * time.Millisecond):
	}
	trigger.Notify()
	select {
	case <-runner.calls:
	case <-time.After(5 * time.Second):
		t.Fatal("a trigger did not run the embedder")
	}
	cancel()
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}

// Notifications that arrive before the module is free coalesce into one run.
func TestTriggerCoalescesPendingNotifications(t *testing.T) {
	trigger := NewTrigger()
	trigger.Notify()
	trigger.Notify()
	trigger.Notify()
	<-trigger.c()
	select {
	case <-trigger.c():
		t.Fatal("three pending notifications produced more than one wake-up")
	default:
	}
	var none *Trigger
	none.Notify() // a module without a trigger ignores notifications
}

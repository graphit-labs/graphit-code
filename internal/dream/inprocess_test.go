package dream

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
)

func idleRunner(t *testing.T) *Runner {
	t.Helper()
	dir := t.TempDir()
	file := filepath.Join(dir, "old.txt")
	if err := os.WriteFile(file, []byte("old"), 0600); err != nil {
		t.Fatal(err)
	}
	old := time.Now().Add(-5 * time.Hour)
	if err := os.Chtimes(file, old, old); err != nil {
		t.Fatal(err)
	}
	return NewRunner(dir, "codex", func() config.ConfigMap {
		return config.ConfigMap{"modules": map[string]any{"dream": "true"}, "dream": map[string]any{"idle_timeout": "1", "max_duration": "0"}}
	})
}

func TestDreamInProcessShutdownWaitsForSession(t *testing.T) {
	r := idleRunner(t)
	started, canceled, finish := make(chan struct{}), make(chan struct{}), make(chan struct{})
	r.SetExecutor(func(ctx context.Context, _ string) error {
		close(started)
		<-ctx.Done()
		close(canceled)
		<-finish
		return ctx.Err()
	})
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan error, 1)
	go func() { done <- r.Run(ctx) }()
	select {
	case <-started:
	case <-time.After(3 * time.Second):
		t.Fatal("session not started")
	}
	cancel()
	select {
	case <-canceled:
	case <-time.After(3 * time.Second):
		t.Fatal("session not canceled")
	}
	select {
	case <-done:
		t.Fatal("Run returned before session cleanup")
	case <-time.After(20 * time.Millisecond):
	}
	close(finish)
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("Run did not join")
	}
	if r.IsRunning() {
		t.Fatal("active session state leaked")
	}
}

func TestDreamInProcessPanicCleansState(t *testing.T) {
	r := idleRunner(t)
	r.SetExecutor(func(context.Context, string) error { panic("test panic") })
	ctx, cancel := context.WithCancel(context.Background())
	r.tick(ctx)
	r.active.Wait()
	cancel()
	if r.IsRunning() {
		t.Fatal("panic leaked running state")
	}
}

type callbackDreamClient struct {
	dir string
	req ai.StreamRequest
}

func (*callbackDreamClient) SupportsStructuredStream() bool { return true }

func (*callbackDreamClient) Complete(context.Context, string, string) (string, error) { return "", nil }
func (c *callbackDreamClient) CompleteStream(ctx context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
	c.req = req
	wait, cancel := context.WithTimeout(ctx, time.Second)
	defer cancel()
	release, err := projectversion.AcquireContext(wait, c.dir, "")
	if err != nil {
		return nil, err
	}
	release()
	emit(ai.Event{Kind: ai.EventToolUse, Tool: "graphit_memory_list", ToolCallID: "read"})
	return &ai.StreamResult{Structured: true}, nil
}

func TestDreamInProcessVersionReleasesScopeBeforeAgentCallback(t *testing.T) {
	dir := t.TempDir()
	r := newSessionRunner(dir, "codex")
	ledger, client := &fakeRunLedger{}, &callbackDreamClient{dir: dir}
	r.newLedger = func(context.Context, string) (runLedgerWriter, error) {
		if got := projectversion.Current(dir).Version; got != "selected" {
			t.Errorf("ledger version = %q", got)
		}
		return ledger, nil
	}
	r.newClient = func() (ai.Client, error) { return client, nil }
	r.scopeLedgerVersion("selected")
	if err := r.executeDream(context.Background(), "drm-test"); err != nil {
		t.Fatal(err)
	}
	if client.req.Env[projectversion.EnvVersion()] != "selected" {
		t.Fatal("agent version missing")
	}
	if projectversion.Current(dir).Version != projectversion.DefaultName {
		t.Fatal("version scope leaked")
	}
}

package ai

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestConversationObserverStreamsBeforeCompletion(t *testing.T) {
	dir := t.TempDir()
	release := filepath.Join(dir, "release")
	script := fmt.Sprintf("#!/bin/sh\nprintf 'diagnostic\\n'\nprintf 'warning\\n' >&2\nwhile [ ! -f %q ]; do sleep 0.01; done\nprintf '%%s\\n' '{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"answer\"}}'\npwd > cwd\n", release)
	cli := &cliClient{executablePath: writeFakeCLI(t, "codex", script), binaryName: "codex"}
	events := make(chan Event, 20)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	done := make(chan error, 1)
	go func() {
		answer, err := NewConversation(cli, dir).Complete(WithEventObserver(ctx, func(e Event) { events <- e }), "", "question")
		if err == nil && answer != "answer" {
			err = fmt.Errorf("answer=%q", answer)
		}
		done <- err
	}()
	seen := map[EventKind]bool{}
	for !seen[EventStdout] || !seen[EventStderr] {
		select {
		case e := <-events:
			seen[e.Kind] = true
		case err := <-done:
			t.Fatalf("finished before release: %v", err)
		case <-ctx.Done():
			t.Fatal(ctx.Err())
		}
	}
	if err := os.WriteFile(release, nil, 0600); err != nil {
		t.Fatal(err)
	}
	if err := <-done; err != nil {
		t.Fatal(err)
	}
	cwd, err := os.ReadFile(filepath.Join(dir, "cwd"))
	if err != nil || strings.TrimSpace(string(cwd)) != dir {
		t.Fatalf("workdir lost: %s %v", cwd, err)
	}
}

func TestConversationObserverCancellation(t *testing.T) {
	cli := &cliClient{executablePath: writeFakeCLI(t, "codex", "#!/bin/sh\nprintf 'waiting\\n'\nsleep 30\n"), binaryName: "codex"}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	start := time.Now()
	_, err := NewConversation(cli, t.TempDir()).Complete(WithEventObserver(ctx, func(e Event) {
		if e.Kind == EventStdout {
			cancel()
		}
	}), "", "question")
	if err == nil || time.Since(start) > 5*time.Second {
		t.Fatalf("cancellation failed: %v", err)
	}
}

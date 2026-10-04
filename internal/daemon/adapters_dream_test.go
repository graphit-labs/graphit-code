package daemon

import (
	"context"
	"testing"
	"time"
)

func TestDreamModule_Start_ReturnsOnCancel(t *testing.T) {
	useRunner(t, &recordingRunner{})
	m := NewDreamModule(t.TempDir(), "vscode")
	ctx, cancel := context.WithCancel(context.Background())
	errCh := make(chan error, 1)
	go func() { errCh <- m.Start(ctx) }()
	time.Sleep(100 * time.Millisecond)
	cancel()
	select {
	case <-errCh:
	case <-time.After(5 * time.Second):
		t.Fatal("DreamModule.Start did not return after cancellation")
	}
}

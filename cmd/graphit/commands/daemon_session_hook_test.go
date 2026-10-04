package commands

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"net/url"
	"path/filepath"
	"strconv"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/sessioncontext"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

func sessionHookTestDaemon(t *testing.T, key string) int {
	t.Helper()
	server := httptest.NewServer(newDaemonMCPMux(http.NotFoundHandler(), daemonMCPMuxOptions{RuntimeKey: key}))
	t.Cleanup(server.Close)
	u, err := url.Parse(server.URL)
	if err != nil {
		t.Fatal(err)
	}
	port, err := strconv.Atoi(u.Port())
	if err != nil {
		t.Fatal(err)
	}
	return port
}

// The hook binary renders events itself and asks the daemon only for the store-backed
// half; the payload must be exactly what the core command renders for the same event.
func TestSessionHookEndpointMatchesTheCoreRendering(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	projectDir := t.TempDir()
	if err := hub.SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), &hub.Lockfile{}); err != nil {
		t.Fatal(err)
	}
	port := sessionHookTestDaemon(t, "runtime-key")
	input := []byte(`{"cwd":"` + filepath.ToSlash(projectDir) + `","session_id":"s1"}`)

	remote := func(ctx context.Context, req sessionhook.StoreRequest) (sessionhook.StoreResult, error) {
		return sessionhook.ApplyRemote(ctx, port, "runtime-key", req)
	}
	for _, format := range []string{
		sessionhook.FormatSessionStart, sessionhook.FormatAdditionalContext,
		sessionhook.FormatPostToolUse, sessionhook.FormatStop, sessionhook.FormatToolContext,
	} {
		t.Run(format, func(t *testing.T) {
			want, err := renderSessionHook(context.Background(), projectDir, format, input)
			if err != nil {
				t.Fatalf("core render: %v", err)
			}
			got, err := sessioncontext.Render(context.Background(), projectDir, format, input, remote)
			if err != nil {
				t.Fatalf("daemon render: %v", err)
			}
			if string(got) != string(want) {
				t.Fatalf("daemon payload differs from the core\ncore:   %s\ndaemon: %s", want, got)
			}
		})
	}
}

// Only a process that can read the daemon's runtime key may drive Task lifecycle
// effects; anything else is refused and the hook falls back to the core.
func TestSessionHookEndpointRequiresTheRuntimeKey(t *testing.T) {
	port := sessionHookTestDaemon(t, "runtime-key")
	_, err := sessionhook.ApplyRemote(context.Background(), port, "wrong-key",
		sessionhook.StoreRequest{Format: sessionhook.FormatStop, Task: true})
	if !errors.Is(err, sessionhook.ErrDaemonUnavailable) {
		t.Fatalf("wrong key: err = %v, want a refusal the caller treats as unavailable", err)
	}
}

// The request names the project; the daemon accepts only a real project root, and
// says so as a result rather than as an outage.
func TestSessionHookEndpointRejectsADirectoryThatIsNotAProject(t *testing.T) {
	port := sessionHookTestDaemon(t, "runtime-key")
	_, err := sessionhook.ApplyRemote(context.Background(), port, "runtime-key",
		sessionhook.StoreRequest{Format: sessionhook.FormatStop, ProjectDir: t.TempDir(), Task: true})
	if err == nil || errors.Is(err, sessionhook.ErrDaemonUnavailable) {
		t.Fatalf("non-project dir: err = %v, want a reported rejection", err)
	}
}

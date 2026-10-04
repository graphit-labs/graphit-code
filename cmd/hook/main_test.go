package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/sessioncontext"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

const fakeCoreEnv = "GRAPHIT_HOOK_TEST_FAKE_CORE"

// TestMain doubles as the core binary for the fallback path.
func TestMain(m *testing.M) {
	if os.Getenv(fakeCoreEnv) == "1" {
		in, _ := io.ReadAll(os.Stdin)
		fmt.Printf("core:%s:%s", strings.Join(os.Args[1:], " "), in)
		os.Exit(0)
	}
	os.Exit(m.Run())
}

func useFakeCore(t *testing.T) *bool {
	t.Helper()
	called := new(bool)
	old := coreBinary
	coreBinary = func() (string, error) {
		*called = true
		return os.Executable()
	}
	t.Setenv(fakeCoreEnv, "1")
	t.Cleanup(func() { coreBinary = old })
	return called
}

func project(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	return dir
}

// fakeDaemon answers the store endpoint and publishes its port and key the way the
// daemon does.
func fakeDaemon(t *testing.T, respond func(sessionhook.StoreRequest) sessionhook.StoreResponse) {
	t.Helper()
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != sessionhook.DaemonPath || r.Header.Get("Authorization") != "Bearer key" {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		var req sessionhook.StoreRequest
		_ = json.NewDecoder(r.Body).Decode(&req)
		_ = json.NewEncoder(w).Encode(respond(req))
	}))
	t.Cleanup(server.Close)
	u, _ := url.Parse(server.URL)
	if err := os.MkdirAll(filepath.Dir(daemonctl.PortFilePath()), 0o755); err != nil {
		t.Fatal(err)
	}
	_ = os.WriteFile(daemonctl.PortFilePath(), []byte(u.Port()), 0o644)
	_ = os.WriteFile(daemonctl.KeyFilePath(), []byte("key"), 0o600)
}

// The hook renders the payload itself; the daemon only applies the Task effect and
// hands back mandatory memory, which lands in the payload exactly as the core puts it.
func TestHookRendersLocallyWithTheDaemonStoreResult(t *testing.T) {
	dir := project(t)
	stored := sessionhook.StoreResult{Mandatory: "### project memory: Policy\nkeep it", MandatoryLoaded: true}
	var seen sessionhook.StoreRequest
	fakeDaemon(t, func(req sessionhook.StoreRequest) sessionhook.StoreResponse {
		seen = req
		return sessionhook.StoreResponse{StoreResult: stored}
	})
	called := useFakeCore(t)
	input := `{"cwd":"` + filepath.ToSlash(dir) + `"}`
	var out, errOut bytes.Buffer
	code := run([]string{"--format", sessionhook.FormatSessionStart}, strings.NewReader(input), &out, &errOut)

	want, err := sessioncontext.Render(context.Background(), dir, sessionhook.FormatSessionStart, []byte(input),
		func(context.Context, sessionhook.StoreRequest) (sessionhook.StoreResult, error) { return stored, nil })
	if err != nil {
		t.Fatal(err)
	}
	if code != 0 || out.String() != string(want)+"\n" || !strings.Contains(out.String(), "keep it") {
		t.Fatalf("code=%d out=%q want %q err=%q", code, out.String(), want, errOut.String())
	}
	if *called {
		t.Fatal("the core ran although the daemon answered")
	}
	if seen.ProjectDir != dir || !seen.Task || !seen.Mandatory || seen.Format != sessionhook.FormatSessionStart {
		t.Fatalf("daemon request = %+v, want the Task effect and mandatory memory for %s", seen, dir)
	}
}

// A tool-call event needs only the Task heartbeat; mandatory memory is never asked for.
func TestHookAsksTheDaemonOnlyForTheTaskEffectOnToolCalls(t *testing.T) {
	dir := project(t)
	var seen sessionhook.StoreRequest
	fakeDaemon(t, func(req sessionhook.StoreRequest) sessionhook.StoreResponse {
		seen = req
		return sessionhook.StoreResponse{}
	})
	useFakeCore(t)
	var out, errOut bytes.Buffer
	code := run([]string{"--format", sessionhook.FormatPostToolUse, "--project-dir", dir}, strings.NewReader("{}"), &out, &errOut)
	if code != 0 || !seen.Task || seen.Mandatory {
		t.Fatalf("code=%d request=%+v err=%q", code, seen, errOut.String())
	}
}

// With Task and Memory off nothing needs a store: the hook answers on its own, with no
// daemon and no core.
func TestHookNeedsNoDaemonWhenNoStoreIsInvolved(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	dir := t.TempDir()
	lock := `{"config":{"modules":{"task":"false","memory":"false"}}}`
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(lock), 0o644); err != nil {
		t.Fatal(err)
	}
	called := useFakeCore(t)
	var out, errOut bytes.Buffer
	code := run([]string{"--format", sessionhook.FormatSessionStart, "--project-dir", dir}, strings.NewReader("{}"), &out, &errOut)
	if code != 0 || *called || out.Len() == 0 || strings.HasPrefix(out.String(), "core:") {
		t.Fatalf("code=%d coreCalled=%v out=%q err=%q", code, *called, out.String(), errOut.String())
	}
}

// An error the daemon reports is the event's result: the Task effect already happened,
// so the core must not run it a second time.
func TestHookReportsDaemonErrorsWithoutRetryingThroughTheCore(t *testing.T) {
	dir := project(t)
	fakeDaemon(t, func(sessionhook.StoreRequest) sessionhook.StoreResponse {
		return sessionhook.StoreResponse{Error: "opening task lifecycle store: boom"}
	})
	called := useFakeCore(t)
	var out, errOut bytes.Buffer
	code := run([]string{"--format", "stop", "--project-dir", dir}, strings.NewReader("{}"), &out, &errOut)
	if code == 0 || !strings.Contains(errOut.String(), "boom") || *called {
		t.Fatalf("code=%d err=%q coreCalled=%v", code, errOut.String(), *called)
	}
}

func TestHookFallsBackToTheCoreWithoutADaemon(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	dir := project(t)
	useFakeCore(t)
	var out, errOut bytes.Buffer
	code := run([]string{"--format", "stop", "--project-dir", dir}, strings.NewReader(`{"session_id":"s"}`), &out, &errOut)
	want := fmt.Sprintf(`core:_session-hook --format stop --project-dir %s:{"session_id":"s"}`, dir)
	if code != 0 || out.String() != want {
		t.Fatalf("code=%d out=%q want %q err=%q", code, out.String(), want, errOut.String())
	}
}

func TestHookFinalSyncRequiresAResolvedProject(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	var out, errOut bytes.Buffer
	code := run([]string{"--format", "stop", "--sync", "--project-dir", filepath.Join(t.TempDir(), "missing")}, strings.NewReader("{}"), &out, &errOut)
	if code == 0 || !strings.Contains(errOut.String(), "lockfile") || out.Len() != 0 {
		t.Fatalf("code=%d out=%q err=%q", code, out.String(), errOut.String())
	}
}

package commands

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
)

func TestDaemonUIStartRequiresRuntimeKey(t *testing.T) {
	called := 0
	mux := newDaemonMCPMux(http.NotFoundHandler(), daemonMCPMuxOptions{RuntimeKey: "runtime", EnsureUI: func(context.Context) (string, error) { called++; return "http://127.0.0.1:8080/", nil }})
	for _, token := range []string{"", "static-key", "broker-token", "runtime"} {
		r := httptest.NewRequest(http.MethodPost, daemonctl.UIStartPath, nil)
		if token != "" {
			r.Header.Set("Authorization", "Bearer "+token)
		}
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, r)
		want := http.StatusUnauthorized
		if token == "runtime" {
			want = http.StatusOK
		}
		if w.Code != want {
			t.Fatalf("token=%q status=%d", token, w.Code)
		}
		if token == "runtime" && !strings.Contains(w.Body.String(), `"url":"http://127.0.0.1:8080/"`) {
			t.Fatal(w.Body.String())
		}
	}
	if called != 1 {
		t.Fatalf("unauthorized activation: %d", called)
	}
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, httptest.NewRequest(http.MethodGet, daemonctl.UIStartPath, nil))
	if w.Code != http.StatusMethodNotAllowed {
		t.Fatalf("GET status=%d", w.Code)
	}
}

func TestDaemonUIStartFailure(t *testing.T) {
	h := &daemonUIHandler{runtimeKey: "runtime", ensure: func(context.Context) (string, error) { return "", errors.New("private failure detail") }}
	r := httptest.NewRequest(http.MethodPost, daemonctl.UIStartPath, nil)
	r.Header.Set("Authorization", "Bearer runtime")
	w := httptest.NewRecorder()
	h.ServeHTTP(w, r)
	if w.Code != http.StatusServiceUnavailable || strings.Contains(w.Body.String(), "private failure") {
		t.Fatalf("response=%d %s", w.Code, w.Body.String())
	}
}

func TestDaemonUIGlobalBackendDoesNotOpenProject(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	db := daemonUIASTBackend(brand.GlobalDir())
	defer func() { _ = db.Close() }()
	if db.BackendType() != "empty" || daemonUIProjectName(brand.GlobalDir()) != "" {
		t.Fatal("global UI selected a project")
	}
}

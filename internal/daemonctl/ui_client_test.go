package daemonctl

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func uiClientPIDLock(t *testing.T) {
	t.Helper()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	if err := os.MkdirAll(DaemonDir(), 0o700); err != nil {
		t.Fatal(err)
	}
	pidFile, err := os.OpenFile(PIDFilePath(), os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = pidFile.Close() })
	// Use the daemon's lock range; the generic lock covers the PID on Windows.
	if err := flockProbe(pidFile); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { flockProbeRelease(pidFile) })
	if _, err := pidFile.WriteString(strconv.Itoa(os.Getpid()) + "\n"); err != nil {
		t.Fatal(err)
	}
	if !IsRunning() {
		t.Fatal("daemon fixture PID lock is not visible to status probes")
	}
}

func uiClientDaemon(t *testing.T, handler http.HandlerFunc) {
	t.Helper()
	uiClientPIDLock(t)
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)
	port := strings.Split(srv.Listener.Addr().String(), ":")
	if err := os.WriteFile(PortFilePath(), []byte(port[len(port)-1]), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(KeyFilePath(), []byte("runtime"), 0600); err != nil {
		t.Fatal(err)
	}
}

func TestUIClientActivatesWithLocalRuntimeKey(t *testing.T) {
	uiClientDaemon(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != UIStartPath || r.Header.Get("Authorization") != "Bearer runtime" {
			t.Error("wrong control request")
			w.WriteHeader(401)
			return
		}
		_, _ = w.Write([]byte(`{"url":"http://127.0.0.1:8080/"}`))
	})
	t.Setenv("HTTP_PROXY", "http://127.0.0.1:1")
	address, err := RequestUIURL(context.Background())
	if err != nil || address != "http://127.0.0.1:8080/" {
		t.Fatalf("URL=%q err=%v", address, err)
	}
}

func TestUIClientReusesPublishedURL(t *testing.T) {
	uiClientDaemon(t, func(http.ResponseWriter, *http.Request) { t.Error("already-ready UI requested again") })
	clear, err := PublishUIURL("http://127.0.0.1:9090/")
	if err != nil {
		t.Fatal(err)
	}
	defer clear()
	address, err := RequestUIURL(context.Background())
	if err != nil || address != "http://127.0.0.1:9090/" {
		t.Fatalf("URL=%q err=%v", address, err)
	}
}

func TestUIClientRejectsFailedOrInvalidResponse(t *testing.T) {
	for _, tc := range []struct {
		name, body string
		status     int
		wantError  string
	}{
		{"unauthorized", "", 401, "returned HTTP 401"}, {"unavailable", "", 503, "returned HTTP 503"}, {"redirect", "", 302, "returned HTTP 302"},
		{"bad-json", "no", 200, "invalid daemon UI response"}, {"bad-url", `{"url":"https://example.com/"}`, 200, "invalid daemon UI address"},
		{"credential-url", `{"url":"http://user:pass@127.0.0.1:8080/"}`, 200, "invalid daemon UI address"},
		{"oversized", strings.Repeat(" ", 4097) + `{"url":"http://127.0.0.1:8080/"}`, 200, "invalid daemon UI response"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var reached atomic.Bool
			uiClientDaemon(t, func(w http.ResponseWriter, _ *http.Request) {
				reached.Store(true)
				w.Header().Set("Location", "http://127.0.0.1:1/")
				w.WriteHeader(tc.status)
				_, _ = w.Write([]byte(tc.body))
			})
			if _, err := RequestUIURL(context.Background()); err == nil || !strings.Contains(err.Error(), tc.wantError) {
				t.Fatalf("response error=%v, want %q", err, tc.wantError)
			}
			if !reached.Load() {
				t.Fatal("response validation did not reach daemon control endpoint")
			}
		})
	}
}

func TestUIClientStoppedDaemonAndMissingDiscovery(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	if _, err := RequestUIURL(context.Background()); err == nil {
		t.Fatal("stopped daemon accepted")
	}
	uiClientPIDLock(t)
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
	defer cancel()
	if _, err := RequestUIURL(ctx); !errors.Is(err, context.DeadlineExceeded) || !strings.Contains(err.Error(), "waiting for daemon control endpoint") {
		t.Fatalf("missing discovery error=%v", err)
	}
}

func TestUIClientRejectsInvalidPortOrKey(t *testing.T) {
	uiClientDaemon(t, func(http.ResponseWriter, *http.Request) { t.Error("invalid discovery reached server") })
	port, err := os.ReadFile(PortFilePath())
	if err != nil {
		t.Fatal(err)
	}
	for _, p := range []string{"0", "-1", "65536", "invalid"} {
		if err := os.WriteFile(PortFilePath(), []byte(p), 0600); err != nil {
			t.Fatal(err)
		}
		if _, err := RequestUIURL(context.Background()); err == nil || !strings.Contains(err.Error(), "invalid daemon control port") {
			t.Fatalf("invalid port error=%v", err)
		}
	}
	if err := os.WriteFile(PortFilePath(), port, 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(KeyFilePath(), nil, 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := RequestUIURL(context.Background()); err == nil || !strings.Contains(err.Error(), "daemon runtime key is unavailable") {
		t.Fatalf("empty key error=%v", err)
	}
}

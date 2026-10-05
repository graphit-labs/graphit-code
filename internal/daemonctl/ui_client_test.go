package daemonctl

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

func uiClientDaemon(t *testing.T, handler http.HandlerFunc) {
	t.Helper()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	lock, err := lockfile.TryAcquire(PIDFilePath())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { lock.Release() })
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
	if err := os.WriteFile(PIDFilePath(), []byte(strconv.Itoa(os.Getpid())), 0600); err != nil {
		t.Fatal(err)
	}
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
	}{
		{"unauthorized", "", 401}, {"unavailable", "", 503}, {"redirect", "", 302},
		{"bad-json", "no", 200}, {"bad-url", `{"url":"https://example.com/"}`, 200},
		{"credential-url", `{"url":"http://user:pass@127.0.0.1:8080/"}`, 200},
		{"oversized", strings.Repeat(" ", 4097) + `{"url":"http://127.0.0.1:8080/"}`, 200},
	} {
		t.Run(tc.name, func(t *testing.T) {
			uiClientDaemon(t, func(w http.ResponseWriter, _ *http.Request) {
				w.Header().Set("Location", "http://127.0.0.1:1/")
				w.WriteHeader(tc.status)
				_, _ = w.Write([]byte(tc.body))
			})
			if _, err := RequestUIURL(context.Background()); err == nil {
				t.Fatal("accepted invalid response")
			}
		})
	}
}

func TestUIClientStoppedDaemonAndMissingDiscovery(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	if _, err := RequestUIURL(context.Background()); err == nil {
		t.Fatal("stopped daemon accepted")
	}
	lock, err := lockfile.TryAcquire(PIDFilePath())
	if err != nil {
		t.Fatal(err)
	}
	defer func() { lock.Release() }()
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
	defer cancel()
	if _, err := RequestUIURL(ctx); !errors.Is(err, context.DeadlineExceeded) {
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
		if _, err := RequestUIURL(context.Background()); err == nil {
			t.Fatal("invalid port accepted")
		}
	}
	if err := os.WriteFile(PortFilePath(), port, 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(KeyFilePath(), nil, 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := RequestUIURL(context.Background()); err == nil {
		t.Fatal("empty key accepted")
	}
}

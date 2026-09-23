package tray

import (
	"errors"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

func TestDescribeDaemonStates(t *testing.T) {
	service := daemonservice.Status{Installed: true, Active: true, Manager: "test"}
	for _, tc := range []struct {
		name, want  string
		service     daemonservice.Status
		running     bool
		start, stop bool
	}{
		{"running", "Daemon running", service, true, false, true},
		{"starting", "Daemon starting or restarting", service, false, true, true},
		{"stopped", "Daemon stopped", daemonservice.Status{Installed: true}, false, true, false},
		{"not installed", "Daemon stopped; service not installed", daemonservice.Status{}, false, true, false},
		{"foreground", "Daemon running in foreground", daemonservice.Status{}, true, false, true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got := describe(tc.service, tc.running)
			if got.state != tc.want || got.canStart != tc.start || got.canStop != tc.stop {
				t.Fatalf("state=%q start=%t stop=%t", got.state, got.canStart, got.canStop)
			}
		})
	}
}

func TestUIURLUsesAddressPublishedByCurrentDaemon(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	lock, err := lockfile.TryAcquire(daemonctl.PIDFilePath())
	if err != nil {
		t.Fatal(err)
	}
	defer lock.Release()
	clear, err := daemonctl.PublishUIURL("http://127.0.0.1:9091/")
	if err != nil {
		t.Fatal(err)
	}
	defer clear()
	if got := UIURL(); got != "http://127.0.0.1:9091/" {
		t.Fatalf("URL = %q", got)
	}
}

func TestOpenDaemonUIOnlyPassesPublishedURLToBrowser(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	lock, err := lockfile.TryAcquire(daemonctl.PIDFilePath())
	if err != nil {
		t.Fatal(err)
	}
	defer lock.Release()
	clear, err := daemonctl.PublishUIURL("http://127.0.0.1:9091/")
	if err != nil {
		t.Fatal(err)
	}
	defer clear()
	called := 0
	err = openDaemonUI(func(url string) error {
		called++
		if url != "http://127.0.0.1:9091/" {
			t.Fatalf("browser URL = %q", url)
		}
		return nil
	})
	if err != nil || called != 1 {
		t.Fatalf("open error = %v, browser calls = %d", err, called)
	}
	clear()
	if err := openDaemonUI(func(string) error { called++; return nil }); err == nil || called != 1 {
		t.Fatalf("unavailable UI error = %v, browser calls = %d", err, called)
	}
}

func TestStopAndQuitWaitsForServiceStop(t *testing.T) {
	var calls []string
	if err := stopAndQuit(func() error { calls = append(calls, "stop"); return nil }, func() { calls = append(calls, "quit") }); err != nil {
		t.Fatal(err)
	}
	if got := strings.Join(calls, ","); got != "stop,quit" {
		t.Fatalf("actions = %q", got)
	}
	want := errors.New("manager unavailable")
	calls = nil
	if err := stopAndQuit(func() error { calls = append(calls, "stop"); return want }, func() { calls = append(calls, "quit") }); !errors.Is(err, want) {
		t.Fatalf("stop error = %v", err)
	}
	if got := strings.Join(calls, ","); got != "stop" {
		t.Fatalf("actions after failed stop = %q", got)
	}
}

//go:build linux

package tray

import (
	"strings"
	"testing"
)

func TestDesktopEnvironmentRecoversOnlySessionAddresses(t *testing.T) {
	base := []string{"HOME=/home/test", "DISPLAY=", "GRAPHIT_LAUNCHER_PATH=/graphit"}
	manager := "DISPLAY=:1\nWAYLAND_DISPLAY=wayland-1\nDBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus\nXDG_RUNTIME_DIR=/run/user/1000\nSECRET_KEY=do-not-copy\n"
	env := desktopEnvironment(base, manager)
	if !hasGraphicalEnvironment(env) {
		t.Fatalf("recovered environment is not graphical: %q", env)
	}
	joined := strings.Join(env, "\n")
	for _, want := range []string{"DISPLAY=:1", "WAYLAND_DISPLAY=wayland-1", "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus", "GRAPHIT_LAUNCHER_PATH=/graphit"} {
		if !strings.Contains(joined, want) {
			t.Fatalf("missing %q in %q", want, joined)
		}
	}
	if strings.Contains(joined, "SECRET_KEY") {
		t.Fatalf("copied unrelated manager variable: %q", env)
	}
}

func TestDesktopEnvironmentKeepsHeadlessSessionHeadless(t *testing.T) {
	env := desktopEnvironment([]string{"HOME=/home/test"}, "XDG_RUNTIME_DIR=/run/user/1000\nDBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus\n")
	if hasGraphicalEnvironment(env) {
		t.Fatalf("headless session considered graphical: %q", env)
	}
}

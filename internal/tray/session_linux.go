//go:build linux

package tray

import (
	"context"
	"os"
	"os/exec"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/daemonservice"
)

// graphicalEnvironment recovers only desktop addressing variables from the
// current user's systemd manager. Agent hosts often omit them from MCP env.
func graphicalEnvironment() ([]string, bool) {
	base := os.Environ()
	if hasGraphicalEnvironment(base) {
		return base, true
	}
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, "systemctl", "--user", "show-environment")
	cmd.Env = daemonservice.UserManagerEnvironment()
	output, err := cmd.Output()
	if err != nil {
		return nil, false
	}
	env := desktopEnvironment(base, string(output))
	return env, hasGraphicalEnvironment(env)
}

func hasGraphicalEnvironment(env []string) bool {
	values := make(map[string]string, 3)
	for _, entry := range env {
		key, value, ok := strings.Cut(entry, "=")
		if ok && (key == "DISPLAY" || key == "WAYLAND_DISPLAY" || key == "DBUS_SESSION_BUS_ADDRESS") {
			values[key] = value
		}
	}
	return (values["DISPLAY"] != "" || values["WAYLAND_DISPLAY"] != "") && values["DBUS_SESSION_BUS_ADDRESS"] != ""
}

func desktopEnvironment(base []string, manager string) []string {
	allowed := map[string]bool{
		"DISPLAY": true, "WAYLAND_DISPLAY": true, "DBUS_SESSION_BUS_ADDRESS": true,
		"XDG_RUNTIME_DIR": true, "XAUTHORITY": true, "XDG_SESSION_TYPE": true,
	}
	existing := make(map[string]string)
	for _, entry := range base {
		key, value, ok := strings.Cut(entry, "=")
		if ok {
			existing[key] = value
		}
	}
	env := append([]string(nil), base...)
	for _, line := range strings.Split(manager, "\n") {
		key, value, ok := strings.Cut(line, "=")
		if ok && allowed[key] && existing[key] == "" && value != "" {
			env = append(env, key+"="+value)
			existing[key] = value
		}
	}
	return env
}

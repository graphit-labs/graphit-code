//go:build linux

package daemonservice

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

var systemctlRun commandRunner = runSystemctl

// Agent hosts may start the MCP proxy with a minimal environment that omits
// desktop session variables. The user manager still listens on this user's
// runtime bus, so give systemctl the standard addresses when they are absent.
func runSystemctl(name string, args ...string) error {
	cmd := exec.Command(name, args...)
	cmd.Env = UserManagerEnvironment()
	if output, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("%s %v: %w: %s", name, args, err, output)
	}
	return nil
}

// UserManagerEnvironment provides the current user's systemd bus address when
// an agent host starts a subprocess without desktop session variables.
func UserManagerEnvironment() []string {
	return systemctlEnvironment(os.Environ(), os.Getuid())
}

func systemctlEnvironment(base []string, uid int) []string {
	env := append([]string(nil), base...)
	values := make(map[string]string, 2)
	for _, entry := range base {
		key, value, ok := strings.Cut(entry, "=")
		if ok && (key == "XDG_RUNTIME_DIR" || key == "DBUS_SESSION_BUS_ADDRESS") {
			values[key] = value
		}
	}
	runtimeDir := values["XDG_RUNTIME_DIR"]
	if runtimeDir == "" {
		runtimeDir = fmt.Sprintf("/run/user/%d", uid)
		env = append(env, "XDG_RUNTIME_DIR="+runtimeDir)
	}
	if values["DBUS_SESSION_BUS_ADDRESS"] == "" {
		env = append(env, "DBUS_SESSION_BUS_ADDRESS=unix:path="+filepath.Join(runtimeDir, "bus"))
	}
	return env
}

func unitName() string { return serviceName() + ".service" }

func unitPath() (string, error) {
	config := os.Getenv("XDG_CONFIG_HOME")
	if config == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		config = filepath.Join(home, ".config")
	}
	return filepath.Join(config, "systemd", "user", unitName()), nil
}

// systemd expands percent specifiers even inside quoted paths.
func systemdExecPath(path string) string {
	path = strings.ReplaceAll(path, "%", "%%")
	path = strings.ReplaceAll(path, `\`, `\\`)
	path = strings.ReplaceAll(path, `"`, `\"`)
	return `"` + path + `"`
}

func linuxUnit(exe string) string {
	return "[Unit]\nDescription=Graphit daemon\nStartLimitIntervalSec=0\n\n[Service]\nType=simple\nExecStart=" + systemdExecPath(exe) +
		" daemon --managed\nRestart=on-failure\nRestartSec=2s\n\n[Install]\nWantedBy=default.target\n"
}

func install(autoStart bool) error {
	exe, err := ResolveExecutable()
	if err != nil {
		return err
	}
	path, err := unitPath()
	if err != nil {
		return err
	}
	previous, previousErr := os.ReadFile(path)
	if previousErr != nil && !os.IsNotExist(previousErr) {
		return previousErr
	}
	wasEnabled := false
	if previousErr == nil {
		wasEnabled = systemctlRun("systemctl", "--user", "is-enabled", "--quiet", unitName()) == nil
	}
	rollback := func(cause error) error {
		var rollbackErrors []error
		if os.IsNotExist(previousErr) {
			if err := systemctlRun("systemctl", "--user", "disable", unitName()); err != nil {
				rollbackErrors = append(rollbackErrors, err)
			}
			if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
				rollbackErrors = append(rollbackErrors, err)
			}
		} else {
			if err := writeFileAtomic(path, previous); err != nil {
				rollbackErrors = append(rollbackErrors, err)
			}
		}
		if err := systemctlRun("systemctl", "--user", "daemon-reload"); err != nil {
			rollbackErrors = append(rollbackErrors, err)
		}
		if previousErr == nil {
			verb := "disable"
			if wasEnabled {
				verb = "enable"
			}
			if err := systemctlRun("systemctl", "--user", verb, unitName()); err != nil {
				rollbackErrors = append(rollbackErrors, err)
			}
		}
		return errors.Join(append([]error{cause}, rollbackErrors...)...)
	}
	if err := writeFileAtomic(path, []byte(linuxUnit(exe))); err != nil {
		return err
	}
	if err := systemctlRun("systemctl", "--user", "daemon-reload"); err != nil {
		return rollback(err)
	}
	if autoStart {
		if err := systemctlRun("systemctl", "--user", "enable", unitName()); err != nil {
			return rollback(err)
		}
	} else {
		// A previously enabled service must not keep starting at login.
		if err := systemctlRun("systemctl", "--user", "disable", unitName()); err != nil {
			return rollback(err)
		}
	}
	if err := removeLegacyCron(); err != nil {
		return rollback(err)
	}
	return nil
}

func Start() error {
	if installed, _ := installed(); !installed {
		return ErrNotInstalled
	}
	return systemctlRun("systemctl", "--user", "start", unitName())
}

func setLogin(enabled bool) error {
	if installed, err := installed(); err != nil {
		return err
	} else if !installed {
		return ErrNotInstalled
	}
	verb := "disable"
	if enabled {
		verb = "enable"
	}
	return systemctlRun("systemctl", "--user", verb, unitName())
}

func Stop() error {
	if installed, _ := installed(); !installed {
		return ErrNotInstalled
	}
	return systemctlRun("systemctl", "--user", "stop", unitName())
}

func Restart() error {
	if installed, _ := installed(); !installed {
		return ErrNotInstalled
	}
	return systemctlRun("systemctl", "--user", "restart", unitName())
}

func Remove() error {
	path, err := unitPath()
	if err != nil {
		return err
	}
	if _, err := os.Stat(path); os.IsNotExist(err) {
		return nil
	} else if err != nil {
		return err
	}
	if err := Stop(); err != nil {
		return err
	}
	if err := systemctlRun("systemctl", "--user", "disable", unitName()); err != nil {
		return err
	}
	if err := os.Remove(path); err != nil {
		return err
	}
	return systemctlRun("systemctl", "--user", "daemon-reload")
}

func installed() (bool, error) {
	path, err := unitPath()
	if err != nil {
		return false, err
	}
	_, err = os.Stat(path)
	if os.IsNotExist(err) {
		return false, nil
	}
	return err == nil, err
}

func GetStatus() (Status, error) {
	s := Status{Manager: "systemd --user"}
	if err := systemctlRun("systemctl", "--user", "show-environment"); err != nil {
		return s, fmt.Errorf("%w: %w", ErrManagerUnavailable, err)
	}
	var err error
	s.Installed, err = installed()
	if err != nil || !s.Installed {
		return s, err
	}
	s.AutoStart = systemctlRun("systemctl", "--user", "is-enabled", "--quiet", unitName()) == nil
	s.Active = systemctlRun("systemctl", "--user", "is-active", "--quiet", unitName()) == nil
	return s, nil
}

func removeLegacyCron() error {
	// The old watchdog used a unique two-line marker. Remove only that pair.
	output, err := exec.Command("crontab", "-l").CombinedOutput()
	if err != nil {
		if os.IsNotExist(err) || strings.Contains(strings.ToLower(string(output)), "no crontab for") {
			return nil
		}
		return fmt.Errorf("reading legacy crontab: %w: %s", err, output)
	}
	marker := "# " + strings.ToUpper(brand.Brand) + "_DAEMON_SCHEDULER"
	lines := strings.Split(string(output), "\n")
	cleaned := make([]string, 0, len(lines))
	found := false
	for i := 0; i < len(lines); i++ {
		if strings.TrimSpace(lines[i]) == marker {
			if i+1 >= len(lines) || !strings.Contains(lines[i+1], " daemon") {
				return fmt.Errorf("legacy crontab marker has no daemon command; refusing to alter crontab")
			}
			found = true
			i++
			continue
		}
		cleaned = append(cleaned, lines[i])
	}
	if !found {
		return nil
	}
	cmd := exec.Command("crontab", "-")
	cmd.Stdin = strings.NewReader(strings.TrimRight(strings.Join(cleaned, "\n"), "\n") + "\n")
	if output, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("migrating legacy crontab: %w: %s", err, output)
	}
	return nil
}

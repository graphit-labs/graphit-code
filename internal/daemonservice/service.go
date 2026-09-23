// Package daemonservice manages the per-user operating-system supervisor for
// the Graphit daemon. The tray is a separate process in the graphical session.
package daemonservice

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

var ErrNotInstalled = errors.New("daemon service is not installed")

type Status struct {
	Installed bool
	AutoStart bool
	Active    bool
	Manager   string
}

func (s Status) String() string {
	if !s.Installed {
		return "not installed"
	}
	state := "stopped"
	if s.Active {
		state = "active"
	}
	login := "login off"
	if s.AutoStart {
		login = "login on"
	}
	return fmt.Sprintf("%s (%s, %s)", state, s.Manager, login)
}

type commandRunner func(name string, args ...string) error

func run(name string, args ...string) error {
	cmd := exec.Command(name, args...)
	if output, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("%s %v: %w: %s", name, args, err, output)
	}
	return nil
}

func serviceDir() string { return filepath.Join(brand.GlobalDir(), "daemon") }

func serviceName() string { return brand.Brand + "-daemon" }

func writeFileAtomic(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	f, err := os.CreateTemp(filepath.Dir(path), ".graphit-service-*")
	if err != nil {
		return err
	}
	defer os.Remove(f.Name())
	if err := f.Chmod(0o600); err != nil {
		f.Close()
		return err
	}
	if _, err := f.Write(data); err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return os.Rename(f.Name(), path)
}

// Install serializes registration across independent CLI processes. The daemon
// PID lock remains the final one-instance guard during Start.
func Install(autoStart bool) error {
	if err := os.MkdirAll(serviceDir(), 0o700); err != nil {
		return err
	}
	lock, err := lockfile.Acquire(filepath.Join(serviceDir(), ".service-install.lock"), 10*time.Second)
	if err != nil {
		return fmt.Errorf("acquiring service install lock: %w", err)
	}
	defer lock.Release()
	return install(autoStart)
}

// EnsureInstalled is the ordinary CLI autostart path. It never changes a
// user's existing login preference.
func EnsureInstalled() error {
	if err := os.MkdirAll(serviceDir(), 0o700); err != nil {
		return err
	}
	lock, err := lockfile.Acquire(filepath.Join(serviceDir(), ".service-install.lock"), 10*time.Second)
	if err != nil {
		return fmt.Errorf("acquiring service install lock: %w", err)
	}
	defer lock.Release()
	status, err := GetStatus()
	if err != nil {
		return err
	}
	if status.Installed {
		return nil
	}
	return install(false)
}

// ResolveExecutable prefers the stable launcher when the launcher provided its
// path. A directly executed binary uses its own path, even if another Graphit
// binary appears first on PATH.
func ResolveExecutable() (string, error) {
	if path := os.Getenv(brand.EnvVar("LAUNCHER_PATH")); path != "" {
		absolute, err := filepath.Abs(path)
		if err != nil {
			return "", err
		}
		info, err := os.Stat(absolute)
		if err != nil {
			return "", fmt.Errorf("graphit launcher %s: %w", absolute, err)
		}
		if info.IsDir() {
			return "", fmt.Errorf("graphit launcher %s is a directory", absolute)
		}
		return absolute, nil
	}
	if path, err := os.Executable(); err == nil {
		return filepath.Abs(path)
	}
	if path, err := exec.LookPath(brand.BinName()); err == nil {
		return filepath.Abs(path)
	}
	return "", fmt.Errorf("cannot find %s launcher on PATH", brand.BinName())
}

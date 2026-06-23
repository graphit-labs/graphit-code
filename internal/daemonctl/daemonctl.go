// Package daemonctl provides lightweight daemon lifecycle helpers that
// do not import heavy packages (ast, sqlite). Safe for CGO_ENABLED=0 binaries.
package daemonctl

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

func DaemonDir() string {
	return filepath.Join(brand.GlobalDir(), "daemon")
}

func PIDFilePath() string  { return filepath.Join(DaemonDir(), "daemon.pid") }
func PortFilePath() string { return filepath.Join(DaemonDir(), "mcp.port") }
func KeyFilePath() string  { return filepath.Join(DaemonDir(), "mcp.key") }

func EnsureRunning() (bool, error) {
	if isDaemonLocked() {
		return false, nil
	}

	exe := ResolveExe()
	if exe == "" {
		return false, nil
	}

	cmd := exec.Command(exe, "daemon")
	cmd.Stdin = nil
	cmd.Stdout = nil
	cmd.Stderr = nil
	sysutil.DetachProcess(cmd)

	if err := cmd.Start(); err != nil {
		return false, err
	}
	go func() { _ = cmd.Wait() }()
	return true, nil
}

func isDaemonLocked() bool {
	f, err := os.Open(PIDFilePath())
	if err != nil {
		return false
	}
	defer f.Close()

	if err := flockProbe(f); err != nil {
		return true
	}
	flockProbeRelease(f)
	return false
}

func ResolveExe() string {
	if launcher := os.Getenv(brand.EnvVar("LAUNCHER_PATH")); launcher != "" {
		if _, err := os.Stat(launcher); err == nil {
			return launcher
		}
	}
	exe, err := os.Executable()
	if err != nil {
		return ""
	}
	exe, err = filepath.EvalSymlinks(exe)
	if err != nil {
		return ""
	}
	return exe
}

// LauncherStampPath returns the path to the launcher stamp file.
func LauncherStampPath() string {
	return filepath.Join(DaemonDir(), "launcher.stamp")
}

// ReadLauncherStamp reads the current launcher stamp (SHA256 of the core
// executable). Returns an empty string if the file does not exist or is blank.
func ReadLauncherStamp() string {
	data, err := os.ReadFile(LauncherStampPath())
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(data))
}

package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func resolveDaemonExe() string {
	// Primary: use the launcher binary (set by the launcher when it spawns us).
	// The launcher handles runtime extraction and version switching, so the
	// daemon must always be spawned through it.
	if launcher := os.Getenv(brand.EnvVar("LAUNCHER_PATH")); launcher != "" {
		if _, err := os.Stat(launcher); err == nil {
			return launcher
		}
	}

	// Fallback: look for the launcher in $PATH.
	launcherName := brand.BinName()
	if runtime.GOOS == "windows" {
		launcherName += ".exe"
	}
	if p, err := exec.LookPath(launcherName); err == nil {
		if abs, err := filepath.Abs(p); err == nil {
			return abs
		}
		return p
	}

	return ""
}

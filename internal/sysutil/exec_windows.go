//go:build windows

package sysutil

import (
	"errors"
	"os"
	"os/exec"
)

func SanitizeInheritedFDs() {}

func ReplaceProcess(argv0 string, argv []string, envv []string) error {
	cmd := exec.Command(argv0, argv[1:]...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Env = envv

	if err := cmd.Start(); err != nil {
		return err
	}

	waitErr := cmd.Wait()
	if waitErr == nil {
		os.Exit(0)
	}

	var exitErr *exec.ExitError
	if errors.As(waitErr, &exitErr) {
		os.Exit(exitErr.ExitCode())
	}
	os.Exit(1)
	return nil
}

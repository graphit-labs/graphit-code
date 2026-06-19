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

	_ = os.Stdin.Close()

	err := cmd.Wait()
	if err != nil {
		var exitError *exec.ExitError
		if errors.As(err, &exitError) {
			os.Exit(exitError.ExitCode())
		}
		os.Exit(1)
	}
	os.Exit(0)
	return nil
}

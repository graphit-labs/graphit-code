//go:build windows

package sysutil

import (
	"errors"
	"os"
	"os/exec"
	"strings"
)

const relayExitCode = 42

func SanitizeInheritedFDs() {}

func ReplaceProcess(argv0 string, argv []string, envv []string) error {
	workerEnv := ensureWorkerEnv(envv)

	if isRelayWorker() {
		runChild(argv0, argv, workerEnv)
		os.Exit(relayExitCode)
		return nil
	}

	for {
		runChild(argv0, argv, workerEnv)
	}
}

func runChild(argv0 string, argv []string, env []string) {
	cmd := exec.Command(argv0, argv[1:]...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Env = env

	if err := cmd.Start(); err != nil {
		os.Exit(1)
	}

	waitErr := cmd.Wait()
	if waitErr == nil {
		os.Exit(0)
	}

	var exitErr *exec.ExitError
	if errors.As(waitErr, &exitErr) {
		if exitErr.ExitCode() == relayExitCode {
			return
		}
		os.Exit(exitErr.ExitCode())
	}
	os.Exit(1)
}

func ensureWorkerEnv(envv []string) []string {
	out := make([]string, 0, len(envv)+1)
	for _, e := range envv {
		k, _, _ := strings.Cut(e, "=")
		if k == relayWorkerEnv {
			continue
		}
		out = append(out, e)
	}
	return append(out, relayWorkerEnv+"=1")
}

func isRelayWorker() bool {
	return os.Getenv(relayWorkerEnv) != ""
}

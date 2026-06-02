package git

import (
	"fmt"
	"os/exec"
	"sync"
)

type Git interface {
	Run(repoDir string, args ...string) error

	RunOutput(repoDir string, args ...string) (string, error)

	RunSilent(repoDir string, args ...string) string

	RunWithStdin(repoDir string, data []byte, args ...string) (string, error)

	RunWithEnv(repoDir string, env map[string]string, args ...string) error

	RunOutputWithEnv(repoDir string, env map[string]string, args ...string) (string, error)

	RunGlobal(args ...string) error

	RunGlobalOutput(args ...string) (string, error)
}

var (
	defaultInstance Git
	defaultOnce     sync.Once
	defaultInitErr  error
)

// Default returns the singleton Git instance. Returns nil if git is not
// available in PATH — callers that cannot tolerate this should use DefaultErr.
func Default() Git {
	defaultOnce.Do(func() {
		if _, err := exec.LookPath("git"); err != nil {
			defaultInitErr = fmt.Errorf("git CLI not found in PATH: %w", err)
			return
		}
		defaultInstance = &cliBackend{}
	})
	return defaultInstance
}

// DefaultErr returns the singleton Git instance along with any initialization
// error. Use this instead of Default when you need to handle missing git
// gracefully rather than risking a nil-pointer dereference.
func DefaultErr() (Git, error) {
	_ = Default()
	return defaultInstance, defaultInitErr
}


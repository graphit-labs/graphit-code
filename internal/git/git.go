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
)

func Default() Git {
	defaultOnce.Do(func() {
		if _, err := exec.LookPath("git"); err != nil {
			panic(fmt.Sprintf("git CLI not found in PATH: %v — run 'graphit setup' to verify prerequisites", err))
		}
		defaultInstance = &cliBackend{}
	})
	return defaultInstance
}

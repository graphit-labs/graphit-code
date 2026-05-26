package git

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

type cliBackend struct{}

func (c *cliBackend) Run(repoDir string, args ...string) error {
	cmd := c.buildCmd(repoDir, nil, args...)
	cmd.Stdin = os.Stdin
	var combined bytes.Buffer
	cmd.Stdout = &combined
	cmd.Stderr = &combined
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("%w: %s", err, CleanStderr(combined.String()))
	}
	return nil
}

func (c *cliBackend) RunOutput(repoDir string, args ...string) (string, error) {
	cmd := c.buildCmd(repoDir, nil, args...)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("%w: %s", err, CleanStderr(stderr.String()))
	}
	return strings.TrimSpace(string(out)), nil
}

func (c *cliBackend) RunSilent(repoDir string, args ...string) string {
	out, _ := c.RunOutput(repoDir, args...)
	return out
}

func (c *cliBackend) RunWithStdin(repoDir string, data []byte, args ...string) (string, error) {
	cmd := c.buildCmd(repoDir, nil, args...)
	cmd.Stdin = bytes.NewReader(data)
	out, err := cmd.Output()
	return strings.TrimSpace(string(out)), err
}

func (c *cliBackend) RunWithEnv(repoDir string, env map[string]string, args ...string) error {
	cmd := c.buildCmd(repoDir, env, args...)
	var combined bytes.Buffer
	cmd.Stdout = &combined
	cmd.Stderr = &combined
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("%w: %s", err, CleanStderr(combined.String()))
	}
	return nil
}

func (c *cliBackend) RunOutputWithEnv(repoDir string, env map[string]string, args ...string) (string, error) {
	cmd := c.buildCmd(repoDir, env, args...)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("%w: %s", err, CleanStderr(stderr.String()))
	}
	return strings.TrimSpace(string(out)), nil
}

func (c *cliBackend) RunGlobal(args ...string) error {
	return c.Run("", args...)
}

func (c *cliBackend) RunGlobalOutput(args ...string) (string, error) {
	return c.RunOutput("", args...)
}

func (c *cliBackend) buildCmd(repoDir string, env map[string]string, args ...string) *exec.Cmd {
	var fullArgs []string
	if repoDir != "" {
		fullArgs = append([]string{"-C", repoDir}, args...)
	} else {
		fullArgs = args
	}
	cmd := exec.Command("git", fullArgs...)
	if len(env) > 0 {
		cmd.Env = append(os.Environ(), MapToEnv(env)...)
	}
	return cmd
}

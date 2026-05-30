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
		stderr := combined.String()
		return wrapSSHError(fmt.Errorf("%w: %s", err, CleanStderr(stderr)), stderr)
	}
	return nil
}

func (c *cliBackend) RunOutput(repoDir string, args ...string) (string, error) {
	cmd := c.buildCmd(repoDir, nil, args...)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		stderrStr := stderr.String()
		return "", wrapSSHError(fmt.Errorf("%w: %s", err, CleanStderr(stderrStr)), stderrStr)
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
		stderr := combined.String()
		return wrapSSHError(fmt.Errorf("%w: %s", err, CleanStderr(stderr)), stderr)
	}
	return nil
}

func (c *cliBackend) RunOutputWithEnv(repoDir string, env map[string]string, args ...string) (string, error) {
	cmd := c.buildCmd(repoDir, env, args...)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		stderrStr := stderr.String()
		return "", wrapSSHError(fmt.Errorf("%w: %s", err, CleanStderr(stderrStr)), stderrStr)
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
	baseEnv := os.Environ()
	if _, ok := os.LookupEnv("GIT_SSH_COMMAND"); !ok {
		baseEnv = append(baseEnv, "GIT_SSH_COMMAND=ssh -o BatchMode=yes")
	}
	if len(env) > 0 {
		baseEnv = append(baseEnv, MapToEnv(env)...)
	}
	cmd.Env = baseEnv
	return cmd
}

func wrapSSHError(err error, stderr string) error {
	if err == nil {
		return nil
	}
	lower := strings.ToLower(stderr)
	if strings.Contains(lower, "host key verification failed") ||
		strings.Contains(lower, "no matching host key") ||
		strings.Contains(lower, "known_hosts") {
		host := extractHost(stderr)
		hint := "the remote host is not in your known_hosts file.\n"
		if host != "" {
			hint += fmt.Sprintf("  Verify the host manually:  ssh -T %s\n", host)
		} else {
			hint += "  Verify the host manually:  ssh -T git@<hostname>\n"
		}
		hint += "  Once verified, retry the operation."
		return fmt.Errorf("%w\n\n%s", err, hint)
	}
	return err
}

func extractHost(stderr string) string {
	for _, line := range strings.Split(stderr, "\n") {
		lower := strings.ToLower(line)
		if strings.Contains(lower, "host key verification") || strings.Contains(lower, "known_hosts") {
			for _, token := range strings.Fields(line) {
				if strings.Contains(token, "@") {
					return strings.Trim(token, "'\"")
				}
			}
		}
	}
	return ""
}


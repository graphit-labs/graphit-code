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
	baseEnv := withoutInheritedGitScope(os.Environ())
	if _, ok := os.LookupEnv("GIT_SSH_COMMAND"); !ok {
		baseEnv = append(baseEnv, "GIT_SSH_COMMAND=ssh -o BatchMode=yes")
	}
	if len(env) > 0 {
		baseEnv = append(baseEnv, MapToEnv(env)...)
	}
	cmd.Env = baseEnv
	return cmd
}

// gitInvocationScopeEnv names the variables git exports to describe the invocation that is
// running, which therefore must never be inherited by a git command aimed at another
// repository.
//
// Every call in this package names its repository with `-C`, so inheriting any of these is
// always wrong: they re-point git at a repository, an index or a moment that belongs to
// whatever launched this process.
//
// This is not hypothetical. Git hooks export exactly this set, and a hook that starts a
// long-lived process hands it that environment for the rest of its life. Measured on the
// machine this was written on: the daemon carried GIT_INDEX_FILE=.git/index — RELATIVE, so
// it resolved against each `-C` target — plus GIT_PREFIX, GIT_EXEC_PATH and a pinned
// GIT_AUTHOR_DATE. Every memory commit it attempted died on
//
//	fatal: Unable to create '<worktree>/.git/index.lock': Not a directory
//
// because a linked worktree's `.git` is a FILE, not a directory. Memories were written to
// disk and never committed, and the error named a lock — which reads like contention, not
// like a stray environment variable.
//
// The commit dates are here for the same reason and a different failure: inherited, they
// stamp every commit with the hook's timestamp instead of the moment the work happened.
// The author IDENTITY is deliberately NOT stripped — git falls back to the same config
// values anyway, and it is the one variable in this family a person may legitimately set
// for a whole session.
var gitInvocationScopeEnv = map[string]struct{}{
	"GIT_DIR":                          {},
	"GIT_COMMON_DIR":                   {},
	"GIT_WORK_TREE":                    {},
	"GIT_INDEX_FILE":                   {},
	"GIT_INDEX_VERSION":                {},
	"GIT_PREFIX":                       {},
	"GIT_OBJECT_DIRECTORY":             {},
	"GIT_ALTERNATE_OBJECT_DIRECTORIES": {},
	"GIT_NAMESPACE":                    {},
	"GIT_GRAFT_FILE":                   {},
	"GIT_QUARANTINE_PATH":              {},
	"GIT_REFLOG_ACTION":                {},
	"GIT_AUTHOR_DATE":                  {},
	"GIT_COMMITTER_DATE":               {},
}

// withoutInheritedGitScope drops gitInvocationScopeEnv from an environment.
//
// It filters the INHERITED environment only. buildCmd appends a caller's explicit env after
// this, and a later assignment wins, so a caller that means to set one of these still can.
func withoutInheritedGitScope(environ []string) []string {
	clean := make([]string, 0, len(environ))
	for _, kv := range environ {
		name, _, ok := strings.Cut(kv, "=")
		if ok {
			if _, drop := gitInvocationScopeEnv[name]; drop {
				continue
			}
		}
		clean = append(clean, kv)
	}
	return clean
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

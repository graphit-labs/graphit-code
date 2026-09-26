package commands

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
	"github.com/spf13/cobra"
)

// newGitHookCmd is the stable entry point used by managed Git hook scripts.
// The script checks for the executable before invoking this command.
func newGitHookCmd() *cobra.Command {
	return &cobra.Command{
		Use:                "_git-hook <hook-name>",
		Hidden:             true,
		DisableFlagParsing: true,
		Args:               cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			projectDir, err := os.Getwd()
			if err != nil {
				return fmt.Errorf("git hook: determine project directory: %w", err)
			}
			return runConfiguredGitHook(cmd.Context(), projectDir, args[0], cmd.InOrStdin(), cmd.OutOrStdout(), cmd.ErrOrStderr(), args[1:]...)
		},
	}
}

func runConfiguredGitHook(ctx context.Context, projectDir, hookName string, stdin io.Reader, stdout, stderr io.Writer, hookArgs ...string) error {
	lockPath := filepath.Join(projectDir, brand.LockFileName())
	lf, err := projectlock.Load(lockPath)
	if err != nil {
		return fmt.Errorf("git hook %s: %w", hookName, err)
	}
	if lf == nil {
		return fmt.Errorf("git hook %s: project lockfile %s not found", hookName, brand.LockFileName())
	}
	commands := lf.Hooks[hookName]
	if (hookName == "proc-receive" || hookName == "fsmonitor-watchman") && len(commands) > 1 {
		return fmt.Errorf("git hook %s: its interactive/output protocol requires exactly one command", hookName)
	}
	var input []byte
	finiteInput := hookHasFiniteInput(hookName)
	if len(commands) > 0 && finiteInput {
		input, err = io.ReadAll(stdin)
		if err != nil {
			return fmt.Errorf("git hook %s: read stdin: %w", hookName, err)
		}
	}
	env := append(os.Environ(), "GRAPHIT_HOOK_NAME="+hookName, fmt.Sprintf("GRAPHIT_HOOK_ARG_COUNT=%d", len(hookArgs)))
	for i, arg := range hookArgs {
		env = append(env, fmt.Sprintf("GRAPHIT_HOOK_ARG_%d=%s", i+1, arg))
	}

	var failures []error
	for i, command := range commands {
		if strings.TrimSpace(command) == "" {
			failures = append(failures, fmt.Errorf("git hook %s: command %d is empty", hookName, i+1))
			continue
		}

		var process *exec.Cmd
		if runtime.GOOS == "windows" {
			process = exec.CommandContext(ctx, "cmd", "/C", command)
		} else {
			process = exec.CommandContext(ctx, "sh", append([]string{"-c", command, hookName}, hookArgs...)...)
		}
		process.Dir = projectDir
		process.Env = env
		if finiteInput {
			process.Stdin = bytes.NewReader(input)
		} else {
			process.Stdin = stdin
		}
		process.Stdout = stdout
		process.Stderr = stderr
		if err := process.Run(); err != nil {
			failures = append(failures, fmt.Errorf("git hook %s: command %d failed: %w", hookName, i+1, err))
		}
	}
	return errors.Join(failures...)
}

// Git supplies a finite payload on stdin for these events. Other hooks may
// inherit an open terminal, so reading them to EOF before a command would hang.
func hookHasFiniteInput(event string) bool {
	switch event {
	case "pre-push", "pre-receive", "post-receive", "post-rewrite", "reference-transaction":
		return true
	default:
		return false
	}
}

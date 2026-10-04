package commands

import (
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/sessioncontext"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
	"github.com/spf13/cobra"
)

func newSessionHookCmd() *cobra.Command {
	var format string
	var projectDir string
	var syncBeforeOutput bool

	cmd := &cobra.Command{
		Use:    "_session-hook",
		Short:  "Render internal Agent lifecycle hook output",
		Hidden: true,
		Args:   cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			input, err := readSessionHookInput(cmd.InOrStdin(), format)
			if err != nil {
				return fmt.Errorf("reading session hook input: %w", err)
			}
			projectDir = resolveSessionHookProjectDir(projectDir, input)
			if syncBeforeOutput && projectDir == "" {
				return fmt.Errorf("no Graphit project lockfile was found from the hook input")
			}
			payload, err := renderSessionHook(cmd.Context(), projectDir, format, input)
			if err != nil {
				return err
			}
			if syncBeforeOutput {
				if err := dispatchFinalHookSync(projectDir); err != nil {
					return err
				}
			}
			if len(payload) == 0 {
				return nil
			}
			_, err = fmt.Fprintln(cmd.OutOrStdout(), string(payload))
			return err
		},
	}
	cmd.Flags().StringVar(&format, "format", "", "session hook output format")
	cmd.Flags().StringVar(&projectDir, "project-dir", "", "starting directory for Graphit project discovery")
	cmd.Flags().BoolVar(&syncBeforeOutput, "sync", false, "dispatch a full project sync asynchronously before rendering completion output")
	_ = cmd.MarkFlagRequired("format")
	return cmd
}

// renderSessionHook applies a hook event for projectDir and renders the payload the
// host expects, with the store-backed half applied in this process.
func renderSessionHook(ctx context.Context, projectDir, format string, input []byte) ([]byte, error) {
	return sessioncontext.Render(ctx, projectDir, format, input, applySessionHookStore)
}

// applySessionHookStore is the store-backed half of a hook event. The core command runs
// it in-process; the daemon runs it for the lightweight hook binary.
func applySessionHookStore(ctx context.Context, req sessionhook.StoreRequest) (sessionhook.StoreResult, error) {
	var result sessionhook.StoreResult
	if req.Task {
		if err := runTaskSessionHook(ctx, req.ProjectDir, req.Format, req.Input); err != nil {
			return result, err
		}
	}
	if req.Mandatory {
		result.Mandatory, result.MandatoryLoaded = loadMandatoryContext(req.ProjectDir)
	}
	return result, nil
}

func loadMandatoryContext(projectDir string) (string, bool) {
	return loadMandatoryContextWith(projectDir, memory.ListMandatoryMemoriesForProject)
}

func loadMandatoryContextWith(projectDir string, list func(string, string) ([]memory.MandatoryEntry, error)) (string, bool) {
	var sections []string
	for _, scope := range []string{"project", "user"} {
		entries, err := list(projectDir, scope)
		if err != nil {
			return "", false
		}
		for _, entry := range entries {
			sections = append(sections, fmt.Sprintf("### %s memory: %s\n%s", scope, entry.Title, entry.Content))
		}
	}
	return strings.Join(sections, "\n\n"), true
}

func runTaskSessionHook(ctx context.Context, projectDir, format string, input []byte) error {
	if projectDir == "" || !lancestore.Available() {
		return nil
	}
	projectCfg := loadProjectConfigFromDir(projectDir)
	if config.IsModuleDisabled("task", nil, projectCfg) {
		return nil
	}
	svc, err := graphtask.Open(projectDir)
	if err != nil {
		return fmt.Errorf("opening task lifecycle store: %w", err)
	}
	agent := graphtask.AgentIDFromHook(input)
	switch strings.ToLower(format) {
	case sessionhook.FormatStop, sessionhook.FormatCursorStop,
		sessionhook.FormatAfterAgent, sessionhook.FormatAntigravityStop,
		sessionhook.FormatSessionEnd, sessionhook.FormatNoOutput:
		return svc.ReleaseOwned(ctx, agent)
	case sessionhook.FormatPostToolUse, sessionhook.FormatAfterTool,
		sessionhook.FormatCursorUnit, sessionhook.FormatPlainUnit,
		sessionhook.FormatPostInvocation:
		return svc.HeartbeatOwned(ctx, agent, graphtask.DefaultLease)
	default:
		return svc.Reconcile(ctx)
	}
}

func dispatchFinalHookSync(projectDir string) error {
	if projectDir == "" {
		return fmt.Errorf("no Graphit project lockfile was found from the hook input")
	}
	executable, err := os.Executable()
	if err != nil {
		return fmt.Errorf("resolving the active Graphit executable: %w", err)
	}
	return dispatchFinalHookSyncExecutable(projectDir, executable)
}

func dispatchFinalHookSyncExecutable(projectDir, executable string) error {
	command := finalHookSyncCommand(projectDir, executable)
	if err := command.Start(); err != nil {
		return fmt.Errorf("starting asynchronous project sync: %w", err)
	}
	if err := command.Process.Release(); err != nil {
		return fmt.Errorf("releasing asynchronous project sync: %w", err)
	}
	return nil
}

func finalHookSyncCommand(projectDir, executable string) *exec.Cmd {
	command := exec.Command(executable, "sync")
	command.Dir = projectDir
	return command
}

func resolveSessionHookProjectDir(explicit string, input []byte) string {
	return sessionhook.ResolveProjectDir(explicit, input)
}

func readSessionHookInput(input io.Reader, format string) ([]byte, error) {
	return sessionhook.ReadInput(input, format)
}

package commands

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
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
		Short:  "Render internal IDE lifecycle hook output",
		Hidden: true,
		Args:   cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			input, err := readSessionHookInput(cmd.InOrStdin(), format)
			if err != nil {
				return fmt.Errorf("reading session hook input: %w", err)
			}
			projectDir = resolveSessionHookProjectDir(projectDir, input)
			if err := runTaskSessionHook(cmd.Context(), projectDir, format, input); err != nil {
				return err
			}
			if syncBeforeOutput {
				if err := dispatchFinalHookSync(projectDir); err != nil {
					return err
				}
			}
			includeMandatory := hookInputNeedsMandatory(format, input)
			context := sessionhook.Context{}
			if includeMandatory || strings.EqualFold(format, sessionhook.FormatToolContext) {
				context = sessioncontext.Build(projectDir, includeMandatory)
			}
			payload, err := sessionhook.RenderWithContext(format, input, context)
			if err != nil {
				return err
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
		sessionhook.FormatSessionEnd:
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
	type hookInput struct {
		CWD            string   `json:"cwd"`
		WorkspaceRoots []string `json:"workspace_roots"`
		WorkspacePaths []string `json:"workspacePaths"`
	}

	candidates := make([]string, 0, 5)
	if explicit != "" {
		return findSessionHookProjectRoot(explicit)
	}

	var event hookInput
	if json.Unmarshal(input, &event) == nil {
		candidates = append(candidates, event.CWD)
		candidates = append(candidates, event.WorkspaceRoots...)
		candidates = append(candidates, event.WorkspacePaths...)
	}
	if wd, err := os.Getwd(); err == nil {
		candidates = append(candidates, wd)
	}

	for _, candidate := range candidates {
		if root := findSessionHookProjectRoot(candidate); root != "" {
			return root
		}
	}
	return ""
}

func findSessionHookProjectRoot(start string) string {
	return sessioncontext.FindProjectRoot(start)
}

func hookInputNeedsMandatory(format string, input []byte) bool {
	switch strings.ToLower(format) {
	case sessionhook.FormatFirstInvocation:
		var event struct {
			InvocationNum *int `json:"invocationNum"`
		}
		return json.Unmarshal(input, &event) == nil && event.InvocationNum != nil && *event.InvocationNum == 0
	case sessionhook.FormatSessionStart, sessionhook.FormatAdditionalContext,
		sessionhook.FormatPlainContext, sessionhook.FormatSubagentStart,
		sessionhook.FormatCursorSubagentTask:
		return true
	default:
		return false
	}
}

func readSessionHookInput(input io.Reader, format string) ([]byte, error) {
	if file, ok := input.(*os.File); ok {
		info, err := file.Stat()
		if err != nil {
			return nil, fmt.Errorf("inspecting session hook input: %w", err)
		}
		if info.Mode()&os.ModeCharDevice != 0 {
			if strings.EqualFold(format, sessionhook.FormatFirstInvocation) {
				return []byte(`{"invocationNum":0}`), nil
			}
			return nil, nil
		}
	}
	return io.ReadAll(input)
}

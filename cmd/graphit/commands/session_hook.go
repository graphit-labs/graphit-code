package commands

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
	"github.com/spf13/cobra"
)

func newSessionHookCmd() *cobra.Command {
	var format string

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
			mandatory, loaded := "", false
			if hookInputNeedsMandatory(format, input) {
				mandatory, loaded = loadMandatoryHookContext()
			}
			var payload []byte
			if loaded {
				payload, err = sessionhook.RenderWithMandatory(format, input, mandatory)
			} else {
				payload, err = sessionhook.Render(format, input)
			}
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
	_ = cmd.MarkFlagRequired("format")
	return cmd
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

func loadMandatoryHookContext() (string, bool) {
	return loadMandatoryHookContextWith(memory.ListMandatoryMemories)
}

func loadMandatoryHookContextWith(list func(string) ([]memory.MandatoryEntry, error)) (string, bool) {
	var sections []string
	for _, scope := range []string{"project", "user"} {
		entries, err := list(scope)
		if err != nil {
			return "", false
		}
		for _, entry := range entries {
			sections = append(sections, fmt.Sprintf("### %s memory: %s\n%s", scope, entry.Title, entry.Content))
		}
	}
	return strings.Join(sections, "\n\n"), true
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

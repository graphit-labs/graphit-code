package commands

import (
	"fmt"
	"io"
	"os"
	"strings"

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
			payload, err := sessionhook.Render(format, input)
			if err != nil {
				return err
			}
			_, err = fmt.Fprintln(cmd.OutOrStdout(), string(payload))
			return err
		},
	}
	cmd.Flags().StringVar(&format, "format", "", "session hook output format")
	_ = cmd.MarkFlagRequired("format")
	return cmd
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

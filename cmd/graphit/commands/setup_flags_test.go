package commands

import (
	"bufio"
	"io"
	"strings"
	"testing"

	"github.com/spf13/cobra"
)

type failingReader struct{ t *testing.T }

func (r failingReader) Read([]byte) (int, error) {
	r.t.Helper()
	r.t.Fatal("stdin was read for an explicitly answered question")
	return 0, io.EOF
}

func noPrompts(t *testing.T) *bufio.Reader { return bufio.NewReader(failingReader{t: t}) }
func answered(value string) setupAnswer    { return setupAnswer{given: value, set: true} }

func TestSetupRegistersOnlyRuntimeAnswers(t *testing.T) {
	cmd := newSetupCmd()
	for _, absent := range []string{
		"username", "hub-bucket", "hub-region", "hub-endpoint", "hub-access-key-id", "hub-secret-access-key",
		"embedding-provider", "embedding-model", "embedding-base-url", "embedding-api-key",
		"rerank-provider", "rerank-model", "rerank-api-key",
		"embedding-device", "embedding-device-id", "rerank-device", "rerank-device-id",
	} {
		if cmd.Flags().Lookup(absent) != nil {
			t.Errorf("setup unexpectedly registers --%s", absent)
		}
	}
	var answers setupAnswers
	for name := range answers.fields() {
		if cmd.Flags().Lookup(name) == nil {
			t.Errorf("--%s is not registered", name)
		}
	}
}

func TestAgentFlagsHaveNoIDEAlias(t *testing.T) {
	commands := []struct {
		name       string
		cmd        *cobra.Command
		persistent bool
	}{
		{name: "setup", cmd: newSetupCmd()},
		{name: "init", cmd: newInitCmd()},
		{name: "update", cmd: newUpdateCmd()},
		{name: "remove", cmd: newRemoveCmd()},
		{name: "sync", cmd: newSyncCmd()},
		{name: "hub", cmd: newHubCmd(), persistent: true},
	}

	for _, tc := range commands {
		t.Run(tc.name, func(t *testing.T) {
			flags := tc.cmd.Flags()
			if tc.persistent {
				flags = tc.cmd.PersistentFlags()
			}
			if flags.Lookup("agent") == nil {
				t.Fatal("--agent is not registered")
			}
			if flags.Lookup("ide") != nil {
				t.Fatal("removed --ide alias is still registered")
			}
			if err := flags.Parse([]string{"--ide", "cursor"}); err == nil || !strings.Contains(err.Error(), "unknown flag") {
				t.Fatalf("--ide must be rejected as unknown, got %v", err)
			}
		})
	}
}

func TestSetupNonInteractiveRequiresOnlyRuntimeAnswers(t *testing.T) {
	var answers setupAnswers
	cmd := &cobra.Command{Use: "setup"}
	answers.register(cmd)
	if err := cmd.Flags().Parse([]string{"--agent", "cursor"}); err != nil {
		t.Fatal(err)
	}
	answers.bind(cmd)
	if err := answers.validateNonInteractive(); err == nil {
		t.Fatal("expected a missing-input error")
	}

	args := []string{"--anonymize-events=false", "--agent=cursor", "--cli=codex"}
	var complete setupAnswers
	completeCmd := &cobra.Command{Use: "setup"}
	complete.register(completeCmd)
	if err := completeCmd.Flags().Parse(args); err != nil {
		t.Fatal(err)
	}
	complete.bind(completeCmd)
	if err := complete.validateNonInteractive(); err != nil {
		t.Fatalf("complete flags rejected: %v", err)
	}
}

func TestSetupAnswerSimpleHonorsExplicitFlags(t *testing.T) {
	if got := answered(" cursor ").simple(noPrompts(t), "default Agent", "claude"); got != "cursor" {
		t.Fatalf("simple = %q", got)
	}
}

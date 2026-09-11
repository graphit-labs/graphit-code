package commands

import (
	"bytes"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/spf13/cobra"
	"github.com/spf13/pflag"
)

func setupTestHome(t *testing.T) (string, func()) {
	tempHome, err := os.MkdirTemp("", "graphit-test-home-*")
	if err != nil {
		t.Fatalf("failed to create temp home: %v", err)
	}

	origHome := os.Getenv("HOME")
	origMcpKeys := os.Getenv("GRAPHIT_AGENT")
	origDaemon := os.Getenv("GRAPHIT_MODULES_DAEMON")

	_ = os.Setenv("HOME", tempHome)
	_ = os.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	_ = os.Unsetenv("GRAPHIT_AGENT")

	cleanup := func() {
		_ = os.Setenv("HOME", origHome)
		if origMcpKeys != "" {
			_ = os.Setenv("GRAPHIT_AGENT", origMcpKeys)
		} else {
			_ = os.Unsetenv("GRAPHIT_AGENT")
		}
		if origDaemon != "" {
			_ = os.Setenv("GRAPHIT_MODULES_DAEMON", origDaemon)
		} else {
			_ = os.Unsetenv("GRAPHIT_MODULES_DAEMON")
		}
		_ = os.RemoveAll(tempHome)
	}

	return tempHome, cleanup
}

func executeCommand(args ...string) (string, error) {
	var resetFlags func(*cobra.Command)
	resetFlags = func(cmd *cobra.Command) {
		cmd.Flags().VisitAll(func(flag *pflag.Flag) {
			_ = flag.Value.Set(flag.DefValue)
			flag.Changed = false
		})
		for _, child := range cmd.Commands() {
			resetFlags(child)
		}
	}
	resetFlags(rootCmd)
	nonInteractive = false
	oldStdout := os.Stdout
	oldStderr := os.Stderr
	r, w, err := os.Pipe()
	if err != nil {
		return "", err
	}
	os.Stdout = w
	os.Stderr = w

	rootCmd.SetArgs(args)
	rootCmd.SilenceErrors = true
	rootCmd.SilenceUsage = true

	if configCmd, _, err := rootCmd.Find([]string{"config"}); err == nil && configCmd != nil {
		_ = configCmd.Flags().Set("global", "false")
		_ = configCmd.Flags().Set("get", "false")
		_ = configCmd.Flags().Set("unset", "false")
		_ = configCmd.Flags().Set("list", "false")
		_ = configCmd.Flags().Set("secret", "false")
	}

	execErr := rootCmd.Execute()

	_ = w.Close()
	var buf bytes.Buffer
	_, _ = io.Copy(&buf, r)
	os.Stdout = oldStdout
	os.Stderr = oldStderr

	return buf.String(), execErr
}

func TestCLIHelpAndRoot(t *testing.T) {
	_, cleanup := setupTestHome(t)
	defer cleanup()

	out, err := executeCommand("--help")
	if err != nil {
		t.Errorf("failed to execute help: %v", err)
	}
	if !strings.Contains(out, brand.DisplayName) {
		t.Errorf("expected DisplayName in help output, got: %s", out)
	}
}

func TestASTExportDefaultOutputUsesProjectRuntime(t *testing.T) {
	cmd := newASTExportCmd()
	flag := cmd.Flags().Lookup("output")
	if flag == nil {
		t.Fatal("output flag is missing")
	}
	want := brand.ProjectRuntimePath(".", "ast", "export")
	if flag.DefValue != want {
		t.Fatalf("output default = %q, want %q", flag.DefValue, want)
	}
}

func TestDreamHelpUsesRuntimeReportsDir(t *testing.T) {
	wantDir := brand.ProjectRuntimePath(".", "dream")

	dreamCmd := newDreamCmd()
	if !strings.Contains(dreamCmd.Long, wantDir) {
		t.Fatalf("dream help does not mention default runtime reports directory %q", wantDir)
	}

	reportsCmd := newDreamReportsCmd()
	wantReport := filepath.Join(wantDir, "<id>.md")
	if !strings.Contains(reportsCmd.Long, wantReport) {
		t.Fatalf("dream reports help does not mention default runtime report path %q", wantReport)
	}
}

func TestCLIConfigGlobal(t *testing.T) {
	_, cleanup := setupTestHome(t)
	defer cleanup()

	_, err := executeCommand("config", "--global", "--get", "agent")
	if err == nil {
		t.Error("expected error getting unset global key")
	}

	out, err := executeCommand("config", "--global", "agent", "cursor")
	if err != nil {
		t.Errorf("failed to set config: %v", err)
	}
	if !strings.Contains(out, "Set agent = cursor (global)") {
		t.Errorf("unexpected output when setting key: %s", out)
	}

	out, err = executeCommand("config", "--global", "--get", "agent")
	if err != nil {
		t.Errorf("failed to get config: %v", err)
	}
	if !strings.Contains(out, "cursor") {
		t.Errorf("expected cursor in get output, got: %s", out)
	}

	out, err = executeCommand("config", "--global", "--list")
	if err != nil {
		t.Errorf("failed to list config: %v", err)
	}
	if !strings.Contains(out, "agent:") || !strings.Contains(out, "cursor") {
		t.Errorf("expected agent: cursor in list output, got: %s", out)
	}

	out, err = executeCommand("config", "--global", "--unset", "agent")
	if err != nil {
		t.Errorf("failed to unset config: %v", err)
	}
	if !strings.Contains(out, "Unset agent (global)") {
		t.Errorf("unexpected output when unsetting key: %s", out)
	}

	out, err = executeCommand("config", "--global", "--list")
	if err != nil {
		t.Errorf("failed to list config: %v", err)
	}
	if !strings.Contains(out, "No global configuration set.") {
		t.Errorf("expected empty list output, got: %s", out)
	}
}

func TestCLIConfigProjectWithoutInit(t *testing.T) {
	_, cleanup := setupTestHome(t)
	defer cleanup()

	_, err := executeCommand("config", "agent", "cursor")
	if err == nil {
		t.Error("expected config set to fail without initialized project")
	}

	_, err = executeCommand("config", "--get", "agent")
	if err == nil {
		t.Error("expected config get to fail without initialized project")
	}

	_, err = executeCommand("config", "--unset", "agent")
	if err == nil {
		t.Error("expected config unset to fail without initialized project")
	}

	_, err = executeCommand("config", "--list")
	if err == nil {
		t.Error("expected config list to fail without initialized project")
	}
}

func TestCLIConfigProjectWithInit(t *testing.T) {
	tempHome, cleanup := setupTestHome(t)
	defer cleanup()

	tempProj, err := os.MkdirTemp("", "graphit-test-proj-*")
	if err != nil {
		t.Fatalf("failed to create temp project: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempProj) }()

	oldWd, _ := os.Getwd()
	_ = os.Chdir(tempProj)
	defer func() { _ = os.Chdir(oldWd) }()

	err = os.MkdirAll(filepath.Join(tempHome, "."+brand.Brand), 0755)
	if err != nil {
		t.Fatalf("failed to create global config dir: %v", err)
	}

	err = os.WriteFile(filepath.Join(tempHome, "."+brand.Brand, "config.json"), []byte(`{"hub":{"repo":"git@github.com:graphit-labs/graphit-code.git"}}`), 0644)
	if err != nil {
		t.Fatalf("failed to write global mock config: %v", err)
	}

	out, err := executeCommand("init", "--id", "01H2PJX...", "--name", "test-project", "--description", "test project description", "--agent", "cursor")
	if err != nil {
		t.Errorf("failed to initialize project: %v, output: %s", err, out)
	}

	out, err = executeCommand("config", "agent", "cursor")
	if err != nil {
		t.Errorf("failed to set project config: %v", err)
	}
	if !strings.Contains(out, "Set agent = cursor (project)") {
		t.Errorf("unexpected output when setting project config: %s", out)
	}

	out, err = executeCommand("config", "--get", "agent")
	if err != nil {
		t.Errorf("failed to get project config: %v", err)
	}
	if !strings.Contains(out, "cursor") {
		t.Errorf("expected cursor in get output, got: %s", out)
	}

	out, err = executeCommand("config", "--list")
	if err != nil {
		t.Errorf("failed to list project config: %v", err)
	}
	if !strings.Contains(out, "agent:") || !strings.Contains(out, "cursor") {
		t.Errorf("expected agent: cursor in list output, got: %s", out)
	}

	out, err = executeCommand("config", "--unset", "agent")
	if err != nil {
		t.Errorf("failed to unset project config: %v", err)
	}
	if !strings.Contains(out, "Unset agent (project)") {
		t.Errorf("unexpected output when unsetting project config: %s", out)
	}
}

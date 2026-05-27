package commands

import (
	"bytes"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func setupTestHome(t *testing.T) (string, func()) {
	// Create a temp directory for the HOME folder
	tempHome, err := os.MkdirTemp("", "graphit-test-home-*")
	if err != nil {
		t.Fatalf("failed to create temp home: %v", err)
	}

	origHome := os.Getenv("HOME")
	origMcpKeys := os.Getenv("GRAPHIT_IDE")
	origDaemon := os.Getenv("GRAPHIT_MODULES_DAEMON")

	_ = os.Setenv("HOME", tempHome)
	_ = os.Setenv("GRAPHIT_MODULES_DAEMON", "false") // Disable daemon start in PersistentPreRun
	_ = os.Unsetenv("GRAPHIT_IDE")

	cleanup := func() {
		_ = os.Setenv("HOME", origHome)
		if origMcpKeys != "" {
			_ = os.Setenv("GRAPHIT_IDE", origMcpKeys)
		} else {
			_ = os.Unsetenv("GRAPHIT_IDE")
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
	// Redirect stdout and stderr to a pipe
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

	// Explicitly reset the flags of the config command to false
	if configCmd, _, err := rootCmd.Find([]string{"config"}); err == nil && configCmd != nil {
		_ = configCmd.Flags().Set("global", "false")
		_ = configCmd.Flags().Set("get", "false")
		_ = configCmd.Flags().Set("unset", "false")
		_ = configCmd.Flags().Set("list", "false")
		_ = configCmd.Flags().Set("secret", "false")
	}

	execErr := rootCmd.Execute()

	// Restore and read from pipe
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

func TestCLIConfigGlobal(t *testing.T) {
	_, cleanup := setupTestHome(t)
	defer cleanup()

	// 1. Get unset key
	_, err := executeCommand("config", "--global", "--get", "ide")
	if err == nil {
		t.Error("expected error getting unset global key")
	}

	// 2. Set key
	out, err := executeCommand("config", "--global", "ide", "cursor")
	if err != nil {
		t.Errorf("failed to set config: %v", err)
	}
	if !strings.Contains(out, "Set ide = cursor (global)") {
		t.Errorf("unexpected output when setting key: %s", out)
	}

	// 3. Get key
	out, err = executeCommand("config", "--global", "--get", "ide")
	if err != nil {
		t.Errorf("failed to get config: %v", err)
	}
	if !strings.Contains(out, "cursor") {
		t.Errorf("expected cursor in get output, got: %s", out)
	}

	// 4. List config
	out, err = executeCommand("config", "--global", "--list")
	if err != nil {
		t.Errorf("failed to list config: %v", err)
	}
	if !strings.Contains(out, "ide:") || !strings.Contains(out, "cursor") {
		t.Errorf("expected ide: cursor in list output, got: %s", out)
	}

	// 5. Unset key
	out, err = executeCommand("config", "--global", "--unset", "ide")
	if err != nil {
		t.Errorf("failed to unset config: %v", err)
	}
	if !strings.Contains(out, "Unset ide (global)") {
		t.Errorf("unexpected output when unsetting key: %s", out)
	}

	// 6. List config again (should be empty)
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

	// Should fail because project is not initialized
	_, err := executeCommand("config", "ide", "cursor")
	if err == nil {
		t.Error("expected config set to fail without initialized project")
	}

	_, err = executeCommand("config", "--get", "ide")
	if err == nil {
		t.Error("expected config get to fail without initialized project")
	}

	_, err = executeCommand("config", "--unset", "ide")
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

	// Create a temp project directory
	tempProj, err := os.MkdirTemp("", "graphit-test-proj-*")
	if err != nil {
		t.Fatalf("failed to create temp project: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempProj) }()

	// Change current working directory to the project directory
	oldWd, _ := os.Getwd()
	_ = os.Chdir(tempProj)
	defer func() { _ = os.Chdir(oldWd) }()

	// Create the global config directory first
	err = os.MkdirAll(filepath.Join(tempHome, "."+brand.Brand), 0755)
	if err != nil {
		t.Fatalf("failed to create global config dir: %v", err)
	}

	// Let's set it globally so the init/setup requirements pass
	err = os.WriteFile(filepath.Join(tempHome, "."+brand.Brand, "config.json"), []byte(`{"hub":{"repo":"git@github.com:graphit-labs/graphit-code.git"}}`), 0644)
	if err != nil {
		t.Fatalf("failed to write global mock config: %v", err)
	}

	// Initialize the project
	out, err := executeCommand("init", "--id", "01H2PJX...", "--name", "test-project", "--description", "test project description", "--ide", "cursor")
	if err != nil {
		t.Errorf("failed to initialize project: %v, output: %s", err, out)
	}

	// Test config set
	out, err = executeCommand("config", "ide", "cursor")
	if err != nil {
		t.Errorf("failed to set project config: %v", err)
	}
	if !strings.Contains(out, "Set ide = cursor (project)") {
		t.Errorf("unexpected output when setting project config: %s", out)
	}

	// Test config get
	out, err = executeCommand("config", "--get", "ide")
	if err != nil {
		t.Errorf("failed to get project config: %v", err)
	}
	if !strings.Contains(out, "cursor") {
		t.Errorf("expected cursor in get output, got: %s", out)
	}

	// Test config list
	out, err = executeCommand("config", "--list")
	if err != nil {
		t.Errorf("failed to list project config: %v", err)
	}
	if !strings.Contains(out, "ide:") || !strings.Contains(out, "cursor") {
		t.Errorf("expected ide: cursor in list output, got: %s", out)
	}

	// Test config unset
	out, err = executeCommand("config", "--unset", "ide")
	if err != nil {
		t.Errorf("failed to unset project config: %v", err)
	}
	if !strings.Contains(out, "Unset ide (project)") {
		t.Errorf("unexpected output when unsetting project config: %s", out)
	}
}

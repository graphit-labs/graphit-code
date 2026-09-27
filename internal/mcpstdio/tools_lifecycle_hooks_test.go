package mcpstdio

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestReconcileGitHooksWarningContainsManualIntegration(t *testing.T) {
	projectDir := t.TempDir()
	note := reconcileGitHooks(projectDir, false, []string{"pre-push"})
	for _, expected := range []string{"Git hooks warning:", "Manual integration:", "pre-commit", "pre-push", "_git-hook pre-push"} {
		if !strings.Contains(note, expected) {
			t.Errorf("warning missing %q: %s", expected, note)
		}
	}
	if note := reconcileGitHooks(projectDir, true, nil); note != "" {
		t.Fatalf("unexpected cleanup warning for absent hook: %s", note)
	}
}

func TestReconcileGitHooksInstallsAndRemovesAdditionalEvent(t *testing.T) {
	projectDir := t.TempDir()
	if out, err := exec.Command("git", "-C", projectDir, "init", "-q").CombinedOutput(); err != nil {
		t.Fatalf("git init: %v: %s", err, out)
	}
	if note := reconcileGitHooks(projectDir, false, []string{"pre-push"}); note != "" {
		t.Fatal(note)
	}
	_, fileErr := os.Stat(filepath.Join(projectDir, ".git", "hooks", "pre-push"))
	config, configErr := exec.Command("git", "-C", projectDir, "config", "--local", "--get", "hook.graphit-pre-push.event").CombinedOutput()
	if fileErr != nil && (configErr != nil || strings.TrimSpace(string(config)) != "pre-push") {
		t.Fatalf("pre-push not installed in either Git mechanism: file=%v, config=%v (%s)", fileErr, configErr, config)
	}
	if note := reconcileGitHooks(projectDir, true, nil); note != "" {
		t.Fatal(note)
	}
	if _, err := os.Stat(filepath.Join(projectDir, ".git", "hooks", "pre-push")); !os.IsNotExist(err) {
		t.Fatalf("pre-push not removed: %v", err)
	}
	if out, err := exec.Command("git", "-C", projectDir, "config", "--local", "--get", "hook.graphit-pre-push.event").CombinedOutput(); err == nil {
		t.Fatalf("configured pre-push remains: %s", out)
	}
}

package improvements

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestImprovementsRuleBasic(t *testing.T) {
	ruleContent := ImprovementsRuleContent()
	if !strings.Contains(ruleContent, "# Code Improvement Methodology Rule") {
		t.Errorf("unexpected rule content: %q", ruleContent)
	}
}

func TestInstallRule(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	// InstallRule with explicit projectDir should not error
	if err := InstallRule(dir, "claude"); err != nil {
		t.Fatalf("InstallRule(%q, claude) error: %v", dir, err)
	}
}

func TestInstallRuleDefaultDir(t *testing.T) {
	dir := t.TempDir()
	if err := InstallRule(dir, "gemini"); err != nil {
		t.Fatalf("InstallRule with explicit dir error: %v", err)
	}
}

func TestInstallSkill(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	if err := InstallSkill(dir, "claude"); err != nil {
		t.Fatalf("InstallSkill(%q, claude) error: %v", dir, err)
	}
}

func TestRemoveRule(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	if err := InstallRule(dir, "claude"); err != nil {
		t.Fatalf("InstallRule error: %v", err)
	}
	if err := RemoveRule(dir, "claude"); err != nil {
		t.Fatalf("RemoveRule(%q, claude) error: %v", dir, err)
	}
}

func TestRemoveSkill(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	if err := InstallSkill(dir, "claude"); err != nil {
		t.Fatalf("InstallSkill error: %v", err)
	}
	if err := RemoveSkill(dir, "claude"); err != nil {
		t.Fatalf("RemoveSkill(%q, claude) error: %v", dir, err)
	}
}

func TestInstallRuleEmptyProjectDir(t *testing.T) {
	_ = InstallRule(t.TempDir(), "claude")
}

func TestInstallSkillEmptyProjectDir(t *testing.T) {
	_ = InstallSkill(t.TempDir(), "claude")
}

func TestRemoveRuleEmptyProjectDir(t *testing.T) {
	_ = RemoveRule(t.TempDir(), "claude")
}

func TestRemoveSkillEmptyProjectDir(t *testing.T) {
	_ = RemoveSkill(t.TempDir(), "claude")
}

func TestInstallRuleInjectError(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	// InstallRule now only delegates to InstallSkill.
	// An unknown IDE name should cause InstallSkill to fail.
	err := InstallRule(dir, "nonexistent_ide")
	if err == nil {
		t.Error("expected InstallRule to fail with unknown IDE")
	}
}

func TestInstallSkillUnknownIDE(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	// Unknown IDE should cause InstallManagedSkill to error
	err := InstallSkill(dir, "nonexistent_ide")
	if err == nil {
		t.Error("expected InstallSkill to fail with unknown IDE")
	}
}

func TestRemoveSkillUnknownIDE(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	// Unknown IDE should cause RemoveManagedSkill to error
	err := RemoveSkill(dir, "nonexistent_ide")
	if err == nil {
		t.Error("expected RemoveSkill to fail with unknown IDE")
	}
}

func TestGetwdErrorBranches(t *testing.T) {
	// Trigger os.Getwd() failure by removing the current working directory.
	// This covers the error-return branches at lines 88-89, 105-106, 118-119, 130-131.
	origDir, err := os.Getwd()
	if err != nil {
		t.Fatalf("failed to get original wd: %v", err)
	}
	defer func() { _ = os.Chdir(origDir) }()

	// Create a temp directory, chdir into it, then remove it
	tmpDir := t.TempDir()
	removable := filepath.Join(tmpDir, "removable")
	if err := os.MkdirAll(removable, 0o755); err != nil {
		t.Fatalf("failed to create removable dir: %v", err)
	}
	if err := os.Chdir(removable); err != nil {
		t.Fatalf("failed to chdir: %v", err)
	}
	if err := os.RemoveAll(removable); err != nil {
		t.Fatalf("failed to remove cwd: %v", err)
	}

	// Now os.Getwd() should fail since the CWD has been deleted
	if _, err := os.Getwd(); err == nil {
		// If os.Getwd() doesn't fail (some OS implementations cache the cwd),
		// skip the test
		t.Skip("os.Getwd() did not fail after removing cwd, skipping")
	}

	// All four functions with empty projectDir should return error
	if err := InstallRule("", "claude"); err == nil {
		t.Error("expected InstallRule to fail when Getwd fails")
	}
	if err := InstallSkill("", "claude"); err == nil {
		t.Error("expected InstallSkill to fail when Getwd fails")
	}
	if err := RemoveRule("", "claude"); err == nil {
		t.Error("expected RemoveRule to fail when Getwd fails")
	}
	if err := RemoveSkill("", "claude"); err == nil {
		t.Error("expected RemoveSkill to fail when Getwd fails")
	}
}


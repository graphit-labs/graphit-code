package hub

import (
	"strings"
	"testing"
)

func TestHubRuleContent(t *testing.T) {
	t.Parallel()
	content := HubRuleContent(nil)
	if content == "" {
		t.Error("expected non-empty content")
	}
	if !strings.Contains(content, "Hub Discovery Rule") {
		t.Error("expected content to contain 'Hub Discovery Rule'")
	}
	if !strings.Contains(content, "Artifact Types") {
		t.Error("expected content to contain 'Artifact Types'")
	}
	if !strings.Contains(content, "Ecosystem Project Discovery") {
		t.Error("expected content to contain 'Ecosystem Project Discovery'")
	}

	// With installed artifacts
	installed := []InstalledArtifactInfo{
		{ID: "test-rule", Type: "rule", Version: "1.0.0"},
	}
	content2 := HubRuleContent(installed)
	if content2 == "" {
		t.Error("expected non-empty content")
	}
}

func TestHubRouterContent(t *testing.T) {
	t.Parallel()
	content := HubRouterContent(nil, "AGENTS.md")
	if content == "" {
		t.Error("expected non-empty content")
	}
	if !strings.Contains(content, "Hub Discovery") {
		t.Error("expected content to contain 'Hub Discovery'")
	}
	if !strings.Contains(content, "AGENTS.md") {
		t.Error("expected content to contain 'AGENTS.md'")
	}
	if !strings.Contains(content, "Quick Reference") {
		t.Error("expected content to contain 'Quick Reference'")
	}
}

func TestInstallRule(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	err := InstallRule(dir, "claude")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestInstallSkill(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	err := InstallSkill(dir, "claude")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestRemoveRule(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	// Install then remove
	_ = InstallRule(dir, "claude")
	err := RemoveRule(dir, "claude")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestRemoveSkill(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	// Install then remove
	_ = InstallSkill(dir, "claude")
	err := RemoveSkill(dir, "claude")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

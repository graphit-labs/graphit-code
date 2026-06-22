package hub

import (
	"strings"
	"testing"
)

func TestHubRuleContent(t *testing.T) {
	t.Parallel()
	content := HubRuleContent()
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

package hub

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestInstalledRuleContextReadsRulesFromTheLockfileInStableOrder(t *testing.T) {
	projectDir := t.TempDir()
	alpha := filepath.Join(t.TempDir(), "alpha")
	zeta := filepath.Join(t.TempDir(), "zeta")
	for _, dir := range []string{alpha, zeta} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(alpha, "RULE.md"), []byte("---\nname: alpha\n---\n\nAlpha body"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(zeta, "RULE.md"), []byte("Zeta body"), 0o644); err != nil {
		t.Fatal(err)
	}
	lf := &Lockfile{Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
		TypeRule: {
			"zeta":  {LinkSource: zeta},
			"alpha": {LinkSource: alpha},
		},
	}}
	if err := SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), lf); err != nil {
		t.Fatal(err)
	}

	context, err := InstalledRuleContext(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(context, "name: alpha") {
		t.Fatalf("artifact frontmatter leaked into model context: %s", context)
	}
	if !strings.Contains(context, "Alpha body") || !strings.Contains(context, "Zeta body") {
		t.Fatalf("rule bodies missing: %s", context)
	}
	if strings.Index(context, "Hub rule: alpha") >= strings.Index(context, "Hub rule: zeta") {
		t.Fatalf("rules are not deterministically sorted: %s", context)
	}
}

func TestInstalledRuleContextReportsUnreadableClaimsWithoutDroppingReadableRules(t *testing.T) {
	projectDir := t.TempDir()
	readable := filepath.Join(t.TempDir(), "readable")
	if err := os.MkdirAll(readable, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(readable, "RULE.md"), []byte("Readable body"), 0o644); err != nil {
		t.Fatal(err)
	}
	lf := &Lockfile{Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
		TypeRule: {
			"readable": {LinkSource: readable},
			"missing":  {LinkSource: filepath.Join(t.TempDir(), "missing")},
		},
	}}
	if err := SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), lf); err != nil {
		t.Fatal(err)
	}

	context, err := InstalledRuleContext(projectDir)
	if err == nil || !strings.Contains(err.Error(), "missing") {
		t.Fatalf("missing rule was not reported: %v", err)
	}
	if !strings.Contains(context, "Readable body") {
		t.Fatalf("readable rule was discarded with the failed one: %s", context)
	}
}

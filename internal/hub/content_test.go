package hub

import (
	"context"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/store"
)

func contentHome(t *testing.T) string {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	return home
}

// writeArtifactClone builds a clone directory the way an install leaves one, and
// registers it globally so the content tool can find it.
func writeArtifactClone(t *testing.T, artType ArtifactType, id, version string, files map[string]string) string {
	t.Helper()
	dir := filepath.Join(t.TempDir(), "clone")
	for rel, body := range files {
		full := filepath.Join(dir, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("setup: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o600); err != nil {
			t.Fatalf("setup: %v", err)
		}
	}

	mgr, err := NewGlobalLockManager()
	if err != nil {
		t.Fatalf("setup: %v", err)
	}
	if _, err := mgr.RegisterInstall(InstallRecord{
		ID:        id,
		Version:   version,
		Type:      artType,
		Name:      id,
		CachePath: dir,
		Owner:     store.GlobalOwnerKey,
		LocalPath: dir,
	}); err != nil {
		t.Fatalf("setup: %v", err)
	}
	return dir
}

// contentService is a service with the global lock wired up and no registry: reading an
// installed artifact's files must not need the Hub, because nothing is downloaded.
func contentService(t *testing.T) *HubService {
	t.Helper()
	mgr, err := NewGlobalLockManager()
	if err != nil {
		t.Fatalf("setup: %v", err)
	}
	return &HubService{lockMgr: mgr}
}

func keysOf(m map[string]string) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// A skill is the case the path-keyed shape exists for: one artifact, several files that
// refer to each other by name.
func TestAMultiFileSkillReturnsEveryFileKeyedByPath(t *testing.T) {
	contentHome(t)
	writeArtifactClone(t, TypeSkill, "demo-skill", "1.0.0", map[string]string{
		"SKILL.md":               "# Demo skill\nSee reference/patterns.md",
		"reference/patterns.md":  "## Patterns",
		"reference/deep/more.md": "## More",
	})

	got, err := contentService(t).ArtifactContentFor(context.Background(), "", "demo-skill@1.0.0", "", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	want := []string{"SKILL.md", "reference/deep/more.md", "reference/patterns.md"}
	if diff := keysOf(got.Files); !equalStrings(diff, want) {
		t.Errorf("keys = %v, want %v", diff, want)
	}
	if got.Files["SKILL.md"] != "# Demo skill\nSee reference/patterns.md" {
		t.Errorf("SKILL.md = %q", got.Files["SKILL.md"])
	}
	if got.Canonical != "SKILL.md" {
		t.Errorf("canonical = %q, want SKILL.md — the caller needs to know which file to read first", got.Canonical)
	}
	if got.Version != "1.0.0" {
		t.Errorf("version = %q, want 1.0.0", got.Version)
	}
	// Keys are slash-separated so the answer reads the same on every platform.
	for k := range got.Files {
		if strings.Contains(k, "\\") {
			t.Errorf("key %q uses a backslash; keys must be slash-separated", k)
		}
	}
}

func TestASinglePathReturnsOneEntry(t *testing.T) {
	contentHome(t)
	writeArtifactClone(t, TypeSkill, "demo-skill", "1.0.0", map[string]string{
		"SKILL.md":              "# Demo skill",
		"reference/patterns.md": "## Patterns",
	})

	got, err := contentService(t).ArtifactContentFor(context.Background(), "", "demo-skill@1.0.0", "", "reference/patterns.md")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !equalStrings(keysOf(got.Files), []string{"reference/patterns.md"}) {
		t.Errorf("keys = %v, want just the requested path", keysOf(got.Files))
	}
}

// This tool must not become a way to read arbitrary files off the machine running the
// server.
func TestAPathOutsideTheArtifactIsRefused(t *testing.T) {
	contentHome(t)
	writeArtifactClone(t, TypeRule, "demo-rule", "1.0.0", map[string]string{"RULE.md": "# Rule"})

	for _, bad := range []string{"../escape.md", "../../etc/passwd", "/etc/passwd"} {
		if _, err := contentService(t).ArtifactContentFor(context.Background(), "", "demo-rule@1.0.0", "", bad); err == nil {
			t.Errorf("path %q was accepted; it must be refused", bad)
		}
	}
}

func TestTheFourContentTypesAreServed(t *testing.T) {
	for _, tc := range []struct {
		artType   ArtifactType
		canonical string
	}{
		{TypeRule, "RULE.md"},
		{TypeSkill, "SKILL.md"},
		{TypeCommand, "COMMAND.md"},
		{TypeAgent, "AGENT.md"},
	} {
		t.Run(string(tc.artType), func(t *testing.T) {
			contentHome(t)
			writeArtifactClone(t, tc.artType, "demo", "1.0.0", map[string]string{tc.canonical: "# body"})

			got, err := contentService(t).ArtifactContentFor(context.Background(), "", "demo@1.0.0", tc.artType, "")
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got.Canonical != tc.canonical {
				t.Errorf("canonical = %q, want %q", got.Canonical, tc.canonical)
			}
			if got.Type != tc.artType {
				t.Errorf("type = %q, want %q", got.Type, tc.artType)
			}
		})
	}
}

// A mounted type has no file tree, and saying so by name is the whole point: an empty
// answer would read as "the artifact is empty".
func TestMountedTypesAreRefusedByNameWithTheRightToolNamed(t *testing.T) {
	contentHome(t)

	for artType, wantMention := range map[ArtifactType]string{
		TypeAST:       "ast source",
		TypeKnowledge: "wiki source",
	} {
		writeArtifactClone(t, artType, "demo-"+string(artType), "1.0.0", map[string]string{"anything.md": "x"})

		_, err := contentService(t).ArtifactContentFor(context.Background(), "",
			"demo-"+string(artType)+"@1.0.0", artType, "")
		if err == nil {
			t.Fatalf("%s: expected a refusal", artType)
		}
		if !strings.Contains(err.Error(), wantMention) {
			t.Errorf("%s: error must point at %q, got: %v", artType, wantMention, err)
		}
	}
}

func TestAnArtifactThatIsNotInstalledNamesTheMissingStep(t *testing.T) {
	contentHome(t)

	_, err := contentService(t).ArtifactContentFor(context.Background(), "", "demo-skill@1.0.0", "", "")
	if err == nil {
		t.Fatal("expected a refusal")
	}
	if !strings.Contains(err.Error(), "not installed globally") || !strings.Contains(err.Error(), "install") {
		t.Errorf("the error must name installing as the missing step, got: %v", err)
	}
}

// Two versions installed globally are two different stores. Picking one silently is a
// mistake the caller cannot see, so an ambiguous reference is refused and both are named.
func TestAnAmbiguousReferenceIsRefusedAndListsTheCandidates(t *testing.T) {
	contentHome(t)
	writeArtifactClone(t, TypeSkill, "demo-skill", "1.0.0", map[string]string{"SKILL.md": "old"})
	writeArtifactClone(t, TypeSkill, "demo-skill", "2.0.0", map[string]string{"SKILL.md": "new"})

	svc := contentService(t)
	_, err := svc.ArtifactContentFor(context.Background(), "", "demo-skill", "", "")
	if err == nil {
		t.Fatal("expected a refusal for an ambiguous reference")
	}
	for _, want := range []string{"1.0.0", "2.0.0", "@version"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("error must mention %q, got: %v", want, err)
		}
	}

	// Qualified, it resolves.
	got, err := svc.ArtifactContentFor(context.Background(), "", "demo-skill@2.0.0", "", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.Files["SKILL.md"] != "new" {
		t.Errorf("SKILL.md = %q, want the pinned version's content", got.Files["SKILL.md"])
	}
}

// An artifact may carry an asset beside its markdown. Its bytes are useless to a model,
// but its presence is information — so the path is reported and the content is not.
func TestANonTextFileIsListedRatherThanReturned(t *testing.T) {
	contentHome(t)
	dir := writeArtifactClone(t, TypeSkill, "demo-skill", "1.0.0", map[string]string{"SKILL.md": "# Demo"})
	if err := os.MkdirAll(filepath.Join(dir, "assets"), 0o755); err != nil {
		t.Fatalf("setup: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "assets", "logo.png"),
		[]byte{0x89, 'P', 'N', 'G', 0x00, 0xFF, 0xFE}, 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}

	got, err := contentService(t).ArtifactContentFor(context.Background(), "", "demo-skill@1.0.0", "", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.Files["SKILL.md"] != "# Demo" {
		t.Errorf("SKILL.md = %q", got.Files["SKILL.md"])
	}
	marker, ok := got.Files["assets/logo.png"]
	if !ok {
		t.Fatal("the binary file must still be listed — its presence is part of the artifact")
	}
	if !strings.Contains(marker, "not text") {
		t.Errorf("assets/logo.png = %q, want a marker saying it is not text", marker)
	}
}

// An unqualified reference to a single global install is legitimate, and the answer says
// which version it read so the caller is not guessing.
func TestAnUnqualifiedReferenceReportsTheVersionItRead(t *testing.T) {
	contentHome(t)
	writeArtifactClone(t, TypeRule, "demo-rule", "3.1.4", map[string]string{"RULE.md": "# Rule"})

	got, err := contentService(t).ArtifactContentFor(context.Background(), "", "demo-rule", "", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.Version != "3.1.4" {
		t.Errorf("version = %q, want 3.1.4", got.Version)
	}
	if !strings.Contains(got.Notice, "3.1.4") {
		t.Errorf("notice = %q, want it to name the version that was read", got.Notice)
	}
}

func equalStrings(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

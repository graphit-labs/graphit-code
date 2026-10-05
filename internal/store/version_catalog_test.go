package store

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestVersionFromPathSegmentReversesTheEncoding(t *testing.T) {
	for _, version := range []string{"main", "feature/x", "release-1.2", "con", "aux.v2", "feature/ação", ""} {
		segment := VersionPathSegment(version)
		if got := VersionFromPathSegment(segment); got != version {
			t.Errorf("%q -> %q -> %q", version, segment, got)
		}
	}
}

func TestLocalVersionsListsTheVersionsWithAStore(t *testing.T) {
	withHome(t)
	t.Setenv("GRAPHIT_PROJECT_VERSION", "")
	project := t.TempDir()
	writeLockfile(t, project, storeTestProjectID)
	for _, dir := range []string{
		VersionStoreDir(ASTProjectRoot(project), "main"),
		VersionStoreDir(ASTProjectRoot(project), "feature/x"),
		VersionStoreDir(KnowledgeProjectRoot(project), "feature/x"),
		VersionStoreDir(KnowledgeProjectRoot(project), "docs-only"),
		filepath.Join(ASTProjectRoot(project), "versions", ".fork-tmp"),
	} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	want := map[string][]string{
		"main":      {"ast"},
		"feature/x": {"ast", "knowledge"},
		"docs-only": {"knowledge"},
	}
	if got := LocalVersions(project); !reflect.DeepEqual(got, want) {
		t.Fatalf("LocalVersions = %v, want %v", got, want)
	}
}

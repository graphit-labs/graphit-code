package knowledge

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func seedContext(t *testing.T, projectDir, name string) string {
	t.Helper()
	dir := store.KnowledgeContextDir(name)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("setup: %v", err)
	}
	if err := store.AddContext(projectDir, store.KindKnowledge, store.ContextRecord{Name: name}); err != nil {
		t.Fatalf("setup: %v", err)
	}
	return dir
}

func newProjectDir(t *testing.T, ephemeral bool) string {
	t.Helper()
	dir := t.TempDir()
	body := `{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`
	if ephemeral {
		body = `{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","ephemeral":true}}`
	}
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(body), 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}
	return dir
}

func TestANormalProjectReadsItsOwnWiki(t *testing.T) {
	isolateHome(t)
	project := newProjectDir(t, false)
	seedContext(t, project, "acme-docs")

	got := ReadDirIn(project, "")
	if want := WikiDirForContextIn(project, ""); got != want {
		t.Errorf("ReadDirIn = %q, want the project wiki %q — installing a context must not change what an unqualified read means", got, want)
	}
}

func TestAnEphemeralSessionHasNoWikiToReadWithoutNamingOne(t *testing.T) {
	isolateHome(t)
	session := newProjectDir(t, true)
	seedContext(t, session, "alpha-docs")
	seedContext(t, session, "beta-docs")

	if got := ReadDirIn(session, ""); got != "" {
		t.Fatalf("an unqualified read over a session resolved to %q, want nothing", got)
	}
}

func TestNamingAContextWorksForEitherKindOfProject(t *testing.T) {
	isolateHome(t)
	for _, ephemeral := range []bool{false, true} {
		project := newProjectDir(t, ephemeral)
		seedContext(t, project, "acme-docs")

		got := ReadDirIn(project, "acme-docs")
		if want := store.KnowledgeContextDir("acme-docs"); got != want {
			t.Errorf("ephemeral=%v: ReadDirIn = %q, want the context store %q", ephemeral, got, want)
		}
	}
}

func TestASessionWithNothingSelectedStillReadsNothing(t *testing.T) {
	isolateHome(t)
	session := newProjectDir(t, true)

	if got := ReadDirIn(session, ""); got != "" {
		t.Fatalf("ReadDirIn = %q, want nothing", got)
	}
	if got := InstalledContextsIn(session); len(got) != 0 {
		t.Fatalf("InstalledContextsIn = %v, want none", got)
	}
}

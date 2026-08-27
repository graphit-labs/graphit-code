package mcpstdio

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func ephemeralWorkspace(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	body := `{"project":{"id":"01SESSION","name":"live-search-01session","ephemeral":true}}`
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(body), 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}
	return dir
}

func realProject(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	body := `{"project":{"id":"01REALPROJECT"}}`
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(body), 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}
	return dir
}

func TestAnEphemeralSessionResolvesNoKnowledgeWikiOfItsOwn(t *testing.T) {
	ws := ephemeralWorkspace(t)
	if got := resolveWikiDir("knowledge", ws, ""); got != "" {
		t.Errorf("resolveWikiDir = %q; a session has no documentation wiki of its own", got)
	}
	// Naming a context still resolves — that is the only way in.
	if got := resolveWikiDir("knowledge", ws, "acme-docs"); got != store.KnowledgeContextDir("acme-docs") {
		t.Errorf("a named context resolved to %q, want its context store", got)
	}
}

func TestARealProjectStillResolvesItsKnowledgeWiki(t *testing.T) {
	project := realProject(t)
	if got := resolveWikiDir("knowledge", project, ""); got == "" {
		t.Error("a real project must still resolve its own wiki")
	}
}

func TestAnEphemeralSessionsProjectMemoryIsServedFromTheUserScope(t *testing.T) {
	// The redirect exists because opening a project scope CREATES it, and creating it
	// means an orphan branch and a worktree in the shared memory repository named
	// after a session that exists for one search.
	ws := ephemeralWorkspace(t)

	if notice := memoryScopeNotice(false, ws); notice == "" {
		t.Error("asking a session for project memory must be reported as redirected")
	}
	if notice := memoryScopeNotice(true, ws); notice != "" {
		t.Errorf("asking for user memory explicitly is not a redirect, got %q", notice)
	}
	if notice := memoryScopeNotice(false, realProject(t)); notice != "" {
		t.Errorf("a real project's project scope is not a redirect, got %q", notice)
	}
}

func TestTheMemoryWikiRedirectAgreesWithTheScopeRedirect(t *testing.T) {
	// If these two disagree, a search returns user memory slugs and reading one back
	// resolves to a directory that does not exist.
	ws := ephemeralWorkspace(t)
	viaProject := resolveWikiDir("memory", ws, "project")
	viaUser := resolveWikiDir("memory", ws, "user")
	if viaProject != viaUser {
		t.Errorf("project scope resolved to %q and user scope to %q; they must be the same for a session", viaProject, viaUser)
	}
}

func TestAnEphemeralSessionIsRefusedItsOwnCodeGraph(t *testing.T) {
	ws := ephemeralWorkspace(t)

	if _, err := openASTDBReadWrite(ws, ""); err == nil {
		t.Fatal("opening a session's own graph read-write must be refused: opening it is what creates it")
	} else if !strings.Contains(err.Error(), "context") {
		t.Errorf("the refusal should name the way out, got %q", err)
	}
}

func TestAnEphemeralSessionCanStillWriteAnImportedContext(t *testing.T) {
	// Installing and querying contexts is most of what a live search does, so the
	// refusal must be about the project's own graph and nothing else.
	ws := ephemeralWorkspace(t)
	db, err := openASTDBReadWrite(ws, "some-context")
	if err != nil {
		t.Fatalf("a named context must remain writable: %v", err)
	}
	if db != nil {
		_ = db.Close()
	}
}

func TestARealProjectCanStillOpenItsGraphReadWrite(t *testing.T) {
	db, err := openASTDBReadWrite(realProject(t), "")
	if err != nil {
		t.Fatalf("a real project must still open its own graph: %v", err)
	}
	if db != nil {
		_ = db.Close()
	}
}

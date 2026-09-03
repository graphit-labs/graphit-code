package mcpstdio

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func TestResolveProjectDirOptionalAcceptsAbsenceAndStillRejectsNonsense(t *testing.T) {
	got, err := resolveProjectDirOptional("")
	if err != nil {
		t.Fatalf("an absent project_dir must be accepted: %v", err)
	}
	if got != "" {
		t.Errorf("resolveProjectDirOptional(\"\") = %q, want the empty global scope", got)
	}

	if _, err := resolveProjectDirOptional(filepath.Join(t.TempDir(), "nope")); err == nil {
		t.Error("a project_dir that does not exist must still be refused")
	}
}

// A read that has neither a project nor a context has nothing to answer about, and must
// say so rather than resolving a store from an empty path.
func TestAProjectlessReadMustNameTheArtifact(t *testing.T) {
	if _, err := resolveArtifactScope("", ""); err == nil {
		t.Fatal("expected a refusal when both project_dir and context are absent")
	} else if !strings.Contains(err.Error(), "qualified identifier") {
		t.Errorf("the error must tell the caller what to pass instead, got: %v", err)
	}

	if got, err := resolveArtifactScope("", "demo-ast@1.0.0"); err != nil || got != "" {
		t.Errorf("resolveArtifactScope with a context = (%q, %v), want the global scope", got, err)
	}
}

// The memory wiki needs no project — its user scope is keyed by the machine — while the
// knowledge wiki does, unless a context names the artifact instead.
func TestResolveWikiScopeSeparatesTheTwoWikis(t *testing.T) {
	cases := []struct {
		name      string
		wiki      string
		context   string
		wantError bool
	}{
		{"memory needs nothing", "memory", "", false},
		{"knowledge needs a context", "knowledge", "", true},
		{"the default scope is knowledge", "", "", true},
		{"knowledge with a context is fine", "", "demo-kb@1.0.0", false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			_, err := resolveWikiScope("", c.wiki, c.context)
			if (err != nil) != c.wantError {
				t.Errorf("resolveWikiScope(\"\", %q, %q) error = %v, wantError = %v",
					c.wiki, c.context, err, c.wantError)
			}
		})
	}
}

// Opening read-write CREATES the store. With no project there is no identity to key one
// by, so the store would be filed under the hash of an empty path where nothing would
// find it again and nothing would reclaim it.
func TestTheGlobalScopeRefusesToCreateAGraph(t *testing.T) {
	t.Setenv("LADYBUGDB_PATH", "")
	isolateHome(t)

	if _, err := openASTDBReadWrite("", "demo-ast@1.0.0"); err == nil {
		t.Fatal("expected a refusal: the global scope must not create a code graph")
	} else if !strings.Contains(err.Error(), "needs a project") {
		t.Errorf("the error must say a project is needed, got: %v", err)
	}
}

// A project's configuration must not leak into a request that named no project.
// filepath.Join("", "<lockfile>") is RELATIVE, so without the guard this reads the
// lockfile of whatever directory the server is running in.
func TestTheGlobalScopeReadsNoProjectConfiguration(t *testing.T) {
	isolateHome(t)

	bystander := t.TempDir()
	if err := os.WriteFile(filepath.Join(bystander, brand.LockFileName()),
		[]byte(`{"project":{"id":"01BYSTANDER"},"config":{"hub.bucket":"someone-elses-bucket"},"ides":["claude"]}`),
		0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}
	t.Chdir(bystander)

	if cfg := loadProjectConfig(""); cfg != nil {
		t.Errorf("loadProjectConfig(\"\") = %v, want nil — it read the working directory's project", cfg)
	}
	if cfg, ides := loadProjectLockInfo(""); cfg != nil || ides != nil {
		t.Errorf("loadProjectLockInfo(\"\") = (%v, %v), want (nil, nil)", cfg, ides)
	}
	if cfg := loadProjectConfig(bystander); cfg == nil {
		t.Fatal("setup is wrong: the bystander project's config should be readable when named")
	}
}

// A project memory scope is keyed by a project identity. With none, the request is served
// from the user scope — which is a real scope, not a fallback — and the caller is told.
func TestProjectlessMemoryIsServedFromTheUserScope(t *testing.T) {
	isolateHome(t)

	scope, scopeID, redirected, err := memoryScopeFor(false, "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !redirected {
		t.Error("the redirect must be reported, or the tool answers a different question silently")
	}
	if string(scope) == "project" {
		t.Errorf("scope = %q, want the user scope", scope)
	}
	if scopeID == "" {
		t.Error("the user scope must resolve to an id derived from the machine")
	}
	if notice := memoryScopeNotice(false, ""); !strings.Contains(notice, "no project_dir") {
		t.Errorf("notice = %q, want it to name the absent project_dir", notice)
	}
	if notice := memoryScopeNotice(true, ""); notice != "" {
		t.Errorf("notice = %q, want none when the user scope was requested", notice)
	}
}

// anchorToProject exists to make a relative path absolute. In the global scope there is
// nothing to anchor to, and joining with "" would leave it relative — the exact state the
// function exists to remove.
func TestAnchorToProjectLeavesTheGlobalScopeAlone(t *testing.T) {
	if got := anchorToProject("", "some/relative/path"); got != "some/relative/path" {
		t.Errorf("anchorToProject = %q, want the path unchanged", got)
	}
	abs := filepath.Join(string(filepath.Separator), "abs", "path")
	if got := anchorToProject("", abs); got != abs {
		t.Errorf("anchorToProject = %q, want %q", got, abs)
	}
}

// The wiki directory of a project-less memory request must resolve to the user scope, or
// a search returns user slugs and reading one back resolves to a directory that does not
// exist.
func TestProjectlessMemoryWikiDirResolvesToTheUserScope(t *testing.T) {
	isolateHome(t)

	got := resolveWikiDir("memory", "", "")
	if got == "" {
		t.Fatal("resolveWikiDir returned nothing for a project-less memory request")
	}
	if !strings.Contains(filepath.ToSlash(got), "/wiki/memory/user/") {
		t.Errorf("wiki dir = %q, want it under the user memory scope", got)
	}
	if got != resolveWikiDir("memory", "", "user") {
		t.Error("the implicit and explicit user scope must resolve to the same directory")
	}
}

// withProjectDir moves the process into the project it is given. There is nowhere to move
// to in the global scope, and moving anywhere would be worse than staying.
func TestWithProjectDirDoesNotMoveInTheGlobalScope(t *testing.T) {
	before, err := os.Getwd()
	if err != nil {
		t.Skip("no readable working directory")
	}
	ran := false
	if err := withProjectDir("", func() error {
		ran = true
		inside, gErr := os.Getwd()
		if gErr != nil {
			return gErr
		}
		if inside != before {
			t.Errorf("working directory changed to %q, want %q", inside, before)
		}
		return nil
	}); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !ran {
		t.Error("the function was never called")
	}
}

// The reserved owner key must agree between the package that writes the global lock and
// the package that reads it, or a global install is written where nothing looks for it.
func TestTheGlobalOwnerKeyIsTheReservedOne(t *testing.T) {
	if store.GlobalOwnerKey != "_global" {
		t.Errorf("GlobalOwnerKey = %q, want _global", store.GlobalOwnerKey)
	}
}

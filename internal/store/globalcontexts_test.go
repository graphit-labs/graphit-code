package store

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func writeGlobalLock(t *testing.T, body string) {
	t.Helper()
	root := brand.GlobalDir()
	if root == "" {
		t.Fatal("setup: no global dir — the test did not isolate HOME")
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		t.Fatalf("setup: %v", err)
	}
	if err := os.WriteFile(filepath.Join(root, globalLockFileName), []byte(body), 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}
}

func TestAQualifiedIdentifierResolvesToTheSharedVersionedStore(t *testing.T) {
	cases := []struct {
		name        string
		kind        string
		ref         string
		wantVersion string
	}{
		{"ast by id", KindAST, "demo-ast", "2.1.0"},
		{"ast by qualified id", KindAST, "demo-ast@2.1.0", "2.1.0"},
		{"knowledge by id", KindKnowledge, "demo-kb", "1.0.0"},
		{"knowledge by qualified id", KindKnowledge, "demo-kb@1.0.0", "1.0.0"},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			withHome(t)
			writeGlobalLock(t, `{
  "version": 2,
  "projects": {},
  "artifacts": {
    "ast/demo-ast@2.1.0": {"id":"demo-ast","version":"2.1.0","type":"ast","projects":{"_global":{"projectDir":""}}},
    "knowledge/demo-kb@1.0.0": {"id":"demo-kb","version":"1.0.0","type":"knowledge","projects":{"_global":{"projectDir":""}}}
  }
}`)

			rec, ok := LookupContext("", c.kind, c.ref)
			if !ok {
				t.Fatalf("LookupContext(%q, %q) not found", c.kind, c.ref)
			}
			if rec.Version != c.wantVersion {
				t.Errorf("version = %q, want %q", rec.Version, c.wantVersion)
			}
			if !rec.IsHub() {
				t.Error("a global install must resolve as a Hub context, or its store is looked for in the wrong place")
			}
		})
	}
}

// A global install is addressed by the PUBLISHING project when it has one — that is what
// keys the store — so a lookup has to accept both that name and the artifact id.
func TestAPublishedArtifactIsFoundByBothItsNames(t *testing.T) {
	withHome(t)
	writeGlobalLock(t, `{
  "version": 2,
  "artifacts": {
    "ast/demo-ast@3.0.0": {"id":"demo-ast","version":"3.0.0","type":"ast","projectId":"01PUBLISHER","projects":{"_global":{"projectDir":""}}}
  }
}`)

	for _, ref := range []string{"demo-ast", "demo-ast@3.0.0", "01PUBLISHER", "01PUBLISHER@3.0.0"} {
		rec, ok := LookupContext("", KindAST, ref)
		if !ok {
			t.Fatalf("LookupContext(%q) not found", ref)
		}
		if rec.Name != "01PUBLISHER" {
			t.Errorf("%s: name = %q, want the publishing project", ref, rec.Name)
		}
		want := ASTHubDir("01PUBLISHER", "3.0.0")
		if got := ASTContextIcebugDirIn("", ref); got != want {
			t.Errorf("%s: store = %q, want %q", ref, got, want)
		}
		if got := ASTContextDirIn("", ref); got != want {
			t.Errorf("%s: store dir = %q, want %q", ref, got, want)
		}
	}
}

// An install recorded only against real projects is in the global lock because that is
// where EVERY install is recorded. It is not globally installed, and answering a
// project-less query from it would hand out a store nobody asked to share.
func TestAProjectOnlyInstallIsNotGloballyInstalled(t *testing.T) {
	withHome(t)
	writeGlobalLock(t, `{
  "version": 2,
  "artifacts": {
    "ast/demo-ast@2.1.0": {"id":"demo-ast","version":"2.1.0","type":"ast","projects":{"01SOMEPROJECT":{"projectDir":"/somewhere"}}}
  }
}`)

	if _, ok := LookupContext("", KindAST, "demo-ast"); ok {
		t.Error("an install owned only by a project must not resolve in the global scope")
	}
	if names := GlobalContextNames(KindAST); len(names) != 0 {
		t.Errorf("GlobalContextNames = %v, want none", names)
	}
}

// The global fallback must NOT act as a second chance for a project. Membership is the
// point of the per-project record: a project that never installed an artifact must not
// reach its store.
func TestAProjectCannotReachAGlobalInstallItNeverClaimed(t *testing.T) {
	withHome(t)
	writeGlobalLock(t, `{
  "version": 2,
  "artifacts": {
    "ast/demo-ast@2.1.0": {"id":"demo-ast","version":"2.1.0","type":"ast","projects":{"_global":{"projectDir":""}}}
  }
}`)

	projectDir := t.TempDir()
	writeLockfile(t, projectDir, "01PROJECT")

	if _, ok := LookupContext(projectDir, KindAST, "demo-ast"); ok {
		t.Error("a project resolved a globally installed artifact it never claimed")
	}
}

func TestAnUnqualifiedReferenceTakesTheHighestInstalledVersion(t *testing.T) {
	withHome(t)
	writeGlobalLock(t, `{
  "version": 2,
  "artifacts": {
    "ast/demo-ast@1.9.0": {"id":"demo-ast","version":"1.9.0","type":"ast","projects":{"_global":{"projectDir":""}}},
    "ast/demo-ast@2.1.0": {"id":"demo-ast","version":"2.1.0","type":"ast","projects":{"_global":{"projectDir":""}}}
  }
}`)

	rec, ok := LookupContext("", KindAST, "demo-ast")
	if !ok {
		t.Fatal("not found")
	}
	if rec.Version != "2.1.0" {
		t.Errorf("version = %q, want 2.1.0 — an unqualified reference must not depend on map order", rec.Version)
	}

	pinned, ok := LookupContext("", KindAST, "demo-ast@1.9.0")
	if !ok || pinned.Version != "1.9.0" {
		t.Errorf("a qualified reference must still reach the older version, got %+v ok=%v", pinned, ok)
	}
}

func TestAMissingGlobalLockIsAnEmptyScopeNotAnError(t *testing.T) {
	withHome(t)
	if _, ok := LookupContext("", KindAST, "demo-ast"); ok {
		t.Error("resolved a context with no global lock present")
	}
	if got := ListContexts("", KindKnowledge); len(got) != 0 {
		t.Errorf("ListContexts = %v, want empty", got)
	}
}

func TestSplitQualified(t *testing.T) {
	t.Parallel()
	cases := map[string][2]string{
		"demo":            {"demo", ""},
		"demo@1.0.0":      {"demo", "1.0.0"},
		"demo@1.0.0-rc.1": {"demo", "1.0.0-rc.1"},
		"@scoped":         {"@scoped", ""},
		"@scoped@2.0.0":   {"@scoped", "2.0.0"},
	}
	for ref, want := range cases {
		id, version := SplitQualified(ref)
		if id != want[0] || version != want[1] {
			t.Errorf("SplitQualified(%q) = (%q, %q), want (%q, %q)", ref, id, version, want[0], want[1])
		}
	}
}

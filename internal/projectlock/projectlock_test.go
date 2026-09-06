package projectlock

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/oklog/ulid/v2"
)

func TestResolveProjectIdentity(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	identity := resolveProjectIdentity(dir)
	if identity.ID == "" {
		t.Error("expected non-empty ID")
	}
	if identity.Name == "" {
		t.Error("expected non-empty Name")
	}
	if !strings.Contains(identity.Name, filepath.Base(dir)) {
		t.Logf("Name %q from dir %q — may have come from a git remote", identity.Name, dir)
	}
}

func TestEnsureIdentityCreatesOneStableULIDUnderConcurrency(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, brand.LockFileName())
	const callers = 12
	ids := make(chan string, callers)
	errs := make(chan error, callers)
	var wg sync.WaitGroup
	for range callers {
		wg.Add(1)
		go func() {
			defer wg.Done()
			lf, err := EnsureIdentity(path)
			if err != nil {
				errs <- err
				return
			}
			ids <- lf.Project.ID
		}()
	}
	wg.Wait()
	close(ids)
	close(errs)
	for err := range errs {
		t.Errorf("EnsureIdentity: %v", err)
	}
	var want string
	for id := range ids {
		if _, err := ulid.ParseStrict(id); err != nil {
			t.Errorf("identity %q is not a ULID: %v", id, err)
		}
		if want == "" {
			want = id
		} else if id != want {
			t.Errorf("concurrent identity = %q, want %q", id, want)
		}
	}
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("lockfile was not created: %v", err)
	}
}

func TestSaveRejectsChangingAnExistingProjectID(t *testing.T) {
	path := filepath.Join(t.TempDir(), brand.LockFileName())
	if err := Save(path, &Lockfile{Project: ProjectIdentity{ID: "01FIRST"}}); err != nil {
		t.Fatal(err)
	}
	err := Save(path, &Lockfile{Project: ProjectIdentity{ID: "01SECOND"}})
	if err == nil || !strings.Contains(err.Error(), "immutable") {
		t.Fatalf("Save changed an existing project id: %v", err)
	}
}

func TestSourcePathRoundTripsThroughTheProject(t *testing.T) {
	project := filepath.Join(string(filepath.Separator), "home", "someone", "work", "app")
	sibling := filepath.Join(string(filepath.Separator), "home", "someone", "work", "lib")

	stored := RelSourcePath(project, sibling)
	if filepath.IsAbs(filepath.FromSlash(stored)) {
		t.Fatalf("stored path %q is absolute; it must be relative to the project", stored)
	}
	if strings.Contains(stored, "\\") {
		t.Errorf("stored path %q must be slash-separated on every platform", stored)
	}
	if got := SourceDir(project, stored); got != sibling {
		t.Errorf("SourceDir = %q, want %q", got, sibling)
	}
}

func TestASiblingOutsideTheProjectStaysRelative(t *testing.T) {
	project := filepath.Join(string(filepath.Separator), "w", "app")
	sibling := filepath.Join(string(filepath.Separator), "w", "lib")

	stored := RelSourcePath(project, sibling)
	if !strings.HasPrefix(stored, "../") {
		t.Errorf("stored = %q; a sibling outside the project is expected to climb out", stored)
	}
	if got := SourceDir(project, stored); got != sibling {
		t.Errorf("SourceDir = %q, want %q", got, sibling)
	}
}

func TestAnAbsoluteStoredPathIsStillHonoured(t *testing.T) {
	project := filepath.Join(string(filepath.Separator), "w", "app")
	abs := filepath.Join(string(filepath.Separator), "elsewhere", "lib")

	if got := SourceDir(project, filepath.ToSlash(abs)); got != abs {
		t.Errorf("SourceDir = %q, want the absolute path %q unchanged", got, abs)
	}
}

func TestNoSourcePathResolvesToNothing(t *testing.T) {
	if got := SourceDir(filepath.Join(string(filepath.Separator), "w", "app"), ""); got != "" {
		t.Errorf("SourceDir = %q, want empty", got)
	}
}

func TestLocalAndHubArtifactsAreToldApart(t *testing.T) {
	cases := []struct {
		name    string
		meta    ArtifactMeta
		isLocal bool
		isHub   bool
	}{
		{"a local import", ArtifactMeta{Origin: OriginLocal, Version: VersionLocal}, true, false},
		{"a link to a sibling", ArtifactMeta{Origin: OriginLink, Version: VersionLocal}, true, false},
		{"version local with no origin", ArtifactMeta{Version: VersionLocal}, true, false},
		{"a hub install", ArtifactMeta{Origin: OriginHub, Version: "1.2.0"}, false, true},
		{"a published artifact", ArtifactMeta{Origin: OriginPublish, Version: "1.0.0"}, false, false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := c.meta.IsLocal(); got != c.isLocal {
				t.Errorf("IsLocal = %v, want %v", got, c.isLocal)
			}
			if got := c.meta.IsHubInstalled(); got != c.isHub {
				t.Errorf("IsHubInstalled = %v, want %v", got, c.isHub)
			}
		})
	}
}

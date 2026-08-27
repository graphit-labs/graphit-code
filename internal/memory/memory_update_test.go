package memory

import (
	"strings"
	"testing"
)

// Updating a memory changes what it says, not what it is.
//
// The update path used to rebuild the frontmatter from the fields its author
// remembered — id, title, scope, scope_id, created_at, updated_at, and a hardcoded
// `tags: [memory]`. Everything else was dropped. An important `correction` tagged
// `auth,security` came back as an untyped memory with one tag, and nothing reported
// it, because the file was still valid and the body was still right. That is
// silent declassification, and it matters most for exactly the memories worth
// keeping: corrections and conventions are the ones agents edit as they learn more.
func TestUpdatedMemoryContentPreservesClassification(t *testing.T) {
	t.Parallel()

	original := renderMemoryFile(MemoryFrontmatter{
		ID:        "01AAAAAAAAAAAAAAAAAAAAAAAA",
		Title:     "Never bypass the service layer",
		Scope:     "project",
		ScopeID:   "proj-1",
		ProjectID: "origin-project",
		Type:      string(MemoryTypeCorrection),
		Important: true,
		CreatedAt: "2026-01-01T00:00:00Z",
		Tags:      []string{"memory", "project", "correction", "auth", "security"},
	}, "The original body.")

	updated := updatedMemoryContent(original, memoryUpdate{
		ID:        "01AAAAAAAAAAAAAAAAAAAAAAAA",
		Scope:     "project",
		ScopeID:   "proj-1",
		Important: true,
		NewBody:   "A fuller explanation.",
	})

	fm := ParseMemoryFrontmatter(updated)

	if fm.Type != string(MemoryTypeCorrection) {
		t.Errorf("type = %q; want it preserved as correction", fm.Type)
	}
	if !fm.Important {
		t.Error("importance must survive an update")
	}
	if fm.ProjectID != "origin-project" {
		t.Errorf("project_id = %q; want it preserved", fm.ProjectID)
	}
	if fm.CreatedAt != "2026-01-01T00:00:00Z" {
		t.Errorf("created_at = %q; want the original creation time", fm.CreatedAt)
	}
	for _, tag := range []string{"auth", "security", "correction"} {
		if !containsString(fm.Tags, tag) {
			t.Errorf("tag %q was dropped; tags = %v", tag, fm.Tags)
		}
	}
	if fm.Title != "Never bypass the service layer" {
		t.Errorf("title = %q; an empty new title must leave it alone", fm.Title)
	}
	if body := extractBodyAfterFrontmatter(updated); body != "A fuller explanation." {
		t.Errorf("body = %q; want the new body", body)
	}
	if fm.UpdatedAt == "2026-01-01T00:00:00Z" || fm.UpdatedAt == "" {
		t.Errorf("updated_at = %q; want it advanced", fm.UpdatedAt)
	}
}

// An update that only renames must not erase the body. The caller passing no body
// means "leave it", not "empty it".
func TestUpdatedMemoryContentKeepsBodyWhenOnlyTitleChanges(t *testing.T) {
	t.Parallel()

	original := renderMemoryFile(MemoryFrontmatter{
		ID: "01AAAAAAAAAAAAAAAAAAAAAAAA", Title: "Old title", Scope: "project",
		ScopeID: "p", Type: string(MemoryTypeFact), CreatedAt: "2026-01-01T00:00:00Z",
		Tags: []string{"memory", "project", "fact"},
	}, "Body worth keeping.")

	updated := updatedMemoryContent(original, memoryUpdate{
		ID: "01AAAAAAAAAAAAAAAAAAAAAAAA", Scope: "project", ScopeID: "p",
		NewTitle: "New title",
	})

	if body := extractBodyAfterFrontmatter(updated); body != "Body worth keeping." {
		t.Errorf("body = %q; want it preserved", body)
	}
	if fm := ParseMemoryFrontmatter(updated); fm.Title != "New title" {
		t.Errorf("title = %q; want New title", fm.Title)
	}
	// The H1 must follow the new title, not stay on the old one.
	if !strings.Contains(updated, "# New title") {
		t.Error("heading was not updated with the title")
	}
}

// Reclassification is explicit and moves the mirroring tag with it, so a memory
// does not stay searchable under a type it no longer has.
func TestUpdatedMemoryContentReclassifiesAndMovesTheTypeTag(t *testing.T) {
	t.Parallel()

	original := renderMemoryFile(MemoryFrontmatter{
		ID: "01AAAAAAAAAAAAAAAAAAAAAAAA", Title: "T", Scope: "project", ScopeID: "p",
		Type: string(MemoryTypeFact), CreatedAt: "2026-01-01T00:00:00Z",
		Tags: []string{"memory", "project", "fact", "keepme"},
	}, "b")

	updated := updatedMemoryContent(original, memoryUpdate{
		ID: "01AAAAAAAAAAAAAAAAAAAAAAAA", Scope: "project", ScopeID: "p",
		NewType: string(MemoryTypeCorrection),
	})

	fm := ParseMemoryFrontmatter(updated)
	if fm.Type != string(MemoryTypeCorrection) {
		t.Errorf("type = %q; want correction", fm.Type)
	}
	if containsString(fm.Tags, "fact") {
		t.Errorf("the old type tag should be gone; tags = %v", fm.Tags)
	}
	if !containsString(fm.Tags, "correction") {
		t.Errorf("the new type tag is missing; tags = %v", fm.Tags)
	}
	if !containsString(fm.Tags, "keepme") {
		t.Errorf("an unrelated tag was dropped; tags = %v", fm.Tags)
	}
}

// An invalid type is ignored rather than written: a typo from a model must not
// produce a memory with a type nothing recognises.
func TestUpdatedMemoryContentIgnoresInvalidType(t *testing.T) {
	t.Parallel()

	original := renderMemoryFile(MemoryFrontmatter{
		ID: "01AAAAAAAAAAAAAAAAAAAAAAAA", Title: "T", Scope: "project", ScopeID: "p",
		Type: string(MemoryTypeDecision), CreatedAt: "2026-01-01T00:00:00Z",
		Tags: []string{"memory", "project", "decision"},
	}, "b")

	updated := updatedMemoryContent(original, memoryUpdate{
		ID: "01AAAAAAAAAAAAAAAAAAAAAAAA", Scope: "project", ScopeID: "p",
		NewType: "consolidated",
	})

	if fm := ParseMemoryFrontmatter(updated); fm.Type != string(MemoryTypeDecision) {
		t.Errorf("type = %q; an unrecognised type must be ignored", fm.Type)
	}
}

// buildMemoryFile and the update path must agree about the on-disk shape, or a
// memory's fields depend on which call last touched it.
func TestBuildMemoryFileAndUpdateAgreeOnFrontmatter(t *testing.T) {
	t.Parallel()

	created := buildMemoryFile(
		"01AAAAAAAAAAAAAAAAAAAAAAAA", "Title", "Body",
		"project", "p", "origin", true, string(MemoryTypeConvention), []string{"custom"},
	)

	fm := ParseMemoryFrontmatter(created)
	if fm.ID != "01AAAAAAAAAAAAAAAAAAAAAAAA" || fm.Title != "Title" {
		t.Fatalf("unexpected identity: %+v", fm)
	}
	if fm.Type != string(MemoryTypeConvention) || !fm.Important || fm.ProjectID != "origin" {
		t.Errorf("classification not written: %+v", fm)
	}
	for _, tag := range []string{"memory", "project", "convention", "custom"} {
		if !containsString(fm.Tags, tag) {
			t.Errorf("tag %q missing from a freshly created memory; tags = %v", tag, fm.Tags)
		}
	}

	// Round-tripping through an update must not lose any of it.
	updated := updatedMemoryContent(created, memoryUpdate{
		ID: fm.ID, Scope: "project", ScopeID: "p", Important: true, NewBody: "Body 2",
	})
	after := ParseMemoryFrontmatter(updated)
	if after.Type != fm.Type || after.Important != fm.Important || after.ProjectID != fm.ProjectID {
		t.Errorf("round-trip lost classification:\nbefore %+v\nafter  %+v", fm, after)
	}
	for _, tag := range fm.Tags {
		if !containsString(after.Tags, tag) {
			t.Errorf("round-trip dropped tag %q; after = %v", tag, after.Tags)
		}
	}
}

func containsString(haystack []string, needle string) bool {
	for _, s := range haystack {
		if s == needle {
			return true
		}
	}
	return false
}

package backlog

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestSlugify(t *testing.T) {
	tests := []struct {
		title string
		want  string
	}{
		{"Hello World!", "hello-world"},
		{"Título com Acentuação", "titulo-com-acentuacao"},
		{"---Special---Characters---", "special-characters"},
		{strings.Repeat("a", 100), strings.Repeat("a", 60)},
		{"", ""},
		{"   ", ""},
		{"abc", "abc"},
		{"CamelCase Title", "camelcase-title"},
		// Slug truncation: if 60th char boundary lands in the middle of a word
		// followed by hyphens, TrimRight strips them
		{strings.Repeat("abcde-", 11), strings.TrimRight(strings.Repeat("abcde-", 11)[:60], "-")},
	}

	for _, tc := range tests {
		t.Run(tc.title, func(t *testing.T) {
			got := slugify(tc.title)
			if got != tc.want {
				t.Errorf("slugify(%q) = %q; want %q", tc.title, got, tc.want)
			}
		})
	}
}

func TestExtractTitle(t *testing.T) {
	tests := []struct {
		name     string
		content  string
		fallback string
		want     string
	}{
		{"with h1", "# My Title\n\nBody text", "fallback", "My Title"},
		{"no h1", "some text\nmore text", "fallback", "fallback"},
		{"h1 not first line", "preamble\n# Later Title\nmore", "fallback", "Later Title"},
		{"empty content", "", "fallback", "fallback"},
		{"h2 only", "## Not H1\ntext", "fallback", "fallback"},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := extractTitle(tc.content, tc.fallback)
			if got != tc.want {
				t.Errorf("extractTitle() = %q, want %q", got, tc.want)
			}
		})
	}
}

// Location — the reason this package exists apart from dream

// The backlog defaults into the documentation tree, not the brand directory,
// so a deferred finding is versioned with the project instead of living in a
// gitignored directory on one machine.
func TestDirDefaultsUnderDocsTree(t *testing.T) {
	dir := Dir("/tmp/testproj")

	want := filepath.Join("/tmp/testproj", "docs", "tasks", "backlog")
	if dir != want {
		t.Errorf("Dir() = %q, want %q", dir, want)
	}
}

func TestDirHonoursConfigOverride(t *testing.T) {
	projectDir := t.TempDir()
	t.Setenv("GRAPHIT_IMPROVEMENTS_BACKLOG_DIR", "custom/queue")

	dir := Dir(projectDir)

	want := filepath.Join(projectDir, "custom", "queue")
	if dir != want {
		t.Errorf("Dir() = %q, want %q", dir, want)
	}
}

// A backlog kept elsewhere moves with knowledge.docs_dir, because the default
// is composed from it rather than hardcoded.
func TestDirFollowsDocsDir(t *testing.T) {
	projectDir := t.TempDir()
	t.Setenv("GRAPHIT_KNOWLEDGE_DOCS_DIR", "documentation")

	dir := Dir(projectDir)

	want := filepath.Join(projectDir, "documentation", "tasks", "backlog")
	if dir != want {
		t.Errorf("Dir() = %q, want %q", dir, want)
	}
}

func TestBacklogLifecycle(t *testing.T) {
	tempProj := t.TempDir()

	item, err := Add(tempProj, "My Backlog Item", "Instructions to work on.")
	if err != nil {
		t.Fatalf("Add failed: %v", err)
	}
	if item.Slug != "my-backlog-item" {
		t.Errorf("expected slug 'my-backlog-item', got %q", item.Slug)
	}

	// Try adding duplicate
	_, err = Add(tempProj, "My Backlog Item", "Instructions.")
	if err == nil {
		t.Error("expected error when adding duplicate item")
	}

	// 2. List and Pending
	list, err := List(tempProj)
	if err != nil {
		t.Fatalf("List failed: %v", err)
	}
	if len(list) != 1 || list[0].Slug != "my-backlog-item" {
		t.Errorf("expected 1 item in list, got %v", list)
	}

	pending, err := Pending(tempProj)
	if err != nil || len(pending) != 1 {
		t.Errorf("expected 1 pending item, got %v, error: %v", pending, err)
	}

	picked, err := Pick(tempProj)
	if err != nil || picked == nil || picked.Slug != "my-backlog-item" {
		t.Errorf("unexpected picked item: %v, error: %v", picked, err)
	}

	donePath := filepath.Join(Dir(tempProj), "my-backlog-item"+ResultExt)
	_ = os.WriteFile(donePath, []byte("Done content"), 0644)

	listDone, _ := List(tempProj)
	if len(listDone) != 1 || !listDone[0].Done {
		t.Error("expected item to be marked done")
	}

	pendingEmpty, _ := Pending(tempProj)
	if len(pendingEmpty) != 0 {
		t.Errorf("expected 0 pending items after done, got %v", pendingEmpty)
	}

	err = Remove(tempProj, "my-backlog-item")
	if err != nil {
		t.Fatalf("Remove failed: %v", err)
	}

	listEmpty, _ := List(tempProj)
	if len(listEmpty) != 0 {
		t.Errorf("expected empty list after removal, got %v", listEmpty)
	}
}

func TestAddEmptySlug(t *testing.T) {
	dir := t.TempDir()
	_, err := Add(dir, "   ", "body")
	if err == nil {
		t.Error("expected error for title producing empty slug")
	}
}

func TestAddBodyWithoutNewline(t *testing.T) {
	dir := t.TempDir()
	item, err := Add(dir, "Test Item", "body without newline")
	if err != nil {
		t.Fatalf("Add failed: %v", err)
	}
	// Verify content has newline appended
	if !strings.HasSuffix(item.Body, "\n") {
		t.Errorf("expected body to end with newline, got %q", item.Body)
	}
}

func TestAddBodyWithNewline(t *testing.T) {
	dir := t.TempDir()
	item, err := Add(dir, "Newline Item", "body with newline\n")
	if err != nil {
		t.Fatalf("Add failed: %v", err)
	}
	// Body already ends with newline, no extra newline should be added
	if strings.HasSuffix(item.Body, "\n\n\n") {
		t.Errorf("expected no duplicated trailing newline, got %q", item.Body)
	}
}

func TestAddEmptyBody(t *testing.T) {
	dir := t.TempDir()
	item, err := Add(dir, "No Body", "")
	if err != nil {
		t.Fatalf("Add failed: %v", err)
	}
	data, _ := os.ReadFile(item.Path)
	if string(data) != "# No Body\n\n" {
		t.Errorf("unexpected file content: %q", string(data))
	}
}

func TestAddWriteError(t *testing.T) {
	dir := t.TempDir()
	itemDir := Dir(dir)
	_ = os.MkdirAll(itemDir, 0o755)
	// Make dir read-only to prevent writing
	_ = os.Chmod(itemDir, 0o555)
	defer func() { _ = os.Chmod(itemDir, 0o755) }()

	_, err := Add(dir, "Write Error", "body")
	if err == nil {
		t.Error("expected error when writing the item file fails")
	}
}

func TestAddMkdirError(t *testing.T) {
	// Use a path that can't have directories created
	_, err := Add("/proc/nonexistent/path", "Test", "body")
	if err == nil {
		t.Error("expected error when MkdirAll fails")
	}
}

func TestListNonExistentDir(t *testing.T) {
	dir := t.TempDir()
	list, err := List(dir)
	if err != nil {
		t.Errorf("expected nil error for non-existent dir, got %v", err)
	}
	if list != nil {
		t.Errorf("expected nil list for non-existent dir, got %v", list)
	}
}

func TestListWithDirectoryEntry(t *testing.T) {
	dir := t.TempDir()
	itemDir := Dir(dir)
	_ = os.MkdirAll(itemDir, 0o755)

	// Create a subdirectory (should be skipped)
	_ = os.MkdirAll(filepath.Join(itemDir, "a-directory"), 0o755)
	// Create a non-.md file (should be skipped)
	_ = os.WriteFile(filepath.Join(itemDir, "readme.txt"), []byte("not an item"), 0644)
	_ = os.WriteFile(filepath.Join(itemDir, "valid-item.md"), []byte("# Valid\n\nbody"), 0644)

	list, err := List(dir)
	if err != nil {
		t.Fatalf("List failed: %v", err)
	}
	if len(list) != 1 || list[0].Slug != "valid-item" {
		t.Errorf("expected 1 valid item, got %v", list)
	}
}

func TestListSortOrder(t *testing.T) {
	dir := t.TempDir()
	itemDir := Dir(dir)
	_ = os.MkdirAll(itemDir, 0o755)

	// Create items with different mod times
	p1 := filepath.Join(itemDir, "first.md")
	p2 := filepath.Join(itemDir, "second.md")
	_ = os.WriteFile(p1, []byte("# First"), 0644)
	_ = os.Chtimes(p1, time.Now().Add(-2*time.Hour), time.Now().Add(-2*time.Hour))
	_ = os.WriteFile(p2, []byte("# Second"), 0644)

	list, err := List(dir)
	if err != nil {
		t.Fatalf("List failed: %v", err)
	}
	if len(list) != 2 {
		t.Fatalf("expected 2 items, got %d", len(list))
	}
	if list[0].Slug != "first" || list[1].Slug != "second" {
		t.Errorf("expected oldest first, got %q then %q", list[0].Slug, list[1].Slug)
	}
}

func TestRemoveNotFound(t *testing.T) {
	dir := t.TempDir()
	err := Remove(dir, "nonexistent")
	if err == nil {
		t.Error("expected error for non-existent item")
	}
}

func TestRemoveWithResultFile(t *testing.T) {
	dir := t.TempDir()
	item, err := Add(dir, "Remove Test", "body")
	if err != nil {
		t.Fatalf("Add failed: %v", err)
	}

	resultPath := filepath.Join(Dir(dir), item.Slug+ResultExt)
	_ = os.WriteFile(resultPath, []byte("done"), 0644)

	// Remove should also clean up result file
	err = Remove(dir, item.Slug)
	if err != nil {
		t.Fatalf("Remove failed: %v", err)
	}

	if _, err := os.Stat(resultPath); !os.IsNotExist(err) {
		t.Error("expected result file to be removed")
	}
}

func TestPickNoPending(t *testing.T) {
	dir := t.TempDir()
	picked, err := Pick(dir)
	if err != nil {
		t.Fatalf("Pick failed: %v", err)
	}
	if picked != nil {
		t.Errorf("expected nil for no pending items, got %v", picked)
	}
}

// Error paths

func TestListReadDirError(t *testing.T) {
	dir := t.TempDir()
	itemDir := Dir(dir)
	_ = os.MkdirAll(itemDir, 0o755)

	_ = os.Chmod(itemDir, 0o000)
	defer func() { _ = os.Chmod(itemDir, 0o755) }()

	_, err := List(dir)
	if err == nil {
		t.Error("expected error when ReadDir fails")
	}
}

func TestListReadFileError(t *testing.T) {
	dir := t.TempDir()
	itemDir := Dir(dir)
	_ = os.MkdirAll(itemDir, 0o755)

	// Create an item file that can't be read
	itemPath := filepath.Join(itemDir, "unreadable.md")
	_ = os.WriteFile(itemPath, []byte("# Title"), 0644)
	_ = os.Chmod(itemPath, 0o000)
	defer func() { _ = os.Chmod(itemPath, 0o644) }()

	list, err := List(dir)
	if err != nil {
		t.Fatalf("List should not fail: %v", err)
	}
	// The item should fall back to its slug as the title
	if len(list) == 1 && list[0].Title != "unreadable" {
		t.Errorf("expected fallback title 'unreadable', got %q", list[0].Title)
	}
}

// DirEntry.Info() from os.ReadDir almost never fails, so the continue branch is
// asserted indirectly: two readable entries must both survive the loop.
func TestListInfoError(t *testing.T) {
	dir := t.TempDir()
	itemDir := Dir(dir)
	_ = os.MkdirAll(itemDir, 0o755)

	_ = os.WriteFile(filepath.Join(itemDir, "first.md"), []byte("# First"), 0644)
	_ = os.WriteFile(filepath.Join(itemDir, "second.md"), []byte("# Second"), 0644)

	list, err := List(dir)
	if err != nil {
		t.Fatalf("List failed: %v", err)
	}
	if len(list) != 2 {
		t.Errorf("expected 2 items, got %d", len(list))
	}
}

func TestPendingError(t *testing.T) {
	dir := t.TempDir()
	itemDir := Dir(dir)
	_ = os.MkdirAll(itemDir, 0o755)
	_ = os.Chmod(itemDir, 0o000)
	defer func() { _ = os.Chmod(itemDir, 0o755) }()

	_, err := Pending(dir)
	if err == nil {
		t.Error("expected error when List fails")
	}
}

func TestPickError(t *testing.T) {
	dir := t.TempDir()
	itemDir := Dir(dir)
	_ = os.MkdirAll(itemDir, 0o755)
	_ = os.Chmod(itemDir, 0o000)
	defer func() { _ = os.Chmod(itemDir, 0o755) }()

	_, err := Pick(dir)
	if err == nil {
		t.Error("expected error when Pending fails")
	}
}

func TestRemoveRemoveError(t *testing.T) {
	dir := t.TempDir()
	item, err := Add(dir, "Remove Error Test", "body")
	if err != nil {
		t.Fatalf("Add failed: %v", err)
	}

	// Make dir read-only to prevent file removal
	itemDir := Dir(dir)
	_ = os.Chmod(itemDir, 0o555)
	defer func() { _ = os.Chmod(itemDir, 0o755) }()

	err = Remove(dir, item.Slug)
	if err == nil {
		t.Error("expected error when os.Remove fails")
	}
}

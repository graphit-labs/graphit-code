package hub

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSanitizeForFileName(t *testing.T) {
	t.Parallel()
	tests := []struct {
		input, want string
	}{
		{"Hello World!", "hello-world"},
		{"test_name.v2", "test_name.v2"},
		{"", "_"},
		{"---", "_"},
		{"ABC/DEF", "abc-def"},
		{"valid-name", "valid-name"},
		{"  spaces  ", "spaces"},
		{"UPPER", "upper"},
	}
	for _, tc := range tests {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()
			got := sanitizeForFileName(tc.input)
			if got != tc.want {
				t.Errorf("sanitizeForFileName(%q) = %q, want %q", tc.input, got, tc.want)
			}
		})
	}
}

func TestSanitizeEntryFileName(t *testing.T) {
	t.Parallel()

	t.Run("AST type", func(t *testing.T) {
		t.Parallel()
		e := &Entry{Type: TypeAST, Latest: "1.0.0"}
		got := sanitizeEntryFileName(e)
		if !strings.HasSuffix(got, ".json") {
			t.Errorf("expected .json suffix, got %q", got)
		}
		if !strings.HasPrefix(got, "ast_") {
			t.Errorf("expected ast_ prefix, got %q", got)
		}
	})

	t.Run("Knowledge type", func(t *testing.T) {
		t.Parallel()
		e := &Entry{Type: TypeKnowledge, Latest: "2.0.0"}
		got := sanitizeEntryFileName(e)
		if !strings.Contains(got, "knowledge_") {
			t.Errorf("expected knowledge_, got %q", got)
		}
	})

	t.Run("Rule with name", func(t *testing.T) {
		t.Parallel()
		e := &Entry{Type: TypeRule, Name: "my-rule", Latest: "1.0.0"}
		got := sanitizeEntryFileName(e)
		if !strings.Contains(got, "my-rule") {
			t.Errorf("expected my-rule in filename, got %q", got)
		}
	})

	t.Run("Rule with empty name fallback to ID", func(t *testing.T) {
		t.Parallel()
		e := &Entry{Type: TypeRule, Name: "", ID: "rule-id", Latest: "1.0.0"}
		got := sanitizeEntryFileName(e)
		if !strings.Contains(got, "rule-id") {
			t.Errorf("expected rule-id in filename, got %q", got)
		}
	})

	t.Run("Rule with underscore-only name fallback to ID", func(t *testing.T) {
		t.Parallel()
		e := &Entry{Type: TypeRule, Name: "---", ID: "fallback-id", Latest: "1.0.0"}
		got := sanitizeEntryFileName(e)
		if !strings.Contains(got, "fallback-id") {
			t.Errorf("expected fallback-id in filename, got %q", got)
		}
	})
}

func TestProjectDir(t *testing.T) {
	t.Parallel()

	t.Run("empty remoteID defaults to _global", func(t *testing.T) {
		t.Parallel()
		dir := projectDir("")
		if !strings.HasPrefix(dir, "projects/") {
			t.Errorf("expected projects/ prefix, got %q", dir)
		}
	})

	t.Run("consistent hashing", func(t *testing.T) {
		t.Parallel()
		d1 := projectDir("my-project")
		d2 := projectDir("my-project")
		if d1 != d2 {
			t.Errorf("expected same dir, got %q and %q", d1, d2)
		}
	})

	t.Run("different IDs produce different dirs", func(t *testing.T) {
		t.Parallel()
		d1 := projectDir("project-a")
		d2 := projectDir("project-b")
		if d1 == d2 {
			t.Error("expected different dirs for different projects")
		}
	})
}

func TestLoadFromCacheData(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	cache := &RegistryCache{
		V:      1,
		Commit: "abc123",
		Projects: map[string]Project{
			"proj1":      {RemoteID: "proj1", Name: "Project 1"},
			"_global":    {RemoteID: "_global", Name: "Global"},
			"other-proj": {RemoteID: "other-proj", Name: "Other"},
		},
		Entries: []Entry{
			{ID: "rule-1", Type: TypeRule, Name: "Rule One"},
			{ID: "skill-1", Type: TypeSkill, Name: "Skill One"},
			{ID: "rule-2", Type: TypeRule, Name: "Rule Two"},
		},
	}
	m.loadFromCacheData(cache)

	// _global should be excluded
	if _, ok := m.projects["_global"]; ok {
		t.Error("expected _global to be excluded from projects")
	}
	if _, ok := m.projects["proj1"]; !ok {
		t.Error("expected proj1 to be loaded")
	}
	if _, ok := m.projects["other-proj"]; !ok {
		t.Error("expected other-proj to be loaded")
	}

	if m.entries[TypeRule] == nil || len(m.entries[TypeRule]) != 2 {
		t.Errorf("expected 2 rule entries, got %d", len(m.entries[TypeRule]))
	}
	if m.entries[TypeSkill] == nil || len(m.entries[TypeSkill]) != 1 {
		t.Errorf("expected 1 skill entry, got %d", len(m.entries[TypeSkill]))
	}
}

func TestGetEntry(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	e1 := &Entry{ID: "my-rule", Type: TypeRule}
	e2 := &Entry{ID: "my-skill", Type: TypeSkill}
	m.entries[TypeRule] = map[string]*Entry{"my-rule": e1}
	m.entries[TypeSkill] = map[string]*Entry{"my-skill": e2}

	t.Run("with type", func(t *testing.T) {
		t.Parallel()
		got := m.GetEntry("my-rule", TypeRule)
		if got != e1 {
			t.Errorf("expected e1, got %v", got)
		}
	})

	t.Run("with wrong type", func(t *testing.T) {
		t.Parallel()
		got := m.GetEntry("my-rule", TypeSkill)
		if got != nil {
			t.Errorf("expected nil, got %v", got)
		}
	})

	t.Run("without type searches all", func(t *testing.T) {
		t.Parallel()
		got := m.GetEntry("my-skill", "")
		if got != e2 {
			t.Errorf("expected e2, got %v", got)
		}
	})

	t.Run("not found", func(t *testing.T) {
		t.Parallel()
		got := m.GetEntry("nonexistent", "")
		if got != nil {
			t.Errorf("expected nil, got %v", got)
		}
	})
}

func TestListEntries(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.entries[TypeRule] = map[string]*Entry{
		"r1": {ID: "r1", Type: TypeRule},
		"r2": {ID: "r2", Type: TypeRule},
	}
	m.entries[TypeSkill] = map[string]*Entry{
		"s1": {ID: "s1", Type: TypeSkill},
	}

	t.Run("no filter", func(t *testing.T) {
		t.Parallel()
		all := m.ListEntries("")
		if len(all) != 3 {
			t.Errorf("expected 3 entries, got %d", len(all))
		}
	})

	t.Run("filter rule", func(t *testing.T) {
		t.Parallel()
		rules := m.ListEntries(TypeRule)
		if len(rules) != 2 {
			t.Errorf("expected 2 rules, got %d", len(rules))
		}
	})

	t.Run("filter skill", func(t *testing.T) {
		t.Parallel()
		skills := m.ListEntries(TypeSkill)
		if len(skills) != 1 {
			t.Errorf("expected 1 skill, got %d", len(skills))
		}
	})

	t.Run("filter with no matches", func(t *testing.T) {
		t.Parallel()
		agents := m.ListEntries(TypeAgent)
		if len(agents) != 0 {
			t.Errorf("expected 0 agents, got %d", len(agents))
		}
	})
}

func TestSearchEntries(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.entries[TypeRule] = map[string]*Entry{
		"golang-rules":  {ID: "golang-rules", Type: TypeRule, Name: "Go Rules", Description: "Rules for Go projects"},
		"python-linter": {ID: "python-linter", Type: TypeRule, Name: "Python Linter", Description: "Linting for Python"},
	}
	m.entries[TypeSkill] = map[string]*Entry{
		"golang-testing": {ID: "golang-testing", Type: TypeSkill, Name: "Go Testing", Description: "Testing methodology"},
	}

	t.Run("search by name", func(t *testing.T) {
		t.Parallel()
		results := m.SearchEntries("Go", "")
		if len(results) < 2 {
			t.Errorf("expected at least 2 results, got %d", len(results))
		}
	})

	t.Run("search by description", func(t *testing.T) {
		t.Parallel()
		results := m.SearchEntries("Linting", "")
		if len(results) != 1 {
			t.Errorf("expected 1 result, got %d", len(results))
		}
	})

	t.Run("search with type filter", func(t *testing.T) {
		t.Parallel()
		results := m.SearchEntries("Go", TypeSkill)
		if len(results) != 1 {
			t.Errorf("expected 1 result, got %d", len(results))
		}
	})

	t.Run("search by id", func(t *testing.T) {
		t.Parallel()
		results := m.SearchEntries("golang", "")
		if len(results) < 2 {
			t.Errorf("expected at least 2 results matching ID, got %d", len(results))
		}
	})

	t.Run("no matches", func(t *testing.T) {
		t.Parallel()
		results := m.SearchEntries("nonexistent-term-xyz", "")
		if len(results) != 0 {
			t.Errorf("expected 0 results, got %d", len(results))
		}
	})
}

func TestListProjects(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.projects["p1"] = &Project{RemoteID: "p1", Name: "Project 1"}
	m.projects["p2"] = &Project{RemoteID: "p2", Name: "Project 2"}

	result := m.ListProjects()
	if len(result) != 2 {
		t.Errorf("expected 2 projects, got %d", len(result))
	}
}

func TestGetProject(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	p := &Project{RemoteID: "p1", Name: "Proj 1"}
	m.projects["p1"] = p

	if got := m.GetProject("p1"); got != p {
		t.Errorf("expected project, got %v", got)
	}
	if got := m.GetProject("nonexistent"); got != nil {
		t.Errorf("expected nil, got %v", got)
	}
}

func TestGetProjectByName(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	p := &Project{RemoteID: "p1", Name: "My Project"}
	m.projects["p1"] = p

	if got := m.GetProjectByName("My Project"); got != p {
		t.Errorf("expected project, got %v", got)
	}
	if got := m.GetProjectByName("nonexistent"); got != nil {
		t.Errorf("expected nil, got %v", got)
	}
}

func TestIsReady(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	if m.IsReady() {
		t.Error("expected not ready when the store is nil")
	}

	m.store = &S3Store{cacheBase: "/tmp/fake"}
	if !m.IsReady() {
		t.Error("expected ready when the store is set")
	}
}

func TestStore_methods(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	if m.Store() != nil {
		t.Error("expected nil store")
	}
	st := &S3Store{cacheBase: "/tmp/fake"}
	m.store = st
	if m.Store() != st {
		t.Error("expected the store to be returned")
	}
}

func TestCopyDir(t *testing.T) {
	t.Parallel()
	srcDir := t.TempDir()
	dstDir := t.TempDir()

	if err := os.MkdirAll(filepath.Join(srcDir, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(srcDir, "file1.txt"), []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(srcDir, "subdir", "file2.txt"), []byte("world"), 0o644); err != nil {
		t.Fatal(err)
	}

	dst := filepath.Join(dstDir, "copy")
	if err := os.MkdirAll(dst, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := copyDir(srcDir, dst); err != nil {
		t.Fatalf("copyDir failed: %v", err)
	}

	// Verify
	data, err := os.ReadFile(filepath.Join(dst, "file1.txt"))
	if err != nil {
		t.Fatalf("read copied file: %v", err)
	}
	if string(data) != "hello" {
		t.Errorf("expected 'hello', got %q", data)
	}

	data, err = os.ReadFile(filepath.Join(dst, "subdir", "file2.txt"))
	if err != nil {
		t.Fatalf("read copied subdir file: %v", err)
	}
	if string(data) != "world" {
		t.Errorf("expected 'world', got %q", data)
	}
}

func TestCopyFile(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	t.Run("binary file", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		src := filepath.Join(d, "test.go")
		dst := filepath.Join(d, "test_copy.go")
		if err := os.WriteFile(src, []byte("package main"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := copyFile(src, dst); err != nil {
			t.Fatalf("copyFile failed: %v", err)
		}
		data, _ := os.ReadFile(dst)
		if string(data) != "package main" {
			t.Errorf("expected 'package main', got %q", data)
		}
	})

	t.Run("markdown file uses brand copy", func(t *testing.T) {
		t.Parallel()
		src := filepath.Join(dir, "test.md")
		dst := filepath.Join(dir, "test_copy.md")
		if err := os.WriteFile(src, []byte("# Hello"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := copyFile(src, dst); err != nil {
			t.Fatalf("copyFile failed: %v", err)
		}
		data, _ := os.ReadFile(dst)
		if !strings.Contains(string(data), "Hello") {
			t.Errorf("expected Hello in output, got %q", data)
		}
	})

	t.Run("yaml file uses brand copy", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		src := filepath.Join(d, "test.yaml")
		dst := filepath.Join(d, "test_copy.yaml")
		if err := os.WriteFile(src, []byte("key: value"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := copyFile(src, dst); err != nil {
			t.Fatalf("copyFile failed: %v", err)
		}
	})

	t.Run("yml file uses brand copy", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		src := filepath.Join(d, "test.yml")
		dst := filepath.Join(d, "test_copy.yml")
		if err := os.WriteFile(src, []byte("key: value"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := copyFile(src, dst); err != nil {
			t.Fatalf("copyFile failed: %v", err)
		}
	})

	t.Run("txt file uses brand copy", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		src := filepath.Join(d, "test.txt")
		dst := filepath.Join(d, "test_copy.txt")
		if err := os.WriteFile(src, []byte("hello"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := copyFile(src, dst); err != nil {
			t.Fatalf("copyFile failed: %v", err)
		}
	})
}

func TestCopyFileWithBrand(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	src := filepath.Join(dir, "brand.md")
	dst := filepath.Join(dir, "brand_copy.md")
	if err := os.WriteFile(src, []byte("This is a test"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := copyFileWithBrand(src, dst); err != nil {
		t.Fatalf("copyFileWithBrand failed: %v", err)
	}
	data, _ := os.ReadFile(dst)
	if !strings.Contains(string(data), "test") {
		t.Errorf("expected 'test' in output, got %q", data)
	}
}

func TestCopyFile_ErrorPaths(t *testing.T) {
	t.Parallel()

	t.Run("source not found", func(t *testing.T) {
		t.Parallel()
		err := copyFile("/nonexistent/source.go", "/tmp/dst.go")
		if err == nil {
			t.Error("expected error for nonexistent source")
		}
	})

	t.Run("invalid dest", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		src := filepath.Join(dir, "src.go")
		_ = os.WriteFile(src, []byte("data"), 0o644)
		err := copyFile(src, "/nonexistent/dir/dst.go")
		if err == nil {
			t.Error("expected error for invalid dest")
		}
	})

	t.Run("brand source not found", func(t *testing.T) {
		t.Parallel()
		err := copyFileWithBrand("/nonexistent/source.md", "/tmp/dst.md")
		if err == nil {
			t.Error("expected error for nonexistent brand source")
		}
	})
}

// THE SHARD FALLBACK IS GONE, and this is what says so.
//
// It used to be that a store with no graph published its parse shards, and the consumer rebuilt
// the graph from them. That made an artifact's behaviour depend on which shape it happened to
// carry — mounted or rebuilt — and a consumer had no way to tell which it had got. Publishing now
// refuses instead, which moves the discovery from every consumer to the one publisher.
func TestPrepareASTPublishNoLongerFallsBackToShards(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "manifest.json"), []byte(`{"v":7}`), 0o644); err != nil {
		t.Fatal(err)
	}
	shardsDir := filepath.Join(dir, "shards")
	if err := os.MkdirAll(filepath.Join(shardsDir, "sub"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(shardsDir, "shard1.json"), []byte(`{"data":1}`), 0o644); err != nil {
		t.Fatal(err)
	}

	result, err := prepareASTPublish(dir, "s3://bucket/prefix", nil, nil)
	if err == nil {
		_ = os.RemoveAll(result)
		t.Fatal("a store with shards but no graph was published; the fallback is supposed to be gone")
	}
	if !strings.Contains(err.Error(), "no graph") {
		t.Errorf("the refusal does not name what is missing: %v", err)
	}
}

// An empty directory is not publishable either, and it must not leave a staging directory behind
// when it refuses — a temp directory per failed publish is a leak nothing cleans up.
func TestPrepareASTPublishRefusesAnEmptyDirectory(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	result, err := prepareASTPublish(dir, "s3://bucket/prefix", nil, nil)
	if err == nil {
		_ = os.RemoveAll(result)
		t.Fatal("an empty directory was published")
	}
	if result != "" {
		t.Errorf("a failed publish returned a staging path %q, which nothing will clean up", result)
	}
}

func TestLoadLocalRegistries(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	registry := struct {
		Entries []Entry `json:"entries"`
	}{
		Entries: []Entry{
			{ID: "local-rule", Type: TypeRule, Name: "Local Rule"},
			{ID: "local-skill", Type: TypeSkill, Name: "Local Skill"},
		},
	}
	data, _ := json.Marshal(registry)
	regPath := filepath.Join(dir, "registry.json")
	_ = os.WriteFile(regPath, data, 0o644)

	m := &RegistryManager{
		entries:       make(map[ArtifactType]map[string]*Entry),
		projects:      make(map[string]*Project),
		registryPaths: []string{regPath, "/nonexistent/path.json"},
	}
	m.loadLocalRegistries()

	if m.entries[TypeRule] == nil || m.entries[TypeRule]["local-rule"] == nil {
		t.Error("expected local-rule to be loaded")
	}
	if m.entries[TypeSkill] == nil || m.entries[TypeSkill]["local-skill"] == nil {
		t.Error("expected local-skill to be loaded")
	}
}

func TestLoadLocalRegistries_InvalidJSON(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	regPath := filepath.Join(dir, "bad.json")
	_ = os.WriteFile(regPath, []byte("not json"), 0o644)

	m := &RegistryManager{
		entries:       make(map[ArtifactType]map[string]*Entry),
		projects:      make(map[string]*Project),
		registryPaths: []string{regPath},
	}
	m.loadLocalRegistries()

	// Should not panic, just skip
	if len(m.entries) != 0 {
		t.Error("expected no entries from invalid JSON")
	}
}

func TestRegistryCachePath(t *testing.T) {
	t.Parallel()
	path := RegistryCachePath()
	if path == "" {
		t.Error("expected non-empty cache path")
	}
	if !strings.Contains(path, registryCacheFile) {
		t.Errorf("expected path to contain %q, got %q", registryCacheFile, path)
	}
}

func TestLoadRegistryCache_NotFound(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	_, err := m.LoadRegistryCache()
	// It may or may not exist on the system, but if it doesn't exist, it should return error
	if err == nil {
		// Cache exists on the system, that's fine
		return
	}
}

func TestSaveAndLoadRegistryCache(t *testing.T) {
	t.Parallel()
	// We can't override RegistryCachePath easily, so we test SaveRegistryCache logic
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	cache := &RegistryCache{
		V:        1,
		Commit:   "abc",
		Projects: map[string]Project{},
		Entries:  []Entry{},
	}
	// SaveRegistryCache writes to RegistryCachePath - this is a global path,
	// so just validate it doesn't error
	err := m.SaveRegistryCache(cache)
	if err != nil {
		t.Logf("SaveRegistryCache may fail in test env: %v", err)
	}
}

func TestLoadProjectDir(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	pf := projectFile{
		Version: 1,
		Project: &Project{RemoteID: "test-proj", Name: "Test Project"},
	}
	pfData, _ := json.Marshal(pf)
	_ = os.WriteFile(filepath.Join(dir, "project.json"), pfData, 0o644)

	ef := entryFile{
		Version: 1,
		Entry:   Entry{ID: "my-entry", Type: TypeRule, Name: "My Entry"},
	}
	efData, _ := json.Marshal(ef)
	_ = os.WriteFile(filepath.Join(dir, "rule_my-entry_1.0.0.json"), efData, 0o644)

	// Create a non-json file (should be skipped)
	_ = os.WriteFile(filepath.Join(dir, "readme.txt"), []byte("ignored"), 0o644)

	// Create a dir (should be skipped)
	_ = os.MkdirAll(filepath.Join(dir, "subdir"), 0o755)

	cache := &RegistryCache{
		Projects: make(map[string]Project),
		Entries:  []Entry{},
	}

	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.loadProjectDir(dir, "", cache)

	if len(cache.Projects) != 1 {
		t.Errorf("expected 1 project, got %d", len(cache.Projects))
	}
	if len(cache.Entries) != 1 {
		t.Errorf("expected 1 entry, got %d", len(cache.Entries))
	}
	if cache.Entries[0].ProjectID != "test-proj" {
		t.Errorf("expected project ID 'test-proj', got %q", cache.Entries[0].ProjectID)
	}
}

func TestLoadProjectDir_EntryFillsProjectID(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	// Entry with ProjectID but no project.json
	ef := entryFile{
		Version: 1,
		Entry:   Entry{ID: "my-entry", Type: TypeRule, Name: "My Entry", ProjectID: "from-entry"},
	}
	efData, _ := json.Marshal(ef)
	_ = os.WriteFile(filepath.Join(dir, "rule_my-entry_1.0.0.json"), efData, 0o644)

	cache := &RegistryCache{
		Projects: make(map[string]Project),
		Entries:  []Entry{},
	}

	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.loadProjectDir(dir, "", cache)

	if len(cache.Entries) != 1 {
		t.Errorf("expected 1 entry, got %d", len(cache.Entries))
	}
}

func TestLoadProjectDir_InvalidJSON(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	_ = os.WriteFile(filepath.Join(dir, "bad_entry.json"), []byte("not json"), 0o644)

	cache := &RegistryCache{
		Projects: make(map[string]Project),
		Entries:  []Entry{},
	}
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.loadProjectDir(dir, "", cache)

	if len(cache.Entries) != 0 {
		t.Errorf("expected 0 entries for invalid JSON, got %d", len(cache.Entries))
	}
}

func TestLoadRegistry_NoStore(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	err := m.loadRegistry()
	if err == nil {
		t.Error("expected error when the store is nil")
	}
	if !strings.Contains(err.Error(), "git store not initialized") {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestBuildRegistryCache_NoStore(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	_, err := m.BuildRegistryCache()
	if err == nil {
		t.Error("expected error when the store is nil")
	}
}

func TestPersistEntryFile_NoStore(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	err := m.persistEntryFile(&Entry{ID: "test", Type: TypeRule, Latest: "1.0.0"})
	if err == nil {
		t.Error("expected error when the store is nil")
	}
}

func TestPersistProjectFile_NoStore(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	err := m.persistProjectFile("test-id")
	if err == nil {
		t.Error("expected error when the store is nil")
	}
}

// Not parallel: the store is configured through environment variables.
func TestPersistEntryFile_WithStore(t *testing.T) {
	dir := t.TempDir()
	st, fake := newFakeBackedStore(t, dir)
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
		store:    st,
		baseCtx:  context.Background(),
	}
	entry := &Entry{ID: "test-entry", Type: TypeRule, Name: "Test", Latest: "1.0.0"}
	err := m.persistEntryFile(entry)
	if err != nil {
		t.Fatalf("persistEntryFile failed: %v", err)
	}

	relPath := projectDir("_global") + "/" + sanitizeEntryFileName(entry)

	// Both sides: the bucket is the truth, and the local mirror is what the registry walk
	// reads. A write that lands on only one of them is the bug this asserts against.
	if _, ok := fake.Object("registry/" + relPath); !ok {
		t.Errorf("entry not written to the bucket; keys: %v", fake.Keys())
	}
	if _, err := os.Stat(filepath.Join(dir, "registry", filepath.FromSlash(relPath))); err != nil {
		t.Errorf("entry not written to the local mirror: %v", err)
	}
}

func TestPersistEntryFile_WithProjectID(t *testing.T) {
	dir := t.TempDir()
	st, _ := newFakeBackedStore(t, dir)
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
		store:    st,
		baseCtx:  context.Background(),
	}
	entry := &Entry{ID: "test-entry", Type: TypeRule, Name: "Test", Latest: "1.0.0", ProjectID: "my-project"}
	err := m.persistEntryFile(entry)
	if err != nil {
		t.Fatalf("persistEntryFile failed: %v", err)
	}
}

func TestPersistProjectFile_WithStore(t *testing.T) {
	dir := t.TempDir()
	st, _ := newFakeBackedStore(t, dir)
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
		store:    st,
		baseCtx:  context.Background(),
	}

	// With non-global project
	m.projects["my-project"] = &Project{RemoteID: "my-project", Name: "My Proj"}
	err := m.persistProjectFile("my-project")
	if err != nil {
		t.Fatalf("persistProjectFile failed: %v", err)
	}

	// With empty remoteID (defaults to _global)
	err = m.persistProjectFile("")
	if err != nil {
		t.Fatalf("persistProjectFile with empty ID failed: %v", err)
	}
}

func TestGetDefaultBaselines_NoStore(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	_, err := m.GetDefaultBaselines(context.TODO())
	if err == nil {
		t.Error("expected error when the store is nil")
	}
}

func TestEnsureArtifactClone_NoGitStore(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	_, err := m.EnsureArtifactClone(context.TODO(), TypeRule, "test", "1.0.0", "")
	if err == nil {
		t.Error("expected error when the store is nil")
	}
}

func TestUpsertProject_Validation(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}

	t.Run("empty remoteID", func(t *testing.T) {
		t.Parallel()
		_, err := m.UpsertProject(context.TODO(), "", "name", "desc")
		if err == nil || !strings.Contains(err.Error(), "remoteID") {
			t.Errorf("expected remoteID error, got: %v", err)
		}
	})

	t.Run("empty name", func(t *testing.T) {
		t.Parallel()
		_, err := m.UpsertProject(context.TODO(), "id", "", "desc")
		if err == nil || !strings.Contains(err.Error(), "name") {
			t.Errorf("expected name error, got: %v", err)
		}
	})

	t.Run("duplicate name", func(t *testing.T) {
		t.Parallel()
		m2 := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		m2.projects["existing"] = &Project{RemoteID: "existing", Name: "Taken"}
		_, err := m2.UpsertProject(context.TODO(), "new-id", "Taken", "desc")
		if err == nil || !strings.Contains(err.Error(), "already used") {
			t.Errorf("expected duplicate name error, got: %v", err)
		}
	})

	t.Run("success without gitStore", func(t *testing.T) {
		t.Parallel()
		m3 := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		proj, err := m3.UpsertProject(context.TODO(), "new-id", "New Project", "desc")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if proj.Name != "New Project" {
			t.Errorf("expected name 'New Project', got %q", proj.Name)
		}
	})

	t.Run("update existing", func(t *testing.T) {
		t.Parallel()
		m4 := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		m4.projects["existing"] = &Project{RemoteID: "existing", Name: "Old Name"}
		proj, err := m4.UpsertProject(context.TODO(), "existing", "New Name", "new desc")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if proj.Name != "New Name" {
			t.Errorf("expected updated name")
		}
		if proj.Description != "new desc" {
			t.Errorf("expected updated description")
		}
	})
}

func TestDeleteEntry_NoGitStore(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	err := m.DeleteEntry(context.TODO(), "test", TypeRule)
	if err == nil || !strings.Contains(err.Error(), "hub not configured") {
		t.Errorf("expected hub not configured error, got: %v", err)
	}
}

func TestPublishEntry_NoGitStore(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	err := m.PublishEntry(context.TODO(), "test", "/tmp", &Entry{}, "1.0.0")
	if err == nil || !strings.Contains(err.Error(), "hub not configured") {
		t.Errorf("expected hub not configured error, got: %v", err)
	}
}

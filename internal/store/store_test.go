package store

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const storeTestProjectID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"

func withHome(t *testing.T) string {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	return home
}

func writeLockfile(t *testing.T, projectDir, id string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()),
		[]byte(`{"project":{"id":"`+id+`"}}`), 0o644); err != nil {
		t.Fatalf("write lockfile: %v", err)
	}
}

func TestSanitizeSegment(t *testing.T) {
	t.Parallel()
	tests := map[string]string{
		"1.2.3":         "1.2.3",
		"v1.0.0-beta.1": "v1.0.0-beta.1",
		"1.0.0+build7":  "1.0.0+build7",
		"../../etc":     "etc",
		"a/b":           "a-b",
		`..\..\etc`:     "etc",
		`a\b`:           "a-b",
		"C:1.0":         "C-1.0",
		"a*b?c<d>e":     "a-b-c-d-e",
		"trail. ":       "trail",
		"  1.0.0  ":     "1.0.0",
		"":              "unversioned",
		"...":           "unversioned",
		"/////":         "unversioned",
	}
	for in, want := range tests {
		if got := SanitizeSegment(in); got != want {
			t.Errorf("SanitizeSegment(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestVersionPathSegmentPreservesNamedVersionsWithoutPrefixOverlap(t *testing.T) {
	t.Parallel()
	if got := VersionPathSegment("branch/feature/hub-sync"); got != "~YnJhbmNoL2ZlYXR1cmUvaHViLXN5bmM" {
		t.Fatalf("VersionPathSegment() = %q", got)
	}
	if VersionPathSegment("branch/feature") == VersionPathSegment("branch-feature") {
		t.Fatal("distinct named versions collided")
	}
}

func TestDefuseReservedName(t *testing.T) {
	t.Parallel()
	for _, reserved := range []string{"nul", "NUL", "con", "com1", "LPT9", "aux.tar"} {
		if got := DefuseReservedName(reserved); got != reserved+"_" {
			t.Errorf("DefuseReservedName(%q) = %q, want %q", reserved, got, reserved+"_")
		}
	}
	for _, ok := range []string{"console", "com10", "nullable", "01ACME", "1.2.3"} {
		if got := DefuseReservedName(ok); got != ok {
			t.Errorf("DefuseReservedName(%q) = %q, want it unchanged", ok, got)
		}
	}
}

func TestSanitizeName(t *testing.T) {
	t.Parallel()
	tests := map[string]string{
		"My Repo":     "my-repo",
		"UPPER":       "upper",
		"a/b":         "ab",
		"01ACME":      "01acme",
		"":            "unnamed",
		"!!!":         "unnamed",
		"nul":         "nul_",
		"keep_this-1": "keep_this-1",
	}
	for in, want := range tests {
		if got := SanitizeName(in); got != want {
			t.Errorf("SanitizeName(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestProjectStoreIDPrefersTheLockfileID(t *testing.T) {
	projectDir := t.TempDir()
	writeLockfile(t, projectDir, storeTestProjectID)

	if got := ProjectID(projectDir); got != storeTestProjectID {
		t.Fatalf("ProjectID = %q, want %s", got, storeTestProjectID)
	}
	if got := ProjectStoreID(projectDir); got != storeTestProjectID {
		t.Fatalf("ProjectStoreID = %q, want the lockfile id", got)
	}
}

func TestProjectStoreIDRejectsANonULIDLockIdentity(t *testing.T) {
	projectDir := t.TempDir()
	writeLockfile(t, projectDir, "path-deadbeef")
	if got := ProjectStoreID(projectDir); got != "" {
		t.Fatalf("ProjectStoreID = %q, want no store for a non-ULID identity", got)
	}
	if _, err := EnsureProjectID(projectDir); err == nil {
		t.Fatal("EnsureProjectID accepted a non-ULID identity")
	}
}

func TestProjectStoreIDDoesNotCreateIdentityForARead(t *testing.T) {
	dir := t.TempDir()
	if got := ProjectStoreID(dir); got != "" {
		t.Fatalf("ProjectStoreID = %q, want no store before a stateful operation", got)
	}
	if _, err := os.Stat(filepath.Join(dir, brand.LockFileName())); !os.IsNotExist(err) {
		t.Fatalf("read-only identity lookup created a lockfile: %v", err)
	}
	if got := ASTProjectDir(dir); got != "" {
		t.Fatalf("ASTProjectDir = %q, want no unidentified store", got)
	}
	if got := KnowledgeProjectDir(dir); got != "" {
		t.Fatalf("KnowledgeProjectDir = %q, want no unidentified store", got)
	}
}

func TestEveryStoreLivesUnderTheGlobalDirectory(t *testing.T) {
	home := withHome(t)
	projectDir := t.TempDir()
	writeLockfile(t, projectDir, storeTestProjectID)

	global := filepath.Join(home, brand.DotDir())
	cases := map[string]string{
		"ast project":        ASTProjectDir(projectDir),
		"ast project bundle": ASTProjectIcebugDir(projectDir),
		"ast context":        ASTContextDir("other-repo"),
		"ast context bundle": ASTContextIcebugDir("other-repo"),
		"ast hub":            ASTHubDir("01PUB", "1.2.3"),
		"ast hub bundle":     ASTHubIcebugDir("01PUB", "1.2.3"),
		"knowledge project":  KnowledgeProjectDir(projectDir),
		"knowledge context":  KnowledgeContextDir("some-docs"),
		"memory wiki":        MemoryWikiDir("project", storeTestProjectID),
		"memory table":       MemoryTableDir("project", storeTestProjectID),
	}
	for label, got := range cases {
		rel, err := filepath.Rel(global, got)
		if err != nil || rel == ".." || filepath.IsAbs(rel) ||
			len(rel) >= 3 && rel[:3] == ".."+string(filepath.Separator) {
			t.Errorf("%s = %q, which is not under %q", label, got, global)
		}
		if inside, _ := filepath.Rel(projectDir, got); inside != "" && !filepath.IsAbs(inside) &&
			inside != ".." && len(inside) >= 3 && inside[:3] != ".."+string(filepath.Separator) {
			t.Errorf("%s = %q leaks into the project directory", label, got)
		}
	}

	want := map[string]string{
		ASTProjectIcebugDir(projectDir):               filepath.Join(global, "ast", "project", storeTestProjectID, "graph.icebug"),
		ASTContextIcebugDir("Other Repo"):             filepath.Join(global, "ast", "context", "other-repo", "graph.icebug"),
		ASTHubIcebugDir("01PUB", "1.2.3"):             filepath.Join(global, "ast", "hub", "01pub", "1.2.3", "graph.icebug"),
		KnowledgeProjectDir(projectDir):               filepath.Join(global, "wiki", "knowledge", "project", storeTestProjectID),
		KnowledgeContextDir("Some Docs"):              filepath.Join(global, "wiki", "knowledge", "context", "some-docs"),
		MemoryWikiDir("user", "abc123"):               filepath.Join(global, "wiki", "memory", "user", "abc123"),
		MemoryTableDir("project", storeTestProjectID): filepath.Join(global, "memory-table", "memory-project-"+storeTestProjectID),
	}
	for got, expected := range want {
		if got != expected {
			t.Errorf("got %q, want %q", got, expected)
		}
	}

	for label, got := range cases {
		if strings.Contains(got, "memory-raw") {
			t.Errorf("%s = %q resolves into the retired raw memory store", label, got)
		}
	}
}

// A project's own store and the same store seen from the ecosystem are resolved by
// two different functions. They must agree, or one project writes where another
// reads.
func TestByIDResolversAgreeWithProjectStoreID(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()
	writeLockfile(t, projectDir, storeTestProjectID)

	if got, want := ASTProjectDirByID(ProjectStoreID(projectDir)), ASTProjectDir(projectDir); got != want {
		t.Errorf("ASTProjectDirByID = %q, want %q", got, want)
	}
	if got, want := KnowledgeProjectDirByID(ProjectStoreID(projectDir)), KnowledgeProjectDir(projectDir); got != want {
		t.Errorf("KnowledgeProjectDirByID = %q, want %q", got, want)
	}
}

// Windows refuses a device name as a directory name, and a Hub artifact ID is chosen
// freely by its author — so an id can genuinely arrive as "nul". The suffix is applied
// on every platform so that one artifact resolves to the same path everywhere: a
// global directory carried to another machine, or a shared CI image, must agree.
func TestReservedDeviceNamesAreDefusedForExternalIdentifiers(t *testing.T) {
	withHome(t)

	for label, got := range map[string]string{
		"ASTProjectDirByID":       filepath.Base(ASTProjectDirByID("nul")),
		"KnowledgeProjectDirByID": filepath.Base(KnowledgeProjectDirByID("nul")),
		"ASTContextDir":           filepath.Base(ASTContextDir("nul")),
		"KnowledgeContextDir":     filepath.Base(KnowledgeContextDir("nul")),
		"ASTHubDir":               filepath.Base(filepath.Dir(ASTHubDir("nul", "1.0.0"))),
	} {
		if got != "nul_" {
			t.Errorf("%s = %q, want the device name defused to %q", label, got, "nul_")
		}
	}
}

// Every path this package builds must be composed with the platform's separator and
// must never carry a character Windows forbids in a name. The separator itself is the
// only one allowed, which is what makes the check meaningful on Linux too.
func TestNoStorePathCarriesACharacterWindowsForbids(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()
	writeLockfile(t, projectDir, storeTestProjectID)

	hostile := `../..\nul*?<>|:x. `

	paths := map[string]string{
		"ast project":       ASTProjectDir(projectDir),
		"ast context":       ASTContextDir(hostile),
		"ast hub":           ASTHubDir(hostile, hostile),
		"knowledge project": KnowledgeProjectDir(projectDir),
		"knowledge context": KnowledgeContextDir(hostile),
		"memory wiki":       MemoryWikiDir(hostile, hostile),
	}
	for label, p := range paths {
		rel, err := filepath.Rel(Root(), p)
		if err != nil {
			t.Errorf("%s = %q is not under the global root", label, p)
			continue
		}
		for _, seg := range strings.Split(filepath.ToSlash(rel), "/") {
			if seg == "" || seg == "." {
				continue
			}
			if seg == ".." {
				t.Errorf("%s = %q escapes the global root", label, p)
			}
			if strings.ContainsAny(seg, `\:*?"<>|`) {
				t.Errorf("%s segment %q carries a character Windows forbids", label, seg)
			}
			if strings.HasSuffix(seg, " ") || (strings.HasSuffix(seg, ".") && seg != "." && seg != "..") {
				t.Errorf("%s segment %q ends in a space or dot, which Windows silently strips", label, seg)
			}
		}
	}
}

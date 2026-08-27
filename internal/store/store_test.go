package store

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

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
		// Windows separators and the characters Windows forbids outright must be
		// neutralised on every platform, so one artifact resolves to one directory
		// name everywhere rather than to a per-platform variant.
		`..\..\etc`: "etc",
		`a\b`:       "a-b",
		"C:1.0":     "C-1.0",
		"a*b?c<d>e": "a-b-c-d-e",
		"trail. ":   "trail",
		"  1.0.0  ": "1.0.0",
		"":          "unversioned",
		"...":       "unversioned",
		"/////":     "unversioned",
	}
	for in, want := range tests {
		if got := SanitizeSegment(in); got != want {
			t.Errorf("SanitizeSegment(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestDefuseReservedName(t *testing.T) {
	t.Parallel()
	// Windows refuses these as directory names; the suffix is applied on every
	// platform so the path does not differ between them.
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
	writeLockfile(t, projectDir, "01ACMEPROJECT")

	if got := ProjectID(projectDir); got != "01ACMEPROJECT" {
		t.Fatalf("ProjectID = %q, want 01ACMEPROJECT", got)
	}
	if got := ProjectStoreID(projectDir); got != "01ACMEPROJECT" {
		t.Fatalf("ProjectStoreID = %q, want the lockfile id", got)
	}
}

// An uninitialised directory still has to be indexable — `ast index` never required
// `init` — so it gets a path-derived id instead of nothing.
func TestProjectStoreIDFallsBackToAStablePathHash(t *testing.T) {
	a := t.TempDir()
	b := t.TempDir()

	idA := ProjectStoreID(a)
	if idA == "" {
		t.Fatal("ProjectStoreID returned empty for an uninitialised project")
	}
	if idA[:5] != "path-" {
		t.Errorf("ProjectStoreID = %q, want a path- prefix so it cannot be mistaken for a ULID", idA)
	}
	if ProjectStoreID(a) != idA {
		t.Error("ProjectStoreID is not stable for the same directory")
	}
	if ProjectStoreID(b) == idA {
		t.Error("two different directories collided on one store id")
	}
}

func TestEveryStoreLivesUnderTheGlobalDirectory(t *testing.T) {
	home := withHome(t)
	projectDir := t.TempDir()
	writeLockfile(t, projectDir, "01ACME")

	global := filepath.Join(home, brand.DotDir())
	cases := map[string]string{
		"ast project":       ASTProjectDir(projectDir),
		"ast project db":    ASTProjectDBPath(projectDir),
		"ast context":       ASTContextDir("other-repo"),
		"ast hub":           ASTHubDir("01PUB", "1.2.3"),
		"knowledge project": KnowledgeProjectDir(projectDir),
		"knowledge context": KnowledgeContextDir("some-docs"),
		"memory wiki":       MemoryWikiDir("project", "01ACME"),
		"memory worktree":   MemoryRawDir("project", "01ACME"),
	}
	for label, got := range cases {
		rel, err := filepath.Rel(global, got)
		if err != nil || rel == ".." || filepath.IsAbs(rel) ||
			len(rel) >= 3 && rel[:3] == ".."+string(filepath.Separator) {
			t.Errorf("%s = %q, which is not under %q", label, got, global)
		}
		// Nothing may resolve into the project itself: that is the whole point.
		if inside, _ := filepath.Rel(projectDir, got); inside != "" && !filepath.IsAbs(inside) &&
			inside != ".." && len(inside) >= 3 && inside[:3] != ".."+string(filepath.Separator) {
			t.Errorf("%s = %q leaks into the project directory", label, got)
		}
	}

	// The exact shapes, so a rename of the layout is a deliberate decision rather
	// than a silent one that orphans every store on every machine.
	want := map[string]string{
		ASTProjectDBPath(projectDir):      filepath.Join(global, "ast", "project", "01ACME", DBFileName),
		ASTContextDBPath("Other Repo"):    filepath.Join(global, "ast", "context", "other-repo", DBFileName),
		ASTHubDBPath("01PUB", "1.2.3"):    filepath.Join(global, "ast", "hub", "01pub", "1.2.3", DBFileName),
		KnowledgeProjectDir(projectDir):   filepath.Join(global, "wiki", "knowledge", "project", "01ACME"),
		KnowledgeContextDir("Some Docs"):  filepath.Join(global, "wiki", "knowledge", "context", "some-docs"),
		MemoryWikiDir("user", "abc123"):   filepath.Join(global, "wiki", "memory", "user", "abc123"),
		MemoryRawDir("project", "01ACME"): filepath.Join(global, "memory-raw", "memory-project-01ACME"),
	}
	for got, expected := range want {
		if got != expected {
			t.Errorf("got %q, want %q", got, expected)
		}
	}
}

// platform semantics

func TestProjectStoreIDFoldsCaseOnlyWhereTheFilesystemDoes(t *testing.T) {
	t.Parallel()
	// Both behaviours are asserted on every host, because the one that matters here
	// is the one this machine does NOT have: a Linux CI would otherwise never
	// exercise the Windows and macOS path at all.
	const lower = "/home/dev/proj"
	const upper = "/home/dev/PROJ"

	if a, b := pathStoreID(lower, true), pathStoreID(upper, true); a != b {
		t.Errorf("with folding on, %q and %q must share a store id, got %q and %q", lower, upper, a, b)
	}
	if a, b := pathStoreID(lower, false), pathStoreID(upper, false); a == b {
		t.Errorf("with folding off, %q and %q are different directories and must not share a store id", lower, upper)
	}

	// And the platform picks the right one.
	wantFold := runtime.GOOS == "windows" || runtime.GOOS == "darwin"
	if caseInsensitivePaths != wantFold {
		t.Errorf("caseInsensitivePaths = %v on %s, want %v", caseInsensitivePaths, runtime.GOOS, wantFold)
	}

	// A real directory still resolves, folded or not.
	dir := t.TempDir()
	if id := ProjectStoreID(dir); !strings.HasPrefix(id, "path-") {
		t.Errorf("ProjectStoreID = %q, want a path- prefix", id)
	}
}

// A project's own store and the same store seen from the ecosystem are resolved by
// two different functions. They must agree, or one project writes where another
// reads.
func TestByIDResolversAgreeWithProjectStoreID(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()
	writeLockfile(t, projectDir, "01ACME")

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
func TestReservedDeviceNamesAreDefusedEverywhere(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()
	writeLockfile(t, projectDir, "nul")

	for label, got := range map[string]string{
		"ProjectStoreID":          ProjectStoreID(projectDir),
		"ASTProjectDir":           filepath.Base(ASTProjectDir(projectDir)),
		"ASTProjectDirByID":       filepath.Base(ASTProjectDirByID("nul")),
		"KnowledgeProjectDir":     filepath.Base(KnowledgeProjectDir(projectDir)),
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
	writeLockfile(t, projectDir, "01ACME")

	// A hostile name: separators, device name, wildcards, colon, trailing dot.
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

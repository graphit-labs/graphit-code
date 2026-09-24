package ast

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	scip "github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
)

func useLocalSCIPDevImage(t *testing.T) {
	t.Helper()
	t.Setenv("GRAPHIT_AST_SCIP_VERSION", "dev")
	t.Setenv("GRAPHIT_AST_SCIP_PULL_POLICY", "missing")
}

func TestSCIPDockerPullsBeforeUsingCachedTag(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 with the Go :dev image available locally")
	}
	root, global := t.TempDir(), t.TempDir()
	for _, dir := range []string{scipCacheDir(root, global, "go"), scipOutputDir(root, global, "go")} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	tag := fmt.Sprintf("graphit-local-only-%d-%d", os.Getpid(), time.Now().UnixNano())
	image := "ghcr.io/graphit-labs/graphit-scip-go:" + tag
	if output, err := exec.Command("docker", "image", "tag", "ghcr.io/graphit-labs/graphit-scip-go:dev", image).CombinedOutput(); err != nil {
		t.Fatalf("tag local Go image: %v: %s", err, output)
	}
	t.Cleanup(func() { _ = exec.Command("docker", "image", "rm", image).Run() })
	t.Setenv("GRAPHIT_AST_SCIP_VERSION", tag)
	t.Setenv("GRAPHIT_AST_SCIP_PULL_POLICY", "")
	uid, gid := scipHostIdentity(context.Background())
	cacheDir, outDir := scipCacheDir(root, global, "go"), scipOutputDir(root, global, "go")
	if id, err := ensureSCIPContainer(context.Background(), root, cacheDir, outDir, "go", uid, gid); err == nil {
		_ = exec.Command("docker", "container", "rm", "-f", "-v", id).Run()
		t.Fatal("default pull policy used a cached image whose registry tag does not exist")
	}
	t.Setenv("GRAPHIT_AST_SCIP_PULL_POLICY", "missing")
	id, err := ensureSCIPContainer(context.Background(), root, cacheDir, outDir, "go", uid, gid)
	if err != nil {
		t.Fatalf("explicit local-image policy failed: %v", err)
	}
	if output, err := exec.Command("docker", "container", "rm", "-v", id).CombinedOutput(); err != nil {
		t.Fatalf("remove local-image container: %v: %s", err, output)
	}
}

func TestSCIPProjectContainerAndBindPathsAreStableAndIsolated(t *testing.T) {
	rootA := filepath.Join(t.TempDir(), "project a")
	rootB := filepath.Join(t.TempDir(), "project b")
	sharedStore := t.TempDir()
	if scipContainerName(rootA, "go") == scipContainerName(rootB, "go") {
		t.Fatal("container name is not stable per project")
	}
	if scipCacheDir(rootA, sharedStore, "go") == scipCacheDir(rootB, sharedStore, "go") ||
		scipOutputDir(rootA, sharedStore, "go") == scipOutputDir(rootB, sharedStore, "go") {
		t.Fatal("projects share SCIP cache or output bind paths")
	}
}

func TestSCIPImageAndSignatureUseEffectiveVersion(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	t.Setenv("GRAPHIT_AST_SCIP_VERSION", "")
	root := t.TempDir()
	if got := scipImage(root, "go"); got != "ghcr.io/graphit-labs/graphit-scip-go:v1" {
		t.Fatalf("default SCIP image = %q", got)
	}
	if got := scipConfigurationSignature(root); !strings.HasSuffix(got, "\nv1") {
		t.Fatalf("default SCIP signature = %q", got)
	}
	t.Setenv("GRAPHIT_AST_SCIP_VERSION", "0.2.1")
	if got := scipImage(root, "go"); got != "ghcr.io/graphit-labs/graphit-scip-go:0.2.1" {
		t.Fatalf("configured SCIP image = %q", got)
	}
	if got := scipConfigurationSignature(root); !strings.HasSuffix(got, "\n0.2.1") {
		t.Fatalf("configured SCIP signature = %q", got)
	}
}

func TestSCIPTypeScriptConfigUsesExactAllowedFiles(t *testing.T) {
	root, cache := t.TempDir(), t.TempDir()
	allowed := map[string]bool{"nested/a\"b.ts": true, "index.ts": true}
	if err := writeSCIPTypeScriptConfig(root, cache, allowed); err != nil {
		t.Fatal(err)
	}
	configPath := filepath.Join(cache, "graphit-tsconfig.json")
	data, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	var config struct {
		Files []string `json:"files"`
	}
	if err := json.Unmarshal(data, &config); err != nil {
		t.Fatal(err)
	}
	if len(config.Files) != 2 || config.Files[0] != "/workspace/index.ts" || config.Files[1] != "/workspace/nested/a\"b.ts" {
		t.Fatalf("wrong inferred TypeScript roots: %v", config.Files)
	}
	if err := writeSCIPTypeScriptConfig(root, cache, map[string]bool{"../escape.ts": true}); err == nil {
		t.Fatal("unsafe source path accepted")
	}
	if err := os.WriteFile(filepath.Join(root, "tsconfig.json"), []byte(`{"files":["index.ts"]}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := writeSCIPTypeScriptConfig(root, cache, allowed); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(configPath); !os.IsNotExist(err) {
		t.Fatalf("inferred config remains for user tsconfig: %v", err)
	}
}

func TestSCIPClangAllowlistUsesGlobalCache(t *testing.T) {
	cache := t.TempDir()
	allowed := map[string]bool{"nested/Keep.cpp": true, "src/a,b c.c": true}
	if err := writeSCIPClangAllowlist(cache, allowed); err != nil {
		t.Fatal(err)
	}
	manifest := filepath.Join(cache, "graphit-allowed-files.json")
	data, err := os.ReadFile(manifest)
	if err != nil {
		t.Fatal(err)
	}
	var files []string
	if err := json.Unmarshal(data, &files); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(files, []string{"nested/Keep.cpp", "src/a,b c.c"}) {
		t.Fatalf("wrong Clang roots: %v", files)
	}
	if err := writeSCIPClangAllowlist(cache, map[string]bool{"../escape.c": true}); err == nil {
		t.Fatal("unsafe Clang source path accepted")
	}
	if err := writeSCIPClangAllowlist(cache, nil); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(manifest); !os.IsNotExist(err) {
		t.Fatalf("stale Clang manifest remains: %v", err)
	}
}

func TestSCIPClangSelectionTracksNestedCMake(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	root := t.TempDir()
	writeSCIPJavaFixture(t, filepath.Join(root, "CMakeLists.txt"), "add_subdirectory(nested)\n")
	writeSCIPJavaFixture(t, filepath.Join(root, "nested", "CMakeLists.txt"), "add_library(sample sample.c)\n")
	writeSCIPJavaFixture(t, filepath.Join(root, "nested", "sample.c"), "int sample(void) { return 1; }\n")
	files := []string{filepath.Join(root, "nested", "sample.c")}
	before, count, err := scipClangSelectionSignature(root, files, nil)
	if err != nil || count != 1 {
		t.Fatalf("initial CMake selector: count=%d err=%v", count, err)
	}
	writeSCIPJavaFixture(t, filepath.Join(root, "nested", "CMakeLists.txt"), "add_library(sample sample.c)\nset(CMAKE_C_STANDARD 17)\n")
	after, count, err := scipClangSelectionSignature(root, files, nil)
	if err != nil || count != 1 || before == after {
		t.Fatalf("nested CMake edit did not invalidate Clang: count=%d err=%v", count, err)
	}
}

func TestPrepareSCIPEntriesUsesNestedIgnoresAndOneRunPerFamily(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	root := t.TempDir()
	for _, dir := range []string{"src", "git"} {
		if err := os.MkdirAll(filepath.Join(root, dir), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	for name, body := range map[string]string{
		"src/.astignore": "*.go\n!keep.go\n",
		"src/keep.go":    "package demo\nfunc Keep() {}\n",
		"src/skip.go":    "package demo\nfunc Skip() {}\n",
		"git/.gitignore": "*.go\n!keep.go\n",
		"git/keep.go":    "package demo\nfunc Keep() {}\n",
		"git/skip.go":    "package demo\nfunc Skip() {}\n",
	} {
		if err := os.WriteFile(filepath.Join(root, name), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	index := &scip.Index{Documents: []*scip.Document{
		{RelativePath: "src/keep.go", Language: "go", Symbols: []*scip.SymbolInformation{{Symbol: "scip-go gomod demo v1 Keep().", DisplayName: "Keep", Kind: scip.SymbolInformation_Function}}, Occurrences: []*scip.Occurrence{{Symbol: "scip-go gomod demo v1 Keep().", SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{1, 5, 9}}}},
		{RelativePath: "src/skip.go", Language: "go", Symbols: []*scip.SymbolInformation{{Symbol: "scip-go gomod demo v1 Skip().", DisplayName: "Skip", Kind: scip.SymbolInformation_Function}}, Occurrences: []*scip.Occurrence{{Symbol: "scip-go gomod demo v1 Skip().", SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{1, 5, 9}}}},
		{RelativePath: "git/keep.go", Language: "go"},
		{RelativePath: "git/skip.go", Language: "go"},
	}}
	data, err := proto.Marshal(index)
	if err != nil {
		t.Fatal(err)
	}
	old := runSCIPImage
	defer func() { runSCIPImage = old }()
	calls := 0
	runSCIPImage = func(_ context.Context, gotRoot, _, family string, allowed ...map[string]bool) ([]byte, error) {
		calls++
		if gotRoot != root || family != "go" {
			t.Fatalf("runner got root=%q family=%q", gotRoot, family)
		}
		if len(allowed) != 1 || len(allowed[0]) != 2 || !allowed[0]["src/keep.go"] || !allowed[0]["git/keep.go"] {
			t.Fatalf("runner did not receive the final nested ignore selection: %v", allowed)
		}
		return data, nil
	}
	entries, failures := prepareSCIPEntries(context.Background(), root, t.TempDir(), []string{filepath.Join(root, "src/keep.go")}, nil)
	if len(failures) != 0 || calls != 1 || len(entries) != 2 || entries[filepath.Join(root, "src/keep.go")] == nil || entries[filepath.Join(root, "git/keep.go")] == nil {
		t.Fatalf("wrong prepared entries: calls=%d entries=%v", calls, entries)
	}
	if got, failures := prepareSCIPEntries(context.Background(), root, t.TempDir(), nil, nil); len(failures) != 0 || len(got) != 0 || calls != 1 {
		t.Fatalf("no-op launched indexer: calls=%d entries=%v", calls, got)
	}
	if got, failures := prepareSCIPEntries(context.Background(), root, t.TempDir(), []string{filepath.Join(root, "src/keep.go")}, map[string]bool{".go": true}); len(failures) != 0 || len(got) != 0 || calls != 1 {
		t.Fatalf("excluded extension launched indexer: calls=%d entries=%v", calls, got)
	}
}

func TestPrepareSCIPEntriesFallsBackBeforeDockerForInvalidGrammar(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	root := t.TempDir()
	file := filepath.Join(root, "a.go")
	if err := os.WriteFile(file, []byte("package demo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	profilePath := filepath.Join(projectQueriesDir(root), "scip", "scip-go.yaml")
	if err := os.MkdirAll(filepath.Dir(profilePath), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(profilePath, []byte("merge: true\nrelations: [CALLS]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	old := runSCIPImage
	defer func() { runSCIPImage = old }()
	runSCIPImage = func(context.Context, string, string, string, ...map[string]bool) ([]byte, error) {
		t.Fatal("invalid grammar started Docker")
		return nil, nil
	}
	entries, failures := prepareSCIPEntries(context.Background(), root, t.TempDir(), []string{file}, nil)
	if len(entries) != 0 || len(failures) != 1 || !strings.Contains(failures[0].Error(), "SCIP grammar") {
		t.Fatalf("invalid grammar did not trigger syntax fallback: entries=%v failures=%v", entries, failures)
	}
}

func TestPrepareSCIPEntriesAllowsGrammarToLeaveExtensionToSyntax(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	root := t.TempDir()
	file := filepath.Join(root, "a.js")
	if err := os.WriteFile(file, []byte("function demo() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(projectQueriesDir(root), "scip", "scip-typescript.yaml")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("merge: true\nextensions: [.ts]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	old := runSCIPImage
	defer func() { runSCIPImage = old }()
	runSCIPImage = func(context.Context, string, string, string, ...map[string]bool) ([]byte, error) {
		t.Fatal("excluded grammar extension started Docker")
		return nil, nil
	}
	entries, failures := prepareSCIPEntries(context.Background(), root, t.TempDir(), []string{file}, nil)
	if len(failures) != 0 || len(entries) != 0 {
		t.Fatalf("excluded extension should use syntax without indexer: entries=%v failures=%v", entries, failures)
	}
}

func TestSCIPDockerUsesFreshOverlayAcrossParses(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 with the Go image available locally")
	}
	useLocalSCIPDevImage(t)
	project := filepath.Join(t.TempDir(), "project with spaces")
	if err := os.MkdirAll(project, 0o755); err != nil {
		t.Fatal(err)
	}
	cache := filepath.Join(t.TempDir(), "cache with spaces")
	cwd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	relativeCache, err := filepath.Rel(cwd, cache)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(project, "go.mod"), []byte("module example.com/scip-smoke\n\ngo 1.23\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(project, "main.go"), []byte("package smoke\nfunc Add(a, b int) int { return a + b }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	const originalIndex = "project-owned-index"
	if err := os.WriteFile(filepath.Join(project, "index.scip"), []byte(originalIndex), 0o644); err != nil {
		t.Fatal(err)
	}
	name := scipContainerName(project, "go")
	t.Cleanup(func() { _ = exec.Command("docker", "container", "rm", "-f", "-v", name).Run() })
	for _, dir := range []string{scipCacheDir(project, cache, "go"), scipOutputDir(project, cache, "go")} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	uid, gid := scipHostIdentity(context.Background())
	created, err := ensureSCIPContainer(context.Background(), project, scipCacheDir(project, cache, "go"), scipOutputDir(project, cache, "go"), "go", uid, gid)
	if err != nil || created == "" {
		t.Fatalf("create overlay container: id=%q err=%v", created, err)
	}
	inspection, err := exec.Command("docker", "container", "inspect", name).Output()
	if err != nil {
		t.Fatal(err)
	}
	var containers []struct {
		Config struct {
			Env []string `json:"Env"`
		} `json:"Config"`
		Mounts []struct {
			Name        string `json:"Name"`
			Source      string `json:"Source"`
			Destination string `json:"Destination"`
			RW          bool   `json:"RW"`
		} `json:"Mounts"`
	}
	if err := json.Unmarshal(inspection, &containers); err != nil || len(containers) != 1 {
		t.Fatalf("inspect overlay mount contract: %v", err)
	}
	mounts := make(map[string]string)
	var volumeName string
	for _, mount := range containers[0].Mounts {
		mounts[mount.Destination] = mount.Source
		if (mount.Destination == "/source" || mount.Destination == "/workspace") && mount.RW {
			t.Fatal("source bind is writable")
		}
		if mount.Destination == "/overlay" {
			volumeName = mount.Name
		}
	}
	if mounts["/source"] != project || mounts["/workspace"] != project || mounts["/output"] != scipOutputDir(project, cache, "go") || mounts["/cache"] != scipCacheDir(project, cache, "go") || volumeName == "" {
		t.Fatalf("wrong overlay mount contract: %v", mounts)
	}
	if output, err := exec.Command("docker", "container", "rm", "-v", created).CombinedOutput(); err != nil {
		t.Fatalf("remove probe container: %v: %s", err, output)
	}
	if _, err := exec.Command("docker", "volume", "inspect", volumeName).Output(); err == nil {
		t.Fatal("anonymous overlay volume survived container removal")
	}
	for run := 0; run < 2; run++ {
		data, err := runSCIPImage(context.Background(), project, relativeCache, "go")
		if err != nil || len(data) == 0 {
			t.Fatalf("run %d: index bytes=%d err=%v", run, len(data), err)
		}
		entries, err := decodeSCIP(data, project, map[string]bool{"main.go": true})
		if err != nil || entries[filepath.Join(project, "main.go")] == nil || len(entries[filepath.Join(project, "main.go")].Entities) == 0 {
			t.Fatalf("run %d did not produce canonical Go entities: entries=%v err=%v", run, entries, err)
		}
		preserved, err := os.ReadFile(filepath.Join(project, "index.scip"))
		if err != nil || string(preserved) != originalIndex {
			t.Fatalf("run %d changed the project-owned index: %q, err=%v", run, preserved, err)
		}
		files, err := os.ReadDir(project)
		if err != nil || len(files) != 3 {
			t.Fatalf("run %d changed project file inventory: files=%v err=%v", run, files, err)
		}
		if _, err := exec.Command("docker", "container", "inspect", name).Output(); err == nil {
			t.Fatalf("run %d left the container and anonymous overlay volume behind", run)
		}
	}
}

func TestSCIPDockerTypeScriptLeavesSourceUntouched(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 with the TypeScript image available locally")
	}
	useLocalSCIPDevImage(t)
	project := t.TempDir()
	cache := t.TempDir()
	for name, body := range map[string]string{
		"package.json": `{"name":"scip-smoke","version":"1.0.0"}`,
		"index.ts":     "export const value = 1;\n",
	} {
		if err := os.WriteFile(filepath.Join(project, name), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	name := scipContainerName(project, "typescript")
	t.Cleanup(func() { _ = exec.Command("docker", "container", "rm", name).Run() })
	data, err := runSCIPImage(context.Background(), project, cache, "typescript")
	if err != nil || len(data) == 0 {
		t.Fatalf("TypeScript SCIP index failed: bytes=%d err=%v", len(data), err)
	}
	entries, err := decodeSCIP(data, project, map[string]bool{"index.ts": true})
	if err != nil || entries[filepath.Join(project, "index.ts")] == nil {
		t.Fatalf("TypeScript SCIP document missing: entries=%v err=%v", entries, err)
	}
	files, err := os.ReadDir(project)
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 2 || files[0].Name() != "index.ts" || files[1].Name() != "package.json" {
		t.Fatalf("SCIP changed project files: %v", files)
	}
	if _, err := os.Stat(filepath.Join(cache, "scip", scipProjectKey(project), "typescript", "graphit-tsconfig.json")); err != nil {
		t.Fatalf("inferred TypeScript configuration was not stored in AST cache: %v", err)
	}
}

func TestSCIPTypeScriptPreFilterIntegration(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 with the TypeScript :dev image available locally")
	}
	useLocalSCIPDevImage(t)
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	root, global := t.TempDir(), t.TempDir()
	for name, body := range map[string]string{
		"package.json":      `{"name":"scip-ignore","version":"1.0.0"}`,
		".gitignore":        "nested/*.ts\n!nested/Keep.ts\n",
		"Keep.ts":           "export const keep = 1;\n",
		"nested/.astignore": "*.ts\n!Keep.ts\n",
		"nested/Keep.ts":    "export const keptNested = 2;\n",
		"nested/Skip.ts":    "export const skippedNested = 3;\n",
	} {
		writeSCIPJavaFixture(t, filepath.Join(root, name), body)
	}
	t.Cleanup(func() {
		_ = exec.Command("docker", "container", "rm", "-f", scipContainerName(root, "typescript")).Run()
	})
	parse := func(wantRawSkip, wantImportedSymbol bool) {
		t.Helper()
		before := snapshotSCIPJavaFixture(t, root)
		entries, failures := prepareSCIPEntries(context.Background(), root, global, []string{filepath.Join(root, "Keep.ts")}, nil)
		if len(failures) != 0 {
			t.Fatalf("TypeScript indexer failed: %v", failures)
		}
		if len(entries) != 2 || entries[filepath.Join(root, "Keep.ts")] == nil || entries[filepath.Join(root, "nested", "Keep.ts")] == nil || entries[filepath.Join(root, "nested", "Skip.ts")] != nil {
			t.Fatalf("wrong Graphit imports: %v", entries)
		}
		data, err := os.ReadFile(filepath.Join(scipOutputDir(root, global, "typescript"), "index.scip"))
		if err != nil {
			t.Fatal(err)
		}
		index := &scip.Index{}
		if err := proto.Unmarshal(data, index); err != nil {
			t.Fatal(err)
		}
		seen := map[string]bool{}
		importedSymbol := false
		for _, doc := range index.Documents {
			seen[doc.RelativePath] = true
			if doc.RelativePath == "Keep.ts" {
				for _, occurrence := range doc.Occurrences {
					if strings.Contains(occurrence.Symbol, "Skip.ts") {
						importedSymbol = true
					}
				}
			}
		}
		if !seen["Keep.ts"] || !seen["nested/Keep.ts"] || seen["nested/Skip.ts"] != wantRawSkip {
			t.Fatalf("wrong raw SCIP documents: %v", seen)
		}
		if wantImportedSymbol && !importedSymbol {
			t.Fatal("ignored dependency symbol was not retained in allowed document")
		}
		if after := snapshotSCIPJavaFixture(t, root); !reflect.DeepEqual(before, after) {
			t.Fatalf("source changed: before=%v after=%v", before, after)
		}
	}
	parse(false, false)
	configPath := filepath.Join(scipCacheDir(root, global, "typescript"), "graphit-tsconfig.json")
	config, err := os.ReadFile(configPath)
	if err != nil || !strings.Contains(string(config), `"files"`) || strings.Contains(string(config), `"include"`) {
		t.Fatalf("inferred config lacks exact file roots: %s err=%v", config, err)
	}
	writeSCIPJavaFixture(t, filepath.Join(root, "Keep.ts"), "import { skippedNested } from './nested/Skip';\nexport const keep = skippedNested;\n")
	parse(false, true)
	writeSCIPJavaFixture(t, filepath.Join(root, "tsconfig.json"), `{"compilerOptions":{"allowJs":true},"include":["**/*.ts"]}`)
	parse(true, true)
	if _, err := os.Stat(configPath); !os.IsNotExist(err) {
		t.Fatalf("inferred config remains with user tsconfig: %v", err)
	}
}

func TestSCIPClangPreFilterIntegration(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 with the Clang :dev image available locally")
	}
	useLocalSCIPDevImage(t)
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	root := filepath.Join(t.TempDir(), "project, work")
	global := t.TempDir()
	for name, body := range map[string]string{
		".gitignore":        "Skip.c\nnested/*.c\n!nested/Keep.c\n",
		"Keep.c":            "int keep(void) { return 1; }\n",
		"Skip.c":            "int skip(void) { return 2; }\n",
		"nested/.astignore": "*.c\n!Keep.c\n",
		"nested/Keep.c":     "int nested_keep(void) { return 3; }\n",
		"nested/Skip.c":     "int nested_skip(void) { return 4; }\n",
	} {
		writeSCIPJavaFixture(t, filepath.Join(root, name), body)
	}
	var commands []map[string]any
	for _, rel := range []string{"Keep.c", "Skip.c", "nested/Keep.c", "nested/Skip.c"} {
		source := filepath.Join(root, rel)
		commands = append(commands, map[string]any{"directory": root, "file": source, "arguments": []string{"clang", "-c", source}})
	}
	compdb, err := json.Marshal(commands)
	if err != nil {
		t.Fatal(err)
	}
	writeSCIPJavaFixture(t, filepath.Join(root, "compile_commands.json"), string(compdb))
	before := snapshotSCIPJavaFixture(t, root)
	t.Cleanup(func() { _ = exec.Command("docker", "container", "rm", "-f", scipContainerName(root, "clang")).Run() })
	entries, failures := prepareSCIPEntries(context.Background(), root, global, []string{filepath.Join(root, "Keep.c")}, nil)
	if len(failures) != 0 {
		t.Fatalf("Clang indexer failed: %v", failures)
	}
	if entries[filepath.Join(root, "Keep.c")] == nil || entries[filepath.Join(root, "nested", "Keep.c")] == nil || entries[filepath.Join(root, "Skip.c")] != nil || entries[filepath.Join(root, "nested", "Skip.c")] != nil {
		t.Fatalf("wrong Clang graph entries: %v", entries)
	}
	cacheDir := scipCacheDir(root, global, "clang")
	manifest, err := os.ReadFile(filepath.Join(cacheDir, "graphit-allowed-files.json"))
	if err != nil {
		t.Fatal(err)
	}
	var allowed []string
	if err := json.Unmarshal(manifest, &allowed); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(allowed, []string{"Keep.c", "nested/Keep.c"}) {
		t.Fatalf("wrong Clang allowlist: %v", allowed)
	}
	derived, err := os.ReadFile(filepath.Join(cacheDir, "graphit-compile-commands.json"))
	if err != nil {
		t.Fatal(err)
	}
	var selected []struct {
		File string `json:"file"`
	}
	if err := json.Unmarshal(derived, &selected); err != nil {
		t.Fatal(err)
	}
	if len(selected) != 2 || selected[0].File != "/workspace/Keep.c" || selected[1].File != "/workspace/nested/Keep.c" {
		t.Fatalf("wrong derived compilation database: %+v", selected)
	}
	raw, err := os.ReadFile(filepath.Join(scipOutputDir(root, global, "clang"), "index.scip"))
	if err != nil {
		t.Fatal(err)
	}
	index := &scip.Index{}
	if err := proto.Unmarshal(raw, index); err != nil {
		t.Fatal(err)
	}
	if len(index.Documents) != 2 {
		t.Fatalf("Clang raw documents=%d, want 2", len(index.Documents))
	}
	unfiltered, err := runSCIPImage(context.Background(), root, global, "clang")
	if err != nil {
		t.Fatalf("unfiltered Clang comparison failed: %v", err)
	}
	all := &scip.Index{}
	if err := proto.Unmarshal(unfiltered, all); err != nil {
		t.Fatal(err)
	}
	if len(all.Documents) != 4 || len(raw) >= len(unfiltered) {
		t.Fatalf("Clang prefilter did not reduce raw index: filtered=%d B/%d docs, all=%d B/%d docs", len(raw), len(index.Documents), len(unfiltered), len(all.Documents))
	}
	t.Logf("Clang fixture raw index: filtered %d B/%d docs, unfiltered %d B/%d docs", len(raw), len(index.Documents), len(unfiltered), len(all.Documents))
	if after := snapshotSCIPJavaFixture(t, root); !reflect.DeepEqual(before, after) {
		t.Fatalf("Clang changed source: before=%v after=%v", before, after)
	}
}

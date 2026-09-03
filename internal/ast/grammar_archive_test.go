package ast

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

const testGoGrammarSOPath = "../../.build/grammars/treesitter/tree-sitter-go.so"

func skipIfNoGrammarSO(t *testing.T, path string) {
	t.Helper()
	if _, err := os.Stat(path); os.IsNotExist(err) {
		t.Skipf("shared library not found: %s", path)
	}
}

func skipIfNoGrammarSOBench(b *testing.B, path string) {
	b.Helper()
	if _, err := os.Stat(path); os.IsNotExist(err) {
		b.Skipf("shared library not found: %s", path)
	}
}

func createTestArchive(t *testing.T) (archivePath string, originalData []byte) {
	t.Helper()
	skipIfNoGrammarSO(t, testGoGrammarSOPath)

	data, err := os.ReadFile(testGoGrammarSOPath)
	if err != nil {
		t.Fatalf("read Go grammar: %v", err)
	}

	archivePath = filepath.Join(t.TempDir(), "tree-sitter-go.grammar")
	platforms := []GrammarPlatform{
		{
			OS:         runtime.GOOS,
			Arch:       runtime.GOARCH,
			SymbolName: "tree_sitter_go",
			Data:       data,
		},
	}

	if err := WriteGrammarArchive(archivePath, platforms); err != nil {
		t.Fatalf("WriteGrammarArchive: %v", err)
	}

	return archivePath, data
}

func TestGrammarArchive_WriteRead(t *testing.T) {
	skipIfNoGrammarSO(t, testGoGrammarSOPath)

	goData, err := os.ReadFile(testGoGrammarSOPath)
	if err != nil {
		t.Fatalf("read Go grammar: %v", err)
	}

	platforms := []GrammarPlatform{
		{OS: "linux", Arch: "amd64", SymbolName: "tree_sitter_go", Data: goData},
		{OS: "darwin", Arch: "arm64", SymbolName: "tree_sitter_go", Data: goData},
		{OS: "windows", Arch: "amd64", SymbolName: "tree_sitter_go", Data: goData},
	}

	archivePath := filepath.Join(t.TempDir(), "tree-sitter-go.grammar")
	if err := WriteGrammarArchive(archivePath, platforms); err != nil {
		t.Fatalf("WriteGrammarArchive: %v", err)
	}

	archive, err := ReadGrammarArchive(archivePath)
	if err != nil {
		t.Fatalf("ReadGrammarArchive: %v", err)
	}

	if len(archive.Platforms) != 3 {
		t.Fatalf("expected 3 platforms, got %d", len(archive.Platforms))
	}

	expected := []struct{ os, arch, sym string }{
		{"linux", "amd64", "tree_sitter_go"},
		{"darwin", "arm64", "tree_sitter_go"},
		{"windows", "amd64", "tree_sitter_go"},
	}
	for i, e := range expected {
		p := archive.Platforms[i]
		if p.OS != e.os || p.Arch != e.arch || p.SymbolName != e.sym {
			t.Errorf("platform[%d]: got %s/%s sym=%s, want %s/%s sym=%s",
				i, p.OS, p.Arch, p.SymbolName, e.os, e.arch, e.sym)
		}
	}

	info, _ := os.Stat(archivePath)
	originalSize := int64(len(goData)) * 3
	t.Logf("Archive: %d bytes (3 × %d = %d original, ratio=%.2f%%)",
		info.Size(), len(goData), originalSize, float64(info.Size())/float64(originalSize)*100)
}

func TestGrammarArchive_Extract(t *testing.T) {
	archivePath, originalData := createTestArchive(t)
	cacheDir := filepath.Join(t.TempDir(), "cache")

	archive, err := ReadGrammarArchive(archivePath)
	if err != nil {
		t.Fatalf("ReadGrammarArchive: %v", err)
	}

	extractedPath, err := archive.ExtractForCurrentPlatform(archivePath, cacheDir)
	if err != nil {
		t.Fatalf("ExtractForCurrentPlatform: %v", err)
	}

	if _, err := os.Stat(extractedPath); err != nil {
		t.Fatalf("extracted file not found: %v", err)
	}

	extractedData, err := os.ReadFile(extractedPath)
	if err != nil {
		t.Fatalf("read extracted file: %v", err)
	}

	if !bytes.Equal(extractedData, originalData) {
		t.Errorf("extracted data mismatch: got %d bytes, want %d bytes", len(extractedData), len(originalData))
	}

	t.Logf("Extracted %s (%d bytes)", extractedPath, len(extractedData))
}

func TestGrammarArchive_CacheHit(t *testing.T) {
	archivePath, _ := createTestArchive(t)
	cacheDir := filepath.Join(t.TempDir(), "cache")

	archive, err := ReadGrammarArchive(archivePath)
	if err != nil {
		t.Fatalf("ReadGrammarArchive: %v", err)
	}

	path1, err := archive.ExtractForCurrentPlatform(archivePath, cacheDir)
	if err != nil {
		t.Fatalf("first extract: %v", err)
	}

	info1, err := os.Stat(path1)
	if err != nil {
		t.Fatalf("stat cached file: %v", err)
	}

	path2, err := archive.ExtractForCurrentPlatform(archivePath, cacheDir)
	if err != nil {
		t.Fatalf("second extract: %v", err)
	}

	if path1 != path2 {
		t.Errorf("cache path mismatch: %q vs %q", path1, path2)
	}

	info2, err := os.Stat(path2)
	if err != nil {
		t.Fatalf("stat cached file (2nd): %v", err)
	}

	if !info1.ModTime().Equal(info2.ModTime()) {
		t.Error("cached file was rewritten on second extraction (mod time changed)")
	}

	t.Log("Cache hit verified: second extraction returned cached path without rewriting")
}

func TestGrammarArchive_MissingPlatform(t *testing.T) {
	skipIfNoGrammarSO(t, testGoGrammarSOPath)

	data, err := os.ReadFile(testGoGrammarSOPath)
	if err != nil {
		t.Fatalf("read Go grammar: %v", err)
	}

	platforms := []GrammarPlatform{
		{OS: "plan9", Arch: "mips", SymbolName: "tree_sitter_go", Data: data},
	}

	archivePath := filepath.Join(t.TempDir(), "tree-sitter-go.grammar")
	if err := WriteGrammarArchive(archivePath, platforms); err != nil {
		t.Fatalf("WriteGrammarArchive: %v", err)
	}

	archive, err := ReadGrammarArchive(archivePath)
	if err != nil {
		t.Fatalf("ReadGrammarArchive: %v", err)
	}

	cacheDir := filepath.Join(t.TempDir(), "cache")
	_, err = archive.ExtractForCurrentPlatform(archivePath, cacheDir)
	if err == nil {
		t.Fatal("expected error for missing platform, got nil")
	}

	t.Logf("Expected error: %v", err)
}

// TestDynGrammarLoader_LoadFromArchive extracts a grammar from a .grammar archive,
// then loads the extracted shared library and parses Go source.
func TestDynGrammarLoader_LoadFromArchive(t *testing.T) {
	archivePath, _ := createTestArchive(t)

	archive, err := ReadGrammarArchive(archivePath)
	if err != nil {
		t.Fatalf("ReadGrammarArchive failed: %v", err)
	}

	cacheDir := filepath.Join(t.TempDir(), "cache")
	extractedPath, err := archive.ExtractForCurrentPlatform(archivePath, cacheDir)
	if err != nil {
		t.Fatalf("ExtractForCurrentPlatform failed: %v", err)
	}

	loader := NewDynGrammarLoader()
	defer loader.Close()

	lang, err := loader.LoadFromPath("go", extractedPath)
	if err != nil {
		t.Fatalf("LoadFromPath failed: %v", err)
	}
	if lang == nil {
		t.Fatal("loaded language is nil")
	}

	parser := sitter.NewParser()
	_ = parser.SetLanguage(lang)

	tree, err := tsParse(parser, []byte(testGoSource))
	if err != nil {
		t.Fatalf("ParseCtx failed: %v", err)
	}
	if tree == nil {
		t.Fatal("parse tree is nil")
	}
	defer tree.Close()

	root := tree.RootNode()
	if root.Kind() != "source_file" {
		t.Errorf("expected root type 'source_file', got %q", root.Kind())
	}
	if root.ChildCount() == 0 {
		t.Error("expected root node to have children")
	}

	nativeLang := NativeLanguage("go")
	nativeParser := sitter.NewParser()
	_ = nativeParser.SetLanguage(nativeLang)

	nativeTree, err := tsParse(nativeParser, []byte(testGoSource))
	if err != nil {
		t.Fatalf("native parse failed: %v", err)
	}
	defer nativeTree.Close()

	nativeSexp := nativeTree.RootNode().ToSexp()
	archiveSexp := root.ToSexp()
	if nativeSexp != archiveSexp {
		t.Errorf("S-expression mismatch between native and archive-extracted grammar")
	} else {
		t.Log("Archive-extracted grammar produces identical parse trees as native ✓")
	}
}

func TestGrammarArchive_InvalidMagic(t *testing.T) {
	path := filepath.Join(t.TempDir(), "bad.grammar")
	if err := os.WriteFile(path, []byte("BADMxxxxxxxx"), 0o644); err != nil {
		t.Fatal(err)
	}

	_, err := ReadGrammarArchive(path)
	if err == nil {
		t.Fatal("expected error for invalid magic")
	}
	t.Logf("Expected error: %v", err)
}

func TestGrammarArchive_EmptyPlatforms(t *testing.T) {
	path := filepath.Join(t.TempDir(), "empty.grammar")
	err := WriteGrammarArchive(path, nil)
	if err == nil {
		t.Fatal("expected error for empty platforms")
	}
	t.Logf("Expected error: %v", err)
}

func BenchmarkGrammarArchive_Extract(b *testing.B) {
	skipIfNoGrammarSOBench(b, testGoGrammarSOPath)

	goData, err := os.ReadFile(testGoGrammarSOPath)
	if err != nil {
		b.Fatalf("read Go grammar: %v", err)
	}

	platforms := []GrammarPlatform{
		{OS: runtime.GOOS, Arch: runtime.GOARCH, SymbolName: "tree_sitter_go", Data: goData},
	}

	archivePath := filepath.Join(b.TempDir(), "tree-sitter-go.grammar")
	if err := WriteGrammarArchive(archivePath, platforms); err != nil {
		b.Fatalf("WriteGrammarArchive: %v", err)
	}

	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		cacheDir := filepath.Join(b.TempDir(), "cache", fmt.Sprintf("%d", i))

		archive, err := ReadGrammarArchive(archivePath)
		if err != nil {
			b.Fatalf("ReadGrammarArchive: %v", err)
		}

		_, err = archive.ExtractForCurrentPlatform(archivePath, cacheDir)
		if err != nil {
			b.Fatalf("ExtractForCurrentPlatform: %v", err)
		}
	}
}

// BenchmarkGrammarArchive_LoadAndParse measures full flow:
// open .grammar → extract → load via CGO dlopen → parse
// (extraction is cached after first iter, so this mostly measures cache-hit + parse)
func BenchmarkGrammarArchive_LoadAndParse(b *testing.B) {
	skipIfNoGrammarSOBench(b, testGoGrammarSOPath)

	goData, err := os.ReadFile(testGoGrammarSOPath)
	if err != nil {
		b.Fatalf("read Go grammar: %v", err)
	}

	archiveDir := b.TempDir()
	archivePath := filepath.Join(archiveDir, "tree-sitter-go.grammar")
	platforms := []GrammarPlatform{
		{OS: runtime.GOOS, Arch: runtime.GOARCH, SymbolName: "tree_sitter_go", Data: goData},
	}
	if err := WriteGrammarArchive(archivePath, platforms); err != nil {
		b.Fatalf("WriteGrammarArchive: %v", err)
	}

	cacheDir := filepath.Join(b.TempDir(), "cache")
	src := []byte(testGoSource)

	archive, err := ReadGrammarArchive(archivePath)
	if err != nil {
		b.Fatalf("ReadGrammarArchive: %v", err)
	}
	extractedPath, err := archive.ExtractForCurrentPlatform(archivePath, cacheDir)
	if err != nil {
		b.Fatalf("initial extract: %v", err)
	}

	loader := NewDynGrammarLoader()
	defer loader.Close()

	lang, err := loader.LoadFromPath("go", extractedPath)
	if err != nil {
		b.Fatalf("LoadFromPath: %v", err)
	}

	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		parser := sitter.NewParser()
		_ = parser.SetLanguage(lang)
		tree, err := tsParse(parser, src)
		if err != nil {
			b.Fatalf("parse failed: %v", err)
		}
		tree.Close()
	}
}

func BenchmarkTS_Parse_NativeImport(b *testing.B) {
	lang := NativeLanguage("go")
	src := []byte(testGoSource)

	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		parser := sitter.NewParser()
		_ = parser.SetLanguage(lang)
		tree, err := tsParse(parser, src)
		if err != nil {
			b.Fatalf("parse failed: %v", err)
		}
		tree.Close()
	}
}

func BenchmarkTS_Parse_SharedLib(b *testing.B) {
	skipIfNoGrammarSOBench(b, testGoGrammarSOPath)

	loader := NewDynGrammarLoader()
	defer loader.Close()

	lang, err := loader.LoadFromPath("go", testGoGrammarSOPath)
	if err != nil {
		b.Fatalf("LoadFromPath: %v", err)
	}

	src := []byte(testGoSource)

	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		parser := sitter.NewParser()
		_ = parser.SetLanguage(lang)
		tree, err := tsParse(parser, src)
		if err != nil {
			b.Fatalf("parse failed: %v", err)
		}
		tree.Close()
	}
}

// BenchmarkTS_Parse_GrammarArchive benchmarks the full grammar archive path:
// .grammar archive → extract (cached) → CGO dlopen load → parse.
func BenchmarkTS_Parse_GrammarArchive(b *testing.B) {
	skipIfNoGrammarSOBench(b, testGoGrammarSOPath)

	goData, err := os.ReadFile(testGoGrammarSOPath)
	if err != nil {
		b.Fatalf("read Go grammar: %v", err)
	}

	archiveDir := b.TempDir()
	archivePath := filepath.Join(archiveDir, "tree-sitter-go.grammar")
	platforms := []GrammarPlatform{
		{OS: runtime.GOOS, Arch: runtime.GOARCH, SymbolName: "tree_sitter_go", Data: goData},
	}
	if err := WriteGrammarArchive(archivePath, platforms); err != nil {
		b.Fatalf("WriteGrammarArchive: %v", err)
	}

	cacheDir := filepath.Join(b.TempDir(), "cache")
	src := []byte(testGoSource)

	archive, err := ReadGrammarArchive(archivePath)
	if err != nil {
		b.Fatalf("ReadGrammarArchive: %v", err)
	}
	extractedPath, err := archive.ExtractForCurrentPlatform(archivePath, cacheDir)
	if err != nil {
		b.Fatalf("extract: %v", err)
	}

	loader := NewDynGrammarLoader()
	defer loader.Close()

	lang, err := loader.LoadFromPath("go", extractedPath)
	if err != nil {
		b.Fatalf("LoadFromPath: %v", err)
	}

	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		parser := sitter.NewParser()
		_ = parser.SetLanguage(lang)
		tree, err := tsParse(parser, src)
		if err != nil {
			b.Fatalf("parse failed: %v", err)
		}
		tree.Close()
	}
}

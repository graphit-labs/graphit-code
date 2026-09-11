package ast

import (
	"bytes"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

var grammarBinaryFixture = bytes.Repeat([]byte{0x47, 0x52, 0x41, 0x4d}, 256)

func createTestArchive(t *testing.T) (string, []byte) {
	t.Helper()
	data := append([]byte(nil), grammarBinaryFixture...)
	path := filepath.Join(t.TempDir(), "tree-sitter-go.grammar")
	platforms := []GrammarPlatform{{
		OS: runtime.GOOS, Arch: runtime.GOARCH, SymbolName: "tree_sitter_go", Data: data,
	}}
	if err := WriteGrammarArchive(path, platforms); err != nil {
		t.Fatalf("WriteGrammarArchive: %v", err)
	}
	return path, data
}

func TestGrammarArchiveWriteRead(t *testing.T) {
	platforms := []GrammarPlatform{
		{OS: "linux", Arch: "amd64", SymbolName: "tree_sitter_go", Data: grammarBinaryFixture},
		{OS: "darwin", Arch: "arm64", SymbolName: "tree_sitter_go", Data: grammarBinaryFixture},
		{OS: "windows", Arch: "amd64", SymbolName: "tree_sitter_go", Data: grammarBinaryFixture},
	}
	path := filepath.Join(t.TempDir(), "tree-sitter-go.grammar")
	if err := WriteGrammarArchive(path, platforms); err != nil {
		t.Fatalf("WriteGrammarArchive: %v", err)
	}

	archive, err := ReadGrammarArchive(path)
	if err != nil {
		t.Fatalf("ReadGrammarArchive: %v", err)
	}
	if len(archive.Platforms) != len(platforms) {
		t.Fatalf("platform count = %d, want %d", len(archive.Platforms), len(platforms))
	}
	for i, want := range platforms {
		got := archive.Platforms[i]
		if got.OS != want.OS || got.Arch != want.Arch || got.SymbolName != want.SymbolName {
			t.Errorf("platform[%d] = %s/%s %s, want %s/%s %s",
				i, got.OS, got.Arch, got.SymbolName, want.OS, want.Arch, want.SymbolName)
		}
	}
}

func TestGrammarArchiveExtract(t *testing.T) {
	archivePath, original := createTestArchive(t)
	archive, err := ReadGrammarArchive(archivePath)
	if err != nil {
		t.Fatalf("ReadGrammarArchive: %v", err)
	}
	extractedPath, err := archive.ExtractForCurrentPlatform(archivePath, t.TempDir())
	if err != nil {
		t.Fatalf("ExtractForCurrentPlatform: %v", err)
	}
	extracted, err := os.ReadFile(extractedPath)
	if err != nil {
		t.Fatalf("read extracted file: %v", err)
	}
	if !bytes.Equal(extracted, original) {
		t.Fatalf("extracted data has %d bytes, want %d", len(extracted), len(original))
	}
}

func TestGrammarArchiveCacheHit(t *testing.T) {
	archivePath, _ := createTestArchive(t)
	archive, err := ReadGrammarArchive(archivePath)
	if err != nil {
		t.Fatalf("ReadGrammarArchive: %v", err)
	}
	cacheDir := t.TempDir()
	first, err := archive.ExtractForCurrentPlatform(archivePath, cacheDir)
	if err != nil {
		t.Fatalf("first extract: %v", err)
	}
	firstInfo, err := os.Stat(first)
	if err != nil {
		t.Fatalf("stat first extract: %v", err)
	}
	second, err := archive.ExtractForCurrentPlatform(archivePath, cacheDir)
	if err != nil {
		t.Fatalf("second extract: %v", err)
	}
	secondInfo, err := os.Stat(second)
	if err != nil {
		t.Fatalf("stat second extract: %v", err)
	}
	if first != second {
		t.Fatalf("cache paths differ: %q and %q", first, second)
	}
	if !firstInfo.ModTime().Equal(secondInfo.ModTime()) {
		t.Fatal("cached grammar was rewritten")
	}
}

func TestGrammarArchiveMissingPlatform(t *testing.T) {
	path := filepath.Join(t.TempDir(), "tree-sitter-go.grammar")
	platforms := []GrammarPlatform{{
		OS: "plan9", Arch: "mips", SymbolName: "tree_sitter_go", Data: grammarBinaryFixture,
	}}
	if err := WriteGrammarArchive(path, platforms); err != nil {
		t.Fatalf("WriteGrammarArchive: %v", err)
	}
	archive, err := ReadGrammarArchive(path)
	if err != nil {
		t.Fatalf("ReadGrammarArchive: %v", err)
	}
	if _, err := archive.ExtractForCurrentPlatform(path, t.TempDir()); err == nil {
		t.Fatal("ExtractForCurrentPlatform accepted an archive for a different platform")
	}
}

func TestGrammarArchiveInvalidMagic(t *testing.T) {
	path := filepath.Join(t.TempDir(), "bad.grammar")
	if err := os.WriteFile(path, []byte("BADMxxxxxxxx"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := ReadGrammarArchive(path); err == nil {
		t.Fatal("ReadGrammarArchive accepted invalid magic")
	}
}

func TestGrammarArchiveEmptyPlatforms(t *testing.T) {
	path := filepath.Join(t.TempDir(), "empty.grammar")
	if err := WriteGrammarArchive(path, nil); err == nil {
		t.Fatal("WriteGrammarArchive accepted an empty platform list")
	}
}

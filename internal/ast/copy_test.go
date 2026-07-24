package ast

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

func TestCopyDBDir_File(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "ladybugdb")
	want := []byte("ladybug store bytes \x00\x01\x02\xff")
	if err := os.WriteFile(src, want, 0o644); err != nil {
		t.Fatal(err)
	}
	dst := filepath.Join(dir, "ladybugdb.copy")
	if err := CopyDBDir(src, dst); err != nil {
		t.Fatalf("CopyDBDir file: %v", err)
	}
	got, err := os.ReadFile(dst)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, want) {
		t.Errorf("copied content mismatch: got %q want %q", got, want)
	}
}

func TestCopyDBDir_DirTree(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "store")
	if err := os.MkdirAll(filepath.Join(src, "sub"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "a.bin"), []byte("AAAA"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "sub", "b.bin"), []byte("BBBB"), 0o644); err != nil {
		t.Fatal(err)
	}
	dst := filepath.Join(dir, "store.copy")
	if err := CopyDBDir(src, dst); err != nil {
		t.Fatalf("CopyDBDir dir: %v", err)
	}
	if b, _ := os.ReadFile(filepath.Join(dst, "a.bin")); string(b) != "AAAA" {
		t.Errorf("a.bin = %q, want AAAA", b)
	}
	if b, _ := os.ReadFile(filepath.Join(dst, "sub", "b.bin")); string(b) != "BBBB" {
		t.Errorf("sub/b.bin = %q, want BBBB", b)
	}
}

// TestCopyDBDir_OverwritesExisting ensures a stale destination is fully replaced
// (not partially overwritten), which the atomic-swap flow relies on.
func TestCopyDBDir_OverwritesExisting(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "ladybugdb")
	if err := os.WriteFile(src, []byte("new"), 0o644); err != nil {
		t.Fatal(err)
	}
	dst := filepath.Join(dir, "ladybugdb.copy")
	if err := os.WriteFile(dst, []byte("stale-and-longer"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := CopyDBDir(src, dst); err != nil {
		t.Fatalf("CopyDBDir overwrite: %v", err)
	}
	if b, _ := os.ReadFile(dst); string(b) != "new" {
		t.Errorf("dst = %q, want new", b)
	}
}

package hub

import (
	"os"
	"path/filepath"
	"testing"
)

func TestHashFile(t *testing.T) {
	t.Parallel()

	t.Run("valid file", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		f := filepath.Join(dir, "test.txt")
		if err := os.WriteFile(f, []byte("hello"), 0o644); err != nil {
			t.Fatal(err)
		}
		hash, err := HashFile(f)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if hash == "" {
			t.Error("expected non-empty hash")
		}
		// Same file should give same hash
		hash2, err := HashFile(f)
		if err != nil {
			t.Fatal(err)
		}
		if hash != hash2 {
			t.Errorf("expected same hash, got %q and %q", hash, hash2)
		}
	})

	t.Run("nonexistent file", func(t *testing.T) {
		t.Parallel()
		_, err := HashFile("/nonexistent")
		if err == nil {
			t.Error("expected error")
		}
	})
}

func TestHashPath(t *testing.T) {
	t.Parallel()

	t.Run("file", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		f := filepath.Join(dir, "test.txt")
		if err := os.WriteFile(f, []byte("hello"), 0o644); err != nil {
			t.Fatal(err)
		}
		hash, err := HashPath(f)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if hash == "" {
			t.Error("expected non-empty hash")
		}
	})

	t.Run("directory", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("a"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, "b.txt"), []byte("b"), 0o644); err != nil {
			t.Fatal(err)
		}
		hash, err := HashPath(dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if hash == "" {
			t.Error("expected non-empty hash")
		}
	})

	t.Run("empty directory", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_, err := HashPath(dir)
		if err == nil {
			t.Error("expected error for empty directory")
		}
	})

	t.Run("nonexistent path", func(t *testing.T) {
		t.Parallel()
		_, err := HashPath("/nonexistent")
		if err == nil {
			t.Error("expected error")
		}
	})
}

func TestHashDirectory(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "test.txt"), []byte("test"), 0o644); err != nil {
		t.Fatal(err)
	}
	hash, err := HashDirectory(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if hash == "" {
		t.Error("expected non-empty hash")
	}
}

func TestTruncateHash(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		hash string
		n    int
		want string
	}{
		{name: "truncate", hash: "abcdefgh", n: 4, want: "abcd"},
		{name: "exact", hash: "abcd", n: 4, want: "abcd"},
		{name: "shorter", hash: "ab", n: 4, want: "ab"},
		{name: "empty", hash: "", n: 4, want: ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if got := TruncateHash(tt.hash, tt.n); got != tt.want {
				t.Errorf("TruncateHash(%q, %d) = %q, want %q", tt.hash, tt.n, got, tt.want)
			}
		})
	}
}

func TestVerifyHash(t *testing.T) {
	t.Parallel()

	t.Run("empty expected hash", func(t *testing.T) {
		t.Parallel()
		ok, err := VerifyHash("/nonexistent", "")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if !ok {
			t.Error("expected true for empty expected hash")
		}
	})

	t.Run("matching hash", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		f := filepath.Join(dir, "test.txt")
		if err := os.WriteFile(f, []byte("hello"), 0o644); err != nil {
			t.Fatal(err)
		}
		hash, _ := HashFile(f)
		ok, err := VerifyHash(f, hash)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if !ok {
			t.Error("expected true for matching hash")
		}
	})

	t.Run("non-matching hash", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		f := filepath.Join(dir, "test.txt")
		if err := os.WriteFile(f, []byte("hello"), 0o644); err != nil {
			t.Fatal(err)
		}
		ok, err := VerifyHash(f, "wronghash")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if ok {
			t.Error("expected false for non-matching hash")
		}
	})

	t.Run("nonexistent path", func(t *testing.T) {
		t.Parallel()
		_, err := VerifyHash("/nonexistent", "somehash")
		if err == nil {
			t.Error("expected error for nonexistent path")
		}
	})
}

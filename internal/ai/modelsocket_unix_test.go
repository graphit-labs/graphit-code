//go:build !windows

package ai

import (
	"os"
	"path/filepath"
	"testing"
)

func TestPrivateSocketDirIsCreatedForThisUserOnly(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "graphit-sock")
	if err := ensurePrivateDir(dir); err != nil {
		t.Fatal(err)
	}
	if info, err := os.Stat(dir); err != nil || info.Mode().Perm() != 0o700 {
		t.Fatalf("mode = %v, %v", info.Mode().Perm(), err)
	}

	// An existing directory of ours that others can write to is tightened.
	open := filepath.Join(t.TempDir(), "open")
	if err := os.Mkdir(open, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(open, 0o777); err != nil {
		t.Fatal(err)
	}
	if err := ensurePrivateDir(open); err != nil {
		t.Fatal(err)
	}
	if info, _ := os.Stat(open); info.Mode().Perm() != 0o700 {
		t.Fatalf("open directory kept mode %v", info.Mode().Perm())
	}
}

// In a shared temporary directory another user could prepare the directory in advance
// and swap the socket inside it; such a directory is never used.
func TestPrivateSocketDirRefusesAnotherUsersDirectoryOrASymlink(t *testing.T) {
	foreign := filepath.Join(t.TempDir(), "foreign")
	if err := os.Mkdir(foreign, 0o777); err != nil {
		t.Fatal(err)
	}
	old := currentUID
	currentUID = func() int { return os.Getuid() + 1 }
	err := ensurePrivateDir(foreign)
	currentUID = old
	if err == nil {
		t.Fatal("a directory owned by another user was accepted")
	}
	if info, _ := os.Stat(foreign); info.Mode().Perm() == 0o700 {
		t.Fatal("another user's directory was modified")
	}

	target := t.TempDir()
	link := filepath.Join(t.TempDir(), "link")
	if err := os.Symlink(target, link); err != nil {
		t.Fatal(err)
	}
	if err := ensurePrivateDir(link); err == nil {
		t.Fatal("a symlink was accepted as the socket directory")
	}
}

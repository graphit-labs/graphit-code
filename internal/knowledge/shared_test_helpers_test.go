package knowledge

import "testing"

// Helpers that need no search engine, so they must not sit behind `//go:build lancedb`:
// an untagged test depending on one makes the whole package fail to COMPILE without the tag.

func isolateHome(t *testing.T) string {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	return home
}

// publishFrom compiles a docs tree the way a producer does, then returns the directory
// that would be pushed to the branch — the compiled wiki without its database, which is
// what SyncCopyDirExcept(wiki.IsDerivedFile) leaves behind.

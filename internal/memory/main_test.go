package memory

import (
	"os"
	"testing"
)

// TestMain isolates the whole package's tests from the developer's real environment.
//
// The reason is not flakiness, it is reach. A memory store reads the operator's global config, and
// when that config names a bucket — as a real installation does — Publish starts a BACKGROUND
// goroutine that uploads into it. So a test run on a configured machine would write test memories
// into a real bucket. Pointing HOME at a temporary directory leaves the bucket unset, which is the
// condition Publish already checks before doing anything.
//
// This used to be about git as well: it disabled git's automatic maintenance and exported a git
// identity, because the store initialised a repository and committed, and a `gc --auto` triggered
// by that commit could outlive the test and race t.TempDir's removal. There is no repository, no
// commit and no git identity now — the unit identity is a generated ULID under HOME, which this
// isolation covers for free.
//
// Tests that set HOME themselves still win — this is the floor, not a ceiling.
func TestMain(m *testing.M) {
	home, err := os.MkdirTemp("", "graphit-memory-test-home-")
	if err == nil {
		_ = os.Setenv("HOME", home)
		_ = os.Setenv("USERPROFILE", home)
	}

	code := m.Run()

	// Cleaned up here rather than with defer: os.Exit does not run deferred functions, so a defer
	// would leak this directory on every run.
	if home != "" {
		_ = os.RemoveAll(home)
	}
	os.Exit(code)
}

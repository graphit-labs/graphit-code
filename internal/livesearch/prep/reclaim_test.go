package prep

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/store"
)

func seedResidue(t *testing.T, sessionID string) []string {
	t.Helper()
	dirs := []string{
		store.ASTProjectDirByID(sessionID),
		store.KnowledgeProjectDirByID(sessionID),
		store.MemoryWikiDir("project", sessionID),
		store.MemoryTableDir("project", sessionID),
	}
	for _, d := range dirs {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatalf("setup: %v", err)
		}
		if err := os.WriteFile(filepath.Join(d, "leftover"), []byte("x"), 0o600); err != nil {
			t.Fatalf("setup: %v", err)
		}
	}
	return dirs
}

func TestReclaimCollectsEveryStoreKeyedByTheSession(t *testing.T) {
	isolateHome(t)
	dirs := seedResidue(t, "01OLDSESSION")

	Reclaim("01OLDSESSION")

	for _, d := range dirs {
		if _, err := os.Stat(d); err == nil {
			t.Errorf("%s survived the reclaim", d)
		}
	}
}

func TestReclaimTouchesNothingBelongingToAnotherID(t *testing.T) {
	isolateHome(t)
	mine := seedResidue(t, "01MINE")
	theirs := seedResidue(t, "01THEIRS")

	Reclaim("01MINE")

	for _, d := range mine {
		if _, err := os.Stat(d); err == nil {
			t.Errorf("%s survived its own reclaim", d)
		}
	}
	for _, d := range theirs {
		if _, err := os.Stat(d); err != nil {
			t.Errorf("%s was deleted by another session's reclaim", d)
		}
	}
}

func TestReclaimingNothingIsNotAFailure(t *testing.T) {
	isolateHome(t)
	Reclaim("01NEVEREXISTED")
	Reclaim("")
}

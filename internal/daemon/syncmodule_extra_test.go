package daemon

import (
	"testing"
)

// ---------------------------------------------------------------------------
// SyncModule — constructor fields
// ---------------------------------------------------------------------------

func TestWorktreeDirForBranch_EmptyBranch(t *testing.T) {
	t.Parallel()
	got := worktreeDirForBranch("/tmp/wt", "")
	if got != "/tmp/wt/" {
		// filepath.Join("/tmp/wt", "") = "/tmp/wt"
		if got != "/tmp/wt" {
			t.Errorf("expected '/tmp/wt' or '/tmp/wt/', got %q", got)
		}
	}
}

// ---------------------------------------------------------------------------
// parseBranch — edge cases
// ---------------------------------------------------------------------------

func TestParseBranch_EmptyString(t *testing.T) {
	t.Parallel()
	scope, id := parseBranch("")
	if scope != "project" {
		t.Errorf("scope: expected 'project', got %q", scope)
	}
	if id != "" {
		t.Errorf("id: expected empty, got %q", id)
	}
}

func TestParseBranch_SingleSegment(t *testing.T) {
	t.Parallel()
	scope, id := parseBranch("main")
	if scope != "project" {
		t.Errorf("scope: expected 'project', got %q", scope)
	}
	if id != "" {
		t.Errorf("id: expected empty, got %q", id)
	}
}

func TestParseBranch_TwoSegments(t *testing.T) {
	t.Parallel()
	scope, id := parseBranch("memory/project")
	if scope != "project" {
		t.Errorf("scope: expected 'project', got %q", scope)
	}
	if id != "" {
		t.Errorf("id: expected empty, got %q", id)
	}
}

func TestParseBranch_FourSegments(t *testing.T) {
	t.Parallel()
	scope, id := parseBranch("memory/user/john/extra")
	if scope != "user" {
		t.Errorf("scope: expected 'user', got %q", scope)
	}
	if id != "john/extra" {
		t.Errorf("id: expected 'john/extra', got %q", id)
	}
}

// ---------------------------------------------------------------------------
// MemorySyncModule — constructor
// ---------------------------------------------------------------------------

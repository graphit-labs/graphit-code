package daemon

import (
	"testing"
)

// SyncModule — constructor fields

func TestScopeDir_EmptyBranch(t *testing.T) {
	t.Parallel()
	got := scopeDir("/tmp/wt", "")
	if got != "/tmp/wt/" {
		// filepath.Join("/tmp/wt", "") = "/tmp/wt"
		if got != "/tmp/wt" {
			t.Errorf("expected '/tmp/wt' or '/tmp/wt/', got %q", got)
		}
	}
}

func TestParseBranch_EmptyString(t *testing.T) {
	t.Parallel()
	scope, id := parseScopePath("")
	if scope != "project" {
		t.Errorf("scope: expected 'project', got %q", scope)
	}
	if id != "" {
		t.Errorf("id: expected empty, got %q", id)
	}
}

func TestParseBranch_SingleSegment(t *testing.T) {
	t.Parallel()
	scope, id := parseScopePath("main")
	if scope != "project" {
		t.Errorf("scope: expected 'project', got %q", scope)
	}
	if id != "" {
		t.Errorf("id: expected empty, got %q", id)
	}
}

func TestParseBranch_TwoSegments(t *testing.T) {
	t.Parallel()
	scope, id := parseScopePath("memory/project")
	if scope != "project" {
		t.Errorf("scope: expected 'project', got %q", scope)
	}
	if id != "" {
		t.Errorf("id: expected empty, got %q", id)
	}
}

func TestParseBranch_FourSegments(t *testing.T) {
	t.Parallel()
	scope, id := parseScopePath("memory/user/john/extra")
	if scope != "user" {
		t.Errorf("scope: expected 'user', got %q", scope)
	}
	if id != "john/extra" {
		t.Errorf("id: expected 'john/extra', got %q", id)
	}
}

// MemorySyncModule — constructor

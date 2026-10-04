package sessioncontext

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// markSessionBootstrapDelivered is the check-and-mark Render performs at a turn boundary.
func markSessionBootstrapDelivered(projectDir string, input []byte) bool {
	marker := sessionBootstrapMarker(projectDir, input)
	if markerExists(marker) {
		return true
	}
	if marker != "" {
		writeMarker(marker)
	}
	return false
}

func TestHookInputNeedsMandatoryOnlyOnFirstInvocation(t *testing.T) {
	t.Parallel()

	if !NeedsMandatory("first-invocation", []byte(`{"invocationNum":0}`)) {
		t.Fatal("first invocation must load mandatory memory")
	}
	if NeedsMandatory("first-invocation", []byte(`{"invocationNum":1}`)) {
		t.Fatal("later invocation must not reopen mandatory memory")
	}
	if !NeedsMandatory("cursor-subagent-task", nil) {
		t.Fatal("Cursor subagent task injection must carry authoritative mandatory memory")
	}
}

// The turn boundary carries the bootstrap on hosts whose session-start output
// never reaches the model. Marking it per session is what stops a long
// conversation from re-paying the whole protocol on every prompt.
func TestSessionBootstrapIsMarkedOncePerSession(t *testing.T) {
	t.Parallel()

	projectDir := t.TempDir()
	payload := []byte(`{"session_id":"abc-123"}`)

	if markSessionBootstrapDelivered(projectDir, payload) {
		t.Fatal("an unseen session must receive the bootstrap")
	}
	if !markSessionBootstrapDelivered(projectDir, payload) {
		t.Fatal("a session already bootstrapped must not receive it again")
	}
	if markSessionBootstrapDelivered(projectDir, []byte(`{"session_id":"def-456"}`)) {
		t.Fatal("a different session must not inherit another session's marker")
	}
}

// Every failure path has to answer "not delivered": repeating the bootstrap
// only wastes tokens, while wrongly skipping it strands the agent with no
// routing at all.
func TestSessionBootstrapFallsBackToDeliveringWhenItCannotBeMarked(t *testing.T) {
	t.Parallel()

	projectDir := t.TempDir()
	for name, payload := range map[string][]byte{
		"missing session id": []byte(`{}`),
		"empty session id":   []byte(`{"session_id":"   "}`),
		"unparsable payload": []byte(`not json`),
		"absent payload":     nil,
	} {
		if markSessionBootstrapDelivered(projectDir, payload) {
			t.Fatalf("%s must still deliver the bootstrap", name)
		}
	}
	if markSessionBootstrapDelivered("", []byte(`{"session_id":"abc-123"}`)) {
		t.Fatal("an unresolved project must still deliver the bootstrap")
	}

	// A plain file where the marker directory belongs fails MkdirAll for any
	// user, unlike a read-only directory, which root would walk straight through.
	blocked := t.TempDir()
	markerDir := brand.ProjectRuntimePath(blocked, "cache", "session-bootstrap")
	if err := os.MkdirAll(filepath.Dir(markerDir), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(markerDir, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if markSessionBootstrapDelivered(blocked, []byte(`{"session_id":"abc-123"}`)) {
		t.Fatal("an unwritable marker location must still deliver the bootstrap")
	}
}

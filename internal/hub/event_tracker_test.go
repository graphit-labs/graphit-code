package hub

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
)

func TestGenerateULID(t *testing.T) {
	t.Parallel()
	id := generateULID()
	if id == "" {
		t.Error("expected non-empty ULID")
	}
	if len(id) != 26 {
		t.Errorf("ULID length = %d, want 26", len(id))
	}

	id2 := generateULID()
	if id == id2 {
		t.Error("expected different ULIDs")
	}
}

func TestEventTrackerTrackEvent_NilHandling(t *testing.T) {
	t.Parallel()

	var tracker *EventTracker
	tracker.TrackEvent(context.Background(), "test", "", nil, nil)

	tracker2 := &EventTracker{store: nil}
	tracker2.TrackEvent(context.Background(), "test", "", nil, nil)
}

func TestAddEventIdentitiesUsesExplicitIDsByDefault(t *testing.T) {
	payload := map[string]any{
		"project_hash": "injected-project-hash",
		"user_hash":    "injected-user-hash",
	}
	addEventIdentities(payload, "project-1", "alice", false, "")

	if payload["project_id"] != "project-1" || payload["user_id"] != "alice" {
		t.Fatalf("explicit event identities = %#v", payload)
	}
	if _, ok := payload["project_hash"]; ok {
		t.Fatalf("explicit event contained project_hash: %#v", payload)
	}
	if _, ok := payload["user_hash"]; ok {
		t.Fatalf("explicit event contained user_hash: %#v", payload)
	}
}

func TestAddEventIdentitiesUsesOnlySaltedHashesWhenEnabled(t *testing.T) {
	const secret = "known-client-secret"
	payload := map[string]any{
		"project_id":   "injected-project",
		"user_id":      "injected-user",
		"project_hash": "injected-project-hash",
		"user_hash":    "injected-user-hash",
	}
	addEventIdentities(payload, "project-1", "alice", true, secret)

	wantProject := sha256.Sum256([]byte("project-1" + secret))
	wantUser := sha256.Sum256([]byte("alice" + secret))
	if payload["project_hash"] != hex.EncodeToString(wantProject[:]) {
		t.Fatalf("project_hash = %q", payload["project_hash"])
	}
	if payload["user_hash"] != hex.EncodeToString(wantUser[:]) {
		t.Fatalf("user_hash = %q", payload["user_hash"])
	}
	if _, ok := payload["project_id"]; ok {
		t.Fatalf("anonymized event contained project_id: %#v", payload)
	}
	if _, ok := payload["user_id"]; ok {
		t.Fatalf("anonymized event contained user_id: %#v", payload)
	}
}

func TestComputeEventIdentityHashRejectsMissingInputs(t *testing.T) {
	if got := computeEventIdentityHash("", "secret"); got != "" {
		t.Fatalf("empty identity hash = %q", got)
	}
	if got := computeEventIdentityHash("identity", ""); got != "" {
		t.Fatalf("empty secret hash = %q", got)
	}
}

func TestEventClientSecretRejectsPersistenceFailure(t *testing.T) {
	badDir := filepath.Join(t.TempDir(), "not-a-directory")
	if err := os.WriteFile(badDir, []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), badDir)
	t.Setenv(config.ConfigEnvVar(config.ClientSecretConfigKey), "")

	if secret, ok := eventClientSecret(true); ok || secret != "" {
		t.Fatalf("eventClientSecret = %q, %t; anonymized event would be published without a secret", secret, ok)
	}
}

func TestGetOrCreateClientID(t *testing.T) {
	t.Parallel()
	id := getOrCreateClientID()
	if id == "" {
		t.Error("expected non-empty ID")
	}
}

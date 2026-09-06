package hub

import (
	"context"
	"testing"
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

func TestComputeProjectHash(t *testing.T) {
	t.Parallel()
	h := computeProjectHash("test-project")
	if h == "" {
		t.Error("expected non-empty hash")
	}
	h2 := computeProjectHash("test-project")
	if h != h2 {
		t.Errorf("expected same hash, got %q and %q", h, h2)
	}
	h3 := computeProjectHash("other-project")
	if h == h3 {
		t.Error("expected different hash for different input")
	}
}

func TestGetOrCreateClientSecret(t *testing.T) {
	t.Parallel()
	secret := getOrCreateClientSecret()
	if secret == "" {
		t.Error("expected non-empty secret")
	}
}

func TestGetOrCreateClientID(t *testing.T) {
	t.Parallel()
	id := getOrCreateClientID()
	if id == "" {
		t.Error("expected non-empty ID")
	}
}

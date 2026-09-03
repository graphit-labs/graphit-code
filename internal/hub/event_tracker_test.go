package hub

import (
	"testing"
)

func TestBuildEventKey(t *testing.T) {
	t.Parallel()

	t.Run("with artifact type", func(t *testing.T) {
		t.Parallel()
		key := buildEventKey("hub.install", map[string]string{"type": "rule"})
		if key == "" {
			t.Error("expected non-empty key")
		}
		if len(key) < 10 {
			t.Errorf("key too short: %q", key)
		}
	})

	t.Run("artifact with empty type", func(t *testing.T) {
		t.Parallel()
		key := buildEventKey("hub.install", map[string]string{})
		if key == "" {
			t.Error("expected non-empty key")
		}
	})

	t.Run("no artifact", func(t *testing.T) {
		t.Parallel()
		key := buildEventKey("project.init", nil)
		if key == "" {
			t.Error("expected non-empty key")
		}
	})

	t.Run("action without dot", func(t *testing.T) {
		t.Parallel()
		key := buildEventKey("install", map[string]string{"type": "rule"})
		if key == "" {
			t.Error("expected non-empty key")
		}
	})
}

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
	tracker.TrackEvent("test", "", nil, nil)

	tracker2 := &EventTracker{store: nil}
	tracker2.TrackEvent("test", "", nil, nil)
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

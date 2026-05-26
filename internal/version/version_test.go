package version

import "testing"

func TestVersion(t *testing.T) {
	if Version == "" {
		t.Error("expected version to be non-empty")
	}
}

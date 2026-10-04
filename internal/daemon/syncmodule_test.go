package daemon

import "testing"

func TestSyncModule_ImplementsActivityReporter(t *testing.T) {
	var _ ActivityReporter = (*SyncModule)(nil)

	m := NewSyncModule(t.TempDir(), "")
	called := false
	m.SetActivityCallback(func() { called = true })

	if m.onActivity == nil {
		t.Fatal("expected onActivity to be set")
	}
	m.onActivity()
	if !called {
		t.Error("expected the wired callback to run")
	}
}

package sysutil

import "testing"

// TestLowerPriorityBestEffort ensures LowerPriority runs without panicking. It
// is best-effort, so a non-nil error (e.g. EPERM in a container) is acceptable;
// the test only guards that the call is safe to make at startup.
func TestLowerPriorityBestEffort(t *testing.T) {
	if err := LowerPriority(); err != nil {
		t.Logf("LowerPriority returned (non-fatal): %v", err)
	}
}

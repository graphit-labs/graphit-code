package task

import (
	"testing"
	"time"
)

func TestActivityRequiresCurrentOwnership(t *testing.T) {
	now := time.Date(2026, 9, 21, 12, 0, 0, 0, time.UTC)
	v := ActivityRecord{Owner: "unit:worker", Status: "in_progress", LeaseExpiresAt: now.Add(time.Minute).Format(time.RFC3339Nano)}
	if !activeActivity(v, now) {
		t.Fatal("valid work excluded")
	}
	for _, bad := range []ActivityRecord{{Owner: v.Owner, Status: "open", LeaseExpiresAt: v.LeaseExpiresAt}, {Status: v.Status, LeaseExpiresAt: v.LeaseExpiresAt}, {Owner: v.Owner, Status: v.Status, LeaseExpiresAt: now.Format(time.RFC3339Nano)}, {Owner: v.Owner, Status: v.Status, LeaseExpiresAt: "invalid"}} {
		if activeActivity(bad, now) {
			t.Fatalf("inactive work included: %+v", bad)
		}
	}
}

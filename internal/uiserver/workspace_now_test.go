package uiserver

import (
	"fmt"
	"testing"
	"time"
)

func TestActivityOrderingBeforeLimitAndLegacyDates(t *testing.T) {
	items := []nowItem{}
	for i := 0; i < 25; i++ {
		items = append(items, nowItem{ID: fmt.Sprint(i), UpdatedAt: time.Date(2026, 9, 21, i, 0, 0, 0, time.UTC).Format(time.RFC3339Nano)})
	}
	got := finishNow(items, nil)
	if got.Total != 25 || !got.HasMore || len(got.Items) != 20 || got.Items[0].ID != "24" {
		t.Fatalf("%+v", got)
	}
	if activityDate("2026-09-21").IsZero() {
		t.Fatal("legacy date ignored")
	}
	if got := finishNow(nil, fmt.Errorf("offline")); len(got.Items) != 0 || got.Error != "offline" {
		t.Fatal(got)
	}
}

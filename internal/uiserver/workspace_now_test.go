package uiserver

import (
	"fmt"
	"net/http"
	"net/http/httptest"
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

func TestWorkspaceNowResponsesAreNeverCached(t *testing.T) {
	req := httptest.NewRequest(http.MethodGet, "/api/workspace/now", nil)
	recorder := httptest.NewRecorder()
	(&TaskHandler{}).handleWorkspaceNow(recorder, req)
	if got := recorder.Header().Get("Cache-Control"); got != "no-store" {
		t.Fatalf("Cache-Control = %q, want no-store", got)
	}
}

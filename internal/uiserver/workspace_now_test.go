package uiserver

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/task"
)

type fakeWorkspaceActivityReader struct {
	tasks, sessions     []task.ActivityRecord
	taskErr, sessionErr error
}

func (f fakeWorkspaceActivityReader) CurrentActivitySections(context.Context, time.Time) ([]task.ActivityRecord, []task.ActivityRecord, error, error) {
	return f.tasks, f.sessions, f.taskErr, f.sessionErr
}

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

func TestWorkspaceActivitySectionsIsolateSessionProgressFailure(t *testing.T) {
	sections := workspaceActivitySections(context.Background(), fakeWorkspaceActivityReader{
		tasks:      []task.ActivityRecord{{ID: "tsk-live", Title: "Live task"}},
		sessions:   []task.ActivityRecord{{ID: "ses-live", Title: "Live session"}},
		sessionErr: errors.New("progress unavailable"),
	}, time.Now())
	if got := sections["tasks"]; got.Error != "" || len(got.Items) != 1 || got.Items[0].ID != "tsk-live" {
		t.Fatalf("task section = %+v", got)
	}
	if got := sections["sessions"]; got.Error != "progress unavailable" {
		t.Fatalf("session section = %+v", got)
	}
}

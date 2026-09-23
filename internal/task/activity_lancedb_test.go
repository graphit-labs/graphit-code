//go:build lancedb

package task

import (
	"context"
	"testing"
	"time"
)

func TestCurrentActivityRefreshesAfterAnotherServiceWrites(t *testing.T) {
	ctx := context.Background()
	uri := t.TempDir()
	reader := OpenAt("activity-refresh", uri)
	tasks, sessions, err := reader.CurrentActivity(ctx, time.Now())
	if err != nil || len(tasks) != 0 || len(sessions) != 0 {
		t.Fatalf("initial activity: tasks=%#v sessions=%#v err=%v", tasks, sessions, err)
	}

	writer := OpenAt("activity-refresh", uri)
	session := claimSessionFixture(t, writer, createSessionFixture(t, writer, "coordinator", "live-refresh"), "coordinator")
	in := testCreate("New active task", "live-refresh-task")
	in.Actor = "coordinator"
	in.RequireSession = true
	task, err := writer.Create(ctx, in)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = writer.Claim(ctx, task.ID, "worker", time.Hour); err != nil {
		t.Fatal(err)
	}

	tasks, sessions, err = reader.CurrentActivity(ctx, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	if len(tasks) != 1 || tasks[0].ID != task.ID {
		t.Fatalf("fresh tasks = %#v, want %s", tasks, task.ID)
	}
	if len(sessions) != 1 || sessions[0].ID != session.ID {
		t.Fatalf("fresh sessions = %#v, want %s", sessions, session.ID)
	}
	if sessions[0].CompletedTasks != 0 || sessions[0].TotalTasks != 1 {
		t.Fatalf("fresh session progress = %#v", sessions[0])
	}
}

func TestCurrentActivitySummarizesSessionTaskProgress(t *testing.T) {
	ctx := context.Background()
	s := OpenAt("activity-progress", t.TempDir())
	zero := claimSessionFixture(t, s, createSessionFixture(t, s, "coordinator-zero", "activity-zero"), "coordinator-zero")
	partial := claimSessionFixture(t, s, createSessionFixture(t, s, "coordinator-partial", "activity-partial"), "coordinator-partial")
	complete := claimSessionFixture(t, s, createSessionFixture(t, s, "coordinator-complete", "activity-complete"), "coordinator-complete")

	createTask := func(sessionID, key string) Task {
		t.Helper()
		in := testCreate("Deliver "+key, key)
		in.Actor = "coordinator"
		in.RequireSession = true
		in.SessionID = sessionID
		created, err := s.Create(ctx, in)
		if err != nil {
			t.Fatal(err)
		}
		return created
	}

	partialDone := createTask(partial.ID, "activity-partial-done")
	createTask(partial.ID, "activity-partial-open")
	partialCancelled := createTask(partial.ID, "activity-partial-cancelled")
	finishTaskFixture(t, s, partialDone.ID, "worker-partial")
	if _, err := s.Cancel(ctx, partialCancelled.ID, "", "coordinator", "Cancelled fixture remains in the total."); err != nil {
		t.Fatal(err)
	}
	completeA := createTask(complete.ID, "activity-complete-a")
	completeB := createTask(complete.ID, "activity-complete-b")
	finishTaskFixture(t, s, completeA.ID, "worker-complete-a")
	finishTaskFixture(t, s, completeB.ID, "worker-complete-b")

	_, sessions, err := s.CurrentActivity(ctx, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	byID := make(map[string]ActivityRecord, len(sessions))
	for _, session := range sessions {
		byID[session.ID] = session
	}
	for _, tc := range []struct {
		id                         string
		completedTasks, totalTasks int
	}{
		{id: zero.ID, completedTasks: 0, totalTasks: 0},
		{id: partial.ID, completedTasks: 1, totalTasks: 3},
		{id: complete.ID, completedTasks: 2, totalTasks: 2},
	} {
		got, ok := byID[tc.id]
		if !ok || got.CompletedTasks != tc.completedTasks || got.TotalTasks != tc.totalTasks {
			t.Fatalf("session %s progress = %#v", tc.id, got)
		}
	}
	if err := s.withTables(ctx, func(tables *tables) error {
		statuses, queryErr := tables.sessionTaskStatuses(ctx, partial.ID)
		if queryErr != nil {
			return queryErr
		}
		if len(statuses) != 3 {
			t.Fatalf("filtered statuses = %#v", statuses)
		}
		for _, status := range statuses {
			if status.sessionID != partial.ID {
				t.Fatalf("unexpected session in filtered statuses: %#v", status)
			}
		}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
}

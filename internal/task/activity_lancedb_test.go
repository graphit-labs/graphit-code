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
}

package task

import (
	"context"
	"errors"
	"time"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// ActivityRecord deliberately excludes fencing tokens, descriptions and audit history.
type ActivityRecord struct {
	ID             string `json:"id"`
	Title          string `json:"title"`
	Owner          string `json:"owner"`
	Status         string `json:"status"`
	UpdatedAt      string `json:"updated_at"`
	LeaseExpiresAt string `json:"lease_expires_at"`
	Progress       string `json:"progress,omitempty"`
	NextStep       string `json:"next_step,omitempty"`
	CompletedTasks int    `json:"completed_tasks"`
	TotalTasks     int    `json:"total_tasks"`
}

func activeActivity(v ActivityRecord, now time.Time) bool {
	lease, err := time.Parse(time.RFC3339Nano, v.LeaseExpiresAt)
	return err == nil && v.Status == "in_progress" && v.Owner != "" && lease.After(now)
}

// CurrentActivitySections reads current in-progress records under the usual project
// authorization while preserving independent Task and Session source errors. Session
// leases currently live in snapshots: decode on the server and return only this
// allowlisted DTO. Never send snapshots to the browser.
func (s *Service) CurrentActivitySections(ctx context.Context, now time.Time) (tasks, sessions []ActivityRecord, taskErr, sessionErr error) {
	tasks, sessions = []ActivityRecord{}, []ActivityRecord{}
	err := s.withTables(ctx, func(t *tables) error {
		read := func(isSession bool) ([]ActivityRecord, error) {
			items := []ActivityRecord{}
			table := t.tasks
			columns := []string{"id", "title", "owner", "status", "updated_at", "lease_expires_at", "progress_summary", "next_step"}
			if isSession {
				table = t.sessions
				columns = []string{"snapshot_json"}
			}
			for offset := 0; ; offset += pageSize {
				hits, e := table.Search(ctx, lancestore.Query{Filter: "status = 'in_progress'", Columns: columns, Limit: pageSize, Offset: offset})
				if e != nil {
					return nil, e
				}
				for _, h := range hits {
					r := h.Row
					v := ActivityRecord{ID: text(r, "id"), Title: text(r, "title"), Owner: text(r, "owner"), Status: text(r, "status"), UpdatedAt: text(r, "updated_at"), LeaseExpiresAt: text(r, "lease_expires_at"), Progress: text(r, "progress_summary"), NextStep: text(r, "next_step")}
					if isSession {
						x, e := sessionFromRow(r)
						if e != nil {
							return nil, e
						}
						v = ActivityRecord{
							ID: x.ID, Title: x.Title, Owner: x.Owner, Status: string(x.Status),
							UpdatedAt: x.UpdatedAt, LeaseExpiresAt: x.LeaseExpiresAt,
							Progress: x.ProgressSummary, NextStep: x.NextStep,
						}
					}
					if activeActivity(v, now) {
						items = append(items, v)
					}
				}
				if len(hits) < pageSize {
					break
				}
			}
			return items, nil
		}
		tasks, taskErr = read(false)
		sessions, sessionErr = read(true)
		if sessionErr != nil || len(sessions) == 0 {
			return nil
		}
		sessionIDs := make([]string, 0, len(sessions))
		for _, session := range sessions {
			sessionIDs = append(sessionIDs, session.ID)
		}
		taskStatuses, e := t.sessionTaskStatuses(ctx, sessionIDs...)
		if e != nil {
			sessionErr = e
			return nil
		}
		progressBySession := sessionTaskProgressBySession(taskStatuses)
		for i := range sessions {
			progress := progressBySession[sessions[i].ID]
			sessions[i].CompletedTasks = progress.completed
			sessions[i].TotalTasks = progress.total
		}
		return nil
	})
	if err != nil {
		return []ActivityRecord{}, []ActivityRecord{}, err, err
	}
	return tasks, sessions, taskErr, sessionErr
}

// CurrentActivity preserves the original combined-error contract for non-UI callers.
func (s *Service) CurrentActivity(ctx context.Context, now time.Time) (tasks, sessions []ActivityRecord, err error) {
	tasks, sessions, taskErr, sessionErr := s.CurrentActivitySections(ctx, now)
	return tasks, sessions, errors.Join(taskErr, sessionErr)
}

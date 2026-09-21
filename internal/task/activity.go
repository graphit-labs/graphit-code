package task

import (
	"context"
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
}

func activeActivity(v ActivityRecord, now time.Time) bool {
	lease, err := time.Parse(time.RFC3339Nano, v.LeaseExpiresAt)
	return err == nil && v.Status == "in_progress" && v.Owner != "" && lease.After(now)
}

// CurrentActivity reads only current in-progress records under the usual project
// authorization. Session leases currently live in snapshots: decode on the server
// and return only this allowlisted DTO. Never send snapshots to the browser.
func (s *Service) CurrentActivity(ctx context.Context, now time.Time) (tasks, sessions []ActivityRecord, err error) {
	tasks, sessions = []ActivityRecord{}, []ActivityRecord{}
	err = s.withTables(ctx, func(t *tables) error {
		for _, isSession := range []bool{false, true} {
			table := t.tasks
			columns := []string{"id", "title", "owner", "status", "updated_at", "lease_expires_at", "progress_summary", "next_step"}
			if isSession {
				table = t.sessions
				columns = []string{"snapshot_json"}
			}
			for offset := 0; ; offset += pageSize {
				hits, e := table.Search(ctx, lancestore.Query{Filter: "status = 'in_progress'", Columns: columns, Limit: pageSize, Offset: offset})
				if e != nil {
					return e
				}
				for _, h := range hits {
					r := h.Row
					v := ActivityRecord{ID: text(r, "id"), Title: text(r, "title"), Owner: text(r, "owner"), Status: text(r, "status"), UpdatedAt: text(r, "updated_at"), LeaseExpiresAt: text(r, "lease_expires_at"), Progress: text(r, "progress_summary"), NextStep: text(r, "next_step")}
					if isSession {
						x, e := sessionFromRow(r)
						if e != nil {
							return e
						}
						v = ActivityRecord{x.ID, x.Title, x.Owner, string(x.Status), x.UpdatedAt, x.LeaseExpiresAt, x.ProgressSummary, x.NextStep}
					}
					if activeActivity(v, now) {
						if isSession {
							sessions = append(sessions, v)
						} else {
							tasks = append(tasks, v)
						}
					}
				}
				if len(hits) < pageSize {
					break
				}
			}
		}
		return nil
	})
	return
}

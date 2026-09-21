package task

import (
	"context"
	"encoding/json"
	"fmt"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

const (
	sessionsTableName           = "task_sessions"
	sessionEventsTableName      = "task_session_events"
	sessionCheckpointsTableName = "task_session_checkpoints"
	sessionRevisionsTableName   = "task_session_revisions"
)

func sessionSchema() lancestore.Schema {
	fields := []lancestore.Field{}
	for _, name := range []string{"id", "project_id", "idempotency_key", "title", "description", "strategy", "status", "owner", "updated_at", "snapshot_json", "search_text"} {
		fields = append(fields, lancestore.Field{Name: name, Type: lancestore.FieldString})
	}
	fields = append(fields, lancestore.Field{Name: "revision", Type: lancestore.FieldInt64})
	return lancestore.Schema{Fields: fields}
}
func sessionHistorySchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "key", Type: lancestore.FieldString}, {Name: "session_id", Type: lancestore.FieldString},
		{Name: "revision", Type: lancestore.FieldInt64}, {Name: "body_json", Type: lancestore.FieldString},
		{Name: "search_text", Type: lancestore.FieldString},
	}}
}
func sessionRow(v Session) lancestore.Row {
	// LastEvent is private in public responses, but must survive the snapshot CAS.
	snapshot, _ := json.Marshal(struct {
		Session   Session
		LastEvent SessionEvent
	}{v, v.LastEvent})
	return lancestore.Row{"id": v.ID, "project_id": v.ProjectID, "idempotency_key": v.IdempotencyKey,
		"title": v.Title, "description": v.Description, "strategy": v.Strategy, "status": string(v.Status), "owner": v.Owner,
		"updated_at": v.UpdatedAt, "revision": v.Revision, "snapshot_json": string(snapshot),
		"search_text": strings.Join([]string{v.Title, v.Description, v.Strategy, v.ProgressSummary, v.NextStep}, "\n")}
}
func sessionFromRow(r lancestore.Row) (Session, error) {
	var snapshot struct {
		Session   Session
		LastEvent SessionEvent
	}
	if err := json.Unmarshal([]byte(text(r, "snapshot_json")), &snapshot); err != nil {
		return Session{}, fmt.Errorf("decoding session snapshot: %w", err)
	}
	snapshot.Session.LastEvent = snapshot.LastEvent
	return snapshot.Session, nil
}
func (t *tables) getSession(ctx context.Context, id string) (Session, bool, error) {
	hits, err := t.sessions.Search(ctx, lancestore.Query{Filter: "id = " + quote(id), Limit: 1})
	if err != nil || len(hits) == 0 {
		return Session{}, false, err
	}
	v, err := sessionFromRow(hits[0].Row)
	return v, err == nil, err
}
func (t *tables) allSessions(ctx context.Context) ([]Session, error) {
	rows, err := t.allProjectionRows(ctx, t.sessions)
	if err != nil {
		return nil, err
	}
	out := make([]Session, 0, len(rows))
	for _, row := range rows {
		v, err := sessionFromRow(row)
		if err != nil {
			return nil, err
		}
		out = append(out, v)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out, nil
}
func (t *tables) sessionHistory(ctx context.Context, table *lancestore.Table, id string) ([]lancestore.Row, error) {
	out := []lancestore.Row{}
	for offset := 0; ; offset += pageSize {
		hits, err := table.Search(ctx, lancestore.Query{Filter: "session_id = " + quote(id), Limit: pageSize, Offset: offset})
		if err != nil {
			return nil, err
		}
		for _, h := range hits {
			out = append(out, h.Row)
		}
		if len(hits) < pageSize {
			break
		}
	}
	sort.Slice(out, func(i, j int) bool { return number(out[i], "revision") < number(out[j], "revision") })
	return out, nil
}
func sessionHistoryRow(key, id string, revision int64, value any, searchText string) lancestore.Row {
	body, _ := json.Marshal(value)
	return lancestore.Row{"key": key, "session_id": id, "revision": revision, "body_json": string(body), "search_text": searchText}
}
func (s *Service) projectSession(ctx context.Context, t *tables, v Session) error {
	source := relations.Entity{Type: "session", ID: v.ID, Scope: "project", ScopeID: v.ProjectID}
	refs := []relations.Ref{}
	if v.References != nil {
		refs = relations.Qualify(source, *v.References)
	}
	if err := relations.Replace(ctx, t.store, source, v.Revision, "record", refs, fmt.Sprint(v.Revision)); err != nil {
		return err
	}
	e := v.LastEvent
	if e.Key == "" {
		return nil
	}
	entries := []struct {
		table *lancestore.Table
		row   lancestore.Row
	}{{t.sessionEvents, sessionHistoryRow(e.Key, v.ID, e.Revision, e, e.Summary+"\n"+e.NextStep)}}
	if c := e.Checkpoint; c != nil {
		entries = append(entries, struct {
			table *lancestore.Table
			row   lancestore.Row
		}{t.sessionCheckpoints, sessionHistoryRow(c.Key, v.ID, c.Revision, c, strings.Join([]string{c.Summary, c.Problems, c.Decisions, c.Strategy, c.NextStep}, "\n"))})
	}
	if r := e.SpecRevision; r != nil {
		entries = append(entries, struct {
			table *lancestore.Table
			row   lancestore.Row
		}{t.sessionRevisions, sessionHistoryRow(r.Key, v.ID, r.SourceRevision, r, strings.Join([]string{r.Reason, r.Before.Title, r.Before.Description, r.Before.Strategy, r.After.Title, r.After.Description, r.After.Strategy}, "\n"))})
	}
	for _, entry := range entries {
		if _, err := entry.table.Merge(ctx, lancestore.MergeOptions{KeyColumn: "key", MatchCondition: "false", InsertIfMissing: true}, []lancestore.Row{entry.row}); err != nil {
			return err
		}
	}
	return nil
}
func (s *Service) putSessionCAS(ctx context.Context, t *tables, before Session, after *Session) error {
	if refs := relations.Inputs(ctx); refs != nil {
		if err := relations.Validate(refs); err != nil {
			return err
		}
		after.References = refs
	} else {
		after.References = before.References
	}
	// Repair the previous committed event before replacing its recovery anchor.
	if before.ID != "" {
		if err := s.projectSession(ctx, t, before); err != nil {
			return err
		}
	}
	condition := "false"
	if before.ID != "" {
		condition = fmt.Sprintf("target.revision = %d", before.Revision)
	}
	res, err := t.sessions.Merge(ctx, lancestore.MergeOptions{KeyColumn: "id", MatchCondition: condition, InsertIfMissing: before.ID == ""}, []lancestore.Row{sessionRow(*after)})
	if err != nil {
		return err
	}
	if !res.Changed() {
		return fmt.Errorf("%w: session %s", ErrConcurrent, after.ID)
	}
	return s.projectSession(ctx, t, *after)
}

package task

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

const (
	tasksTableName         = "tasks"
	dependenciesTableName  = "task_dependencies"
	eventsTableName        = "task_events"
	checksTableName        = "task_checks"
	commentsTableName      = "task_comments"
	specRevisionsTableName = "task_spec_revisions"
	controlTableName       = "task_control"
	pageSize               = 512
)

type tables struct {
	store         *lancestore.Store
	tasks         *lancestore.Table
	dependencies  *lancestore.Table
	events        *lancestore.Table
	checks        *lancestore.Table
	comments      *lancestore.Table
	specRevisions *lancestore.Table
	control       *lancestore.Table
}

type maintenanceResult struct {
	fragmentsRemoved int64
	oldVersions      int64
	bytesRemoved     int64
}

func openTables(ctx context.Context, uri string, s3 config.S3Config) (*tables, error) {
	st, err := lancestore.Open(ctx, lancestore.Config{
		URI: uri, S3: s3, Writable: true, StrongReadConsistency: true,
	})
	if err != nil {
		return nil, fmt.Errorf("opening task store at %s: %w", uri, err)
	}
	closeOnError := func(err error) (*tables, error) { _ = st.Close(); return nil, err }
	open := func(name string, schema lancestore.Schema) (*lancestore.Table, error) {
		tbl, openErr := st.EnsureTable(ctx, name, schema)
		if openErr != nil {
			return nil, openErr
		}
		if !tbl.Schema().Equal(schema) {
			return nil, fmt.Errorf("task table %s has an incompatible schema; migrate it with a current Graphit binary", name)
		}
		return tbl, nil
	}
	t := &tables{store: st}
	if t.tasks, err = open(tasksTableName, taskSchema()); err != nil {
		return closeOnError(err)
	}
	if t.dependencies, err = open(dependenciesTableName, dependencySchema()); err != nil {
		return closeOnError(err)
	}
	if t.events, err = open(eventsTableName, eventSchema()); err != nil {
		return closeOnError(err)
	}
	if t.checks, err = open(checksTableName, checkSchema()); err != nil {
		return closeOnError(err)
	}
	if t.comments, err = open(commentsTableName, commentSchema()); err != nil {
		return closeOnError(err)
	}
	if t.specRevisions, err = open(specRevisionsTableName, specRevisionSchema()); err != nil {
		return closeOnError(err)
	}
	if t.control, err = open(controlTableName, controlSchema()); err != nil {
		return closeOnError(err)
	}
	return t, nil
}

func (t *tables) ensureIndexes(ctx context.Context) error {
	if err := t.tasks.EnsureIndexes(ctx,
		lancestore.Index{Column: "id", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "parent_id", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "status", Kind: lancestore.IndexScalarBitmap},
		lancestore.Index{Column: "owner", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "search_text", Kind: lancestore.IndexInvertedText},
	); err != nil {
		return fmt.Errorf("indexing tasks: %w", err)
	}
	if err := t.dependencies.EnsureIndexes(ctx,
		lancestore.Index{Column: "key", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "task_id", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "depends_on", Kind: lancestore.IndexScalarBTree},
	); err != nil {
		return fmt.Errorf("indexing task dependencies: %w", err)
	}
	if err := t.events.EnsureIndexes(ctx,
		lancestore.Index{Column: "key", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "task_id", Kind: lancestore.IndexScalarBTree},
	); err != nil {
		return fmt.Errorf("indexing task events: %w", err)
	}
	if err := t.checks.EnsureIndexes(ctx,
		lancestore.Index{Column: "key", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "task_id", Kind: lancestore.IndexScalarBTree},
	); err != nil {
		return fmt.Errorf("indexing task checks: %w", err)
	}
	if err := t.comments.EnsureIndexes(ctx,
		lancestore.Index{Column: "id", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "task_id", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "body", Kind: lancestore.IndexInvertedText},
	); err != nil {
		return fmt.Errorf("indexing task comments: %w", err)
	}
	if err := t.specRevisions.EnsureIndexes(ctx,
		lancestore.Index{Column: "key", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "task_id", Kind: lancestore.IndexScalarBTree},
		lancestore.Index{Column: "subject_id", Kind: lancestore.IndexScalarBTree},
	); err != nil {
		return fmt.Errorf("indexing task specification revisions: %w", err)
	}
	return nil
}

func (t *tables) close() error {
	if t == nil || t.store == nil {
		return nil
	}
	return t.store.Close()
}

func (t *tables) refresh(ctx context.Context) error {
	for _, table := range []*lancestore.Table{t.tasks, t.dependencies, t.events, t.checks, t.comments, t.specRevisions, t.control} {
		if err := table.Refresh(ctx); err != nil {
			return err
		}
	}
	return nil
}

func (t *tables) maintain(ctx context.Context, retention time.Duration) (maintenanceResult, error) {
	var result maintenanceResult
	indexed := []struct {
		name  string
		table *lancestore.Table
	}{
		{tasksTableName, t.tasks},
		{dependenciesTableName, t.dependencies},
		{eventsTableName, t.events},
		{checksTableName, t.checks},
		{commentsTableName, t.comments},
		{specRevisionsTableName, t.specRevisions},
	}
	for _, entry := range indexed {
		if err := entry.table.FoldNewRowsIntoIndexes(ctx); err != nil {
			return result, fmt.Errorf("maintaining task table %s indexes: %w", entry.name, err)
		}
		compacted, err := entry.table.Compact(ctx)
		if err != nil {
			return result, fmt.Errorf("compacting task table %s: %w", entry.name, err)
		}
		result.fragmentsRemoved += compacted.FragmentsRemoved
		pruned, err := entry.table.PruneVersions(ctx, retention)
		if err != nil {
			return result, fmt.Errorf("pruning task table %s: %w", entry.name, err)
		}
		result.oldVersions += pruned.OldVersions
		result.bytesRemoved += pruned.BytesRemoved
	}
	compacted, err := t.control.Compact(ctx)
	if err != nil {
		return result, fmt.Errorf("compacting task table %s: %w", controlTableName, err)
	}
	result.fragmentsRemoved += compacted.FragmentsRemoved
	pruned, err := t.control.PruneVersions(ctx, retention)
	if err != nil {
		return result, fmt.Errorf("pruning task table %s: %w", controlTableName, err)
	}
	result.oldVersions += pruned.OldVersions
	result.bytesRemoved += pruned.BytesRemoved
	return result, nil
}

func taskSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "id", Type: lancestore.FieldString}, {Name: "project_id", Type: lancestore.FieldString},
		{Name: "parent_id", Type: lancestore.FieldString}, {Name: "idempotency_key", Type: lancestore.FieldString},
		{Name: "title", Type: lancestore.FieldString}, {Name: "description", Type: lancestore.FieldString},
		{Name: "type", Type: lancestore.FieldString}, {Name: "status", Type: lancestore.FieldString},
		{Name: "priority", Type: lancestore.FieldInt64}, {Name: "depends_on_json", Type: lancestore.FieldString},
		{Name: "checks_json", Type: lancestore.FieldString},
		{Name: "flagged", Type: lancestore.FieldBool}, {Name: "flag_reason", Type: lancestore.FieldString},
		{Name: "owner", Type: lancestore.FieldString}, {Name: "claim_token", Type: lancestore.FieldString},
		{Name: "claim_epoch", Type: lancestore.FieldInt64}, {Name: "claimed_at", Type: lancestore.FieldString},
		{Name: "lease_expires_at", Type: lancestore.FieldString}, {Name: "heartbeat_at", Type: lancestore.FieldString},
		{Name: "progress_sequence", Type: lancestore.FieldInt64}, {Name: "comment_sequence", Type: lancestore.FieldInt64},
		{Name: "progress_summary", Type: lancestore.FieldString},
		{Name: "next_step", Type: lancestore.FieldString}, {Name: "completed_by", Type: lancestore.FieldString},
		{Name: "completed_at", Type: lancestore.FieldString}, {Name: "created_at", Type: lancestore.FieldString},
		{Name: "updated_at", Type: lancestore.FieldString}, {Name: "revision", Type: lancestore.FieldInt64},
		{Name: "last_event_json", Type: lancestore.FieldString}, {Name: "last_comment_json", Type: lancestore.FieldString},
		{Name: "search_text", Type: lancestore.FieldString},
	}}
}

func dependencySchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "key", Type: lancestore.FieldString}, {Name: "task_id", Type: lancestore.FieldString},
		{Name: "depends_on", Type: lancestore.FieldString}, {Name: "active", Type: lancestore.FieldBool},
		{Name: "created_at", Type: lancestore.FieldString}, {Name: "created_by", Type: lancestore.FieldString},
		{Name: "revision", Type: lancestore.FieldInt64},
	}}
}

func eventSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "key", Type: lancestore.FieldString}, {Name: "task_id", Type: lancestore.FieldString},
		{Name: "sequence", Type: lancestore.FieldInt64}, {Name: "type", Type: lancestore.FieldString},
		{Name: "actor", Type: lancestore.FieldString}, {Name: "at", Type: lancestore.FieldString},
		{Name: "from_status", Type: lancestore.FieldString}, {Name: "to_status", Type: lancestore.FieldString},
		{Name: "summary", Type: lancestore.FieldString}, {Name: "next_step", Type: lancestore.FieldString},
		{Name: "revision", Type: lancestore.FieldInt64},
	}}
}

func checkSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "key", Type: lancestore.FieldString}, {Name: "task_id", Type: lancestore.FieldString},
		{Name: "kind", Type: lancestore.FieldString}, {Name: "text", Type: lancestore.FieldString},
		{Name: "status", Type: lancestore.FieldString}, {Name: "evidence", Type: lancestore.FieldString},
		{Name: "verified_by", Type: lancestore.FieldString}, {Name: "verified_at", Type: lancestore.FieldString},
		{Name: "active", Type: lancestore.FieldBool}, {Name: "revision", Type: lancestore.FieldInt64},
	}}
}

func commentSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "id", Type: lancestore.FieldString}, {Name: "task_id", Type: lancestore.FieldString},
		{Name: "idempotency_key", Type: lancestore.FieldString}, {Name: "sequence", Type: lancestore.FieldInt64},
		{Name: "kind", Type: lancestore.FieldString}, {Name: "body", Type: lancestore.FieldString},
		{Name: "actor", Type: lancestore.FieldString}, {Name: "at", Type: lancestore.FieldString},
		{Name: "revision", Type: lancestore.FieldInt64},
	}}
}

func specRevisionSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "key", Type: lancestore.FieldString}, {Name: "task_id", Type: lancestore.FieldString},
		{Name: "source_revision", Type: lancestore.FieldInt64}, {Name: "kind", Type: lancestore.FieldString},
		{Name: "subject_id", Type: lancestore.FieldString}, {Name: "actor", Type: lancestore.FieldString},
		{Name: "reason", Type: lancestore.FieldString},
		{Name: "at", Type: lancestore.FieldString}, {Name: "before_json", Type: lancestore.FieldString},
		{Name: "after_json", Type: lancestore.FieldString}, {Name: "revision", Type: lancestore.FieldInt64},
	}}
}

func controlSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "key", Type: lancestore.FieldString}, {Name: "token", Type: lancestore.FieldString},
		{Name: "owner", Type: lancestore.FieldString}, {Name: "acquired_at", Type: lancestore.FieldString},
		{Name: "expires_at", Type: lancestore.FieldString}, {Name: "revision", Type: lancestore.FieldInt64},
	}}
}

func taskRow(v Task) lancestore.Row {
	deps, _ := json.Marshal(v.DependsOn)
	checks, _ := json.Marshal(v.Checks)
	event, _ := json.Marshal(v.LastEvent)
	comment, _ := json.Marshal(v.LastComment)
	return lancestore.Row{"id": v.ID, "project_id": v.ProjectID, "parent_id": v.ParentID, "idempotency_key": v.IdempotencyKey,
		"title": v.Title, "description": v.Description,
		"type": v.Type, "status": string(v.Status), "priority": int64(v.Priority), "depends_on_json": string(deps),
		"checks_json": string(checks),
		"flagged":     v.Flagged, "flag_reason": v.FlagReason,
		"owner": v.Owner, "claim_token": v.ClaimToken, "claim_epoch": v.ClaimEpoch, "claimed_at": v.ClaimedAt,
		"lease_expires_at": v.LeaseExpiresAt, "heartbeat_at": v.HeartbeatAt, "progress_sequence": v.ProgressSequence, "comment_sequence": v.CommentSequence,
		"progress_summary": v.ProgressSummary, "next_step": v.NextStep, "completed_by": v.CompletedBy,
		"completed_at": v.CompletedAt, "created_at": v.CreatedAt, "updated_at": v.UpdatedAt, "revision": v.Revision,
		"last_event_json": string(event), "last_comment_json": string(comment), "search_text": taskSearchText(v)}
}

func taskSearchText(v Task) string {
	parts := []string{v.Title, v.Description, v.ProgressSummary, v.NextStep, v.FlagReason, v.Type}
	for _, check := range v.Checks {
		parts = append(parts, check.Text, check.Evidence)
	}
	if v.LastComment.Body != "" {
		parts = append(parts, v.LastComment.Body)
	}
	return strings.Join(parts, "\n")
}

func taskFromRow(r lancestore.Row) Task {
	v := Task{ID: text(r, "id"), ProjectID: text(r, "project_id"), ParentID: text(r, "parent_id"), IdempotencyKey: text(r, "idempotency_key"),
		Title: text(r, "title"), Description: text(r, "description"),
		Type: text(r, "type"), Status: Status(text(r, "status")), Priority: int(number(r, "priority")), Flagged: boolValue(r, "flagged"), FlagReason: text(r, "flag_reason"), Owner: text(r, "owner"),
		ClaimToken: text(r, "claim_token"), ClaimEpoch: number(r, "claim_epoch"), ClaimedAt: text(r, "claimed_at"),
		LeaseExpiresAt: text(r, "lease_expires_at"), HeartbeatAt: text(r, "heartbeat_at"), ProgressSequence: number(r, "progress_sequence"), CommentSequence: number(r, "comment_sequence"),
		ProgressSummary: text(r, "progress_summary"), NextStep: text(r, "next_step"), CompletedBy: text(r, "completed_by"),
		CompletedAt: text(r, "completed_at"), CreatedAt: text(r, "created_at"), UpdatedAt: text(r, "updated_at"), Revision: number(r, "revision")}
	_ = json.Unmarshal([]byte(text(r, "depends_on_json")), &v.DependsOn)
	_ = json.Unmarshal([]byte(text(r, "checks_json")), &v.Checks)
	_ = json.Unmarshal([]byte(text(r, "last_event_json")), &v.LastEvent)
	_ = json.Unmarshal([]byte(text(r, "last_comment_json")), &v.LastComment)
	return v
}

func eventRow(v Event) lancestore.Row {
	return lancestore.Row{"key": v.Key, "task_id": v.TaskID, "sequence": v.Sequence,
		"type": v.Type, "actor": v.Actor, "at": v.At, "from_status": string(v.FromStatus), "to_status": string(v.ToStatus),
		"summary": v.Summary, "next_step": v.NextStep, "revision": v.Revision}
}

func eventFromRow(r lancestore.Row) Event {
	return Event{Key: text(r, "key"), TaskID: text(r, "task_id"), Sequence: number(r, "sequence"),
		Type: text(r, "type"), Actor: text(r, "actor"), At: text(r, "at"), FromStatus: Status(text(r, "from_status")),
		ToStatus: Status(text(r, "to_status")), Summary: text(r, "summary"), NextStep: text(r, "next_step"), Revision: number(r, "revision")}
}

func commentRow(v Comment) lancestore.Row {
	return lancestore.Row{"id": v.ID, "task_id": v.TaskID, "idempotency_key": v.IdempotencyKey,
		"sequence": v.Sequence, "kind": v.Kind, "body": v.Body, "actor": v.Actor, "at": v.At, "revision": v.Revision}
}

func commentFromRow(r lancestore.Row) Comment {
	return Comment{ID: text(r, "id"), TaskID: text(r, "task_id"), IdempotencyKey: text(r, "idempotency_key"),
		Sequence: number(r, "sequence"), Kind: text(r, "kind"), Body: text(r, "body"), Actor: text(r, "actor"),
		At: text(r, "at"), Revision: number(r, "revision")}
}

func specRevisionRow(v SpecRevision) lancestore.Row {
	before, _ := json.Marshal(v.Before)
	after, _ := json.Marshal(v.After)
	return lancestore.Row{"key": v.Key, "task_id": v.TaskID, "source_revision": v.SourceRevision,
		"kind": v.Kind, "subject_id": v.SubjectID, "actor": v.Actor, "reason": v.Reason, "at": v.At,
		"before_json": string(before), "after_json": string(after), "revision": v.SourceRevision}
}

func specRevisionFromRow(r lancestore.Row) SpecRevision {
	v := SpecRevision{Key: text(r, "key"), TaskID: text(r, "task_id"), SourceRevision: number(r, "source_revision"),
		Kind: text(r, "kind"), SubjectID: text(r, "subject_id"), Actor: text(r, "actor"), Reason: text(r, "reason"), At: text(r, "at")}
	_ = json.Unmarshal([]byte(text(r, "before_json")), &v.Before)
	_ = json.Unmarshal([]byte(text(r, "after_json")), &v.After)
	return v
}

func dependencyRecordFromRow(r lancestore.Row) DependencyRecord {
	return DependencyRecord{
		Key: text(r, "key"), TaskID: text(r, "task_id"), DependsOn: text(r, "depends_on"),
		Active: boolValue(r, "active"), CreatedAt: text(r, "created_at"), CreatedBy: text(r, "created_by"),
		Revision: number(r, "revision"),
	}
}

func checkRecordFromRow(r lancestore.Row) CheckRecord {
	return CheckRecord{
		Key: text(r, "key"), TaskID: text(r, "task_id"),
		Check: Check{
			ID: text(r, "key"), Kind: text(r, "kind"), Text: text(r, "text"), Status: text(r, "status"),
			Evidence: text(r, "evidence"), VerifiedBy: text(r, "verified_by"), VerifiedAt: text(r, "verified_at"),
		},
		Active: boolValue(r, "active"), Revision: number(r, "revision"),
	}
}

func (t *tables) getTask(ctx context.Context, id string) (Task, bool, error) {
	hits, err := t.tasks.Search(ctx, lancestore.Query{Filter: "id = " + quote(id), Limit: 1})
	if err != nil {
		return Task{}, false, err
	}
	if len(hits) == 0 {
		return Task{}, false, nil
	}
	return taskFromRow(hits[0].Row), true, nil
}

func (t *tables) getTaskByIdempotencyKey(ctx context.Context, key string) (Task, bool, error) {
	hits, err := t.tasks.Search(ctx, lancestore.Query{Filter: "idempotency_key = " + quote(key), Limit: 2})
	if err != nil {
		return Task{}, false, err
	}
	if len(hits) == 0 {
		return Task{}, false, nil
	}
	if len(hits) > 1 {
		return Task{}, false, fmt.Errorf("task idempotency key %q is duplicated", key)
	}
	return taskFromRow(hits[0].Row), true, nil
}

func (t *tables) allTasks(ctx context.Context) ([]Task, error) {
	return t.tasksWithColumns(ctx, nil)
}

func (t *tables) catalogTasks(ctx context.Context) ([]Task, error) {
	return t.tasksWithColumns(ctx, []string{
		"id", "parent_id", "title", "description", "type", "status", "priority",
		"depends_on_json", "flagged", "flag_reason", "owner", "progress_summary",
		"next_step", "created_at", "updated_at", "revision",
	})
}

func (t *tables) tasksWithColumns(ctx context.Context, columns []string) ([]Task, error) {
	var out []Task
	for offset := 0; ; offset += pageSize {
		hits, err := t.tasks.Search(ctx, lancestore.Query{
			Filter: "revision >= 1", Columns: columns, Limit: pageSize, Offset: offset,
		})
		if err != nil {
			return nil, err
		}
		for _, h := range hits {
			out = append(out, taskFromRow(h.Row))
		}
		if len(hits) < pageSize {
			break
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out, nil
}

func (t *tables) allProjectionRows(ctx context.Context, table *lancestore.Table) ([]lancestore.Row, error) {
	var out []lancestore.Row
	for offset := 0; ; offset += pageSize {
		hits, err := table.Search(ctx, lancestore.Query{Filter: "revision >= 1", Limit: pageSize, Offset: offset})
		if err != nil {
			return nil, err
		}
		for _, hit := range hits {
			out = append(out, hit.Row)
		}
		if len(hits) < pageSize {
			break
		}
	}
	return out, nil
}

func (t *tables) eventsFor(ctx context.Context, id string) ([]Event, error) {
	var out []Event
	for offset := 0; ; offset += pageSize {
		hits, err := t.events.Search(ctx, lancestore.Query{Filter: "task_id = " + quote(id), Limit: pageSize, Offset: offset})
		if err != nil {
			return nil, err
		}
		for _, h := range hits {
			out = append(out, eventFromRow(h.Row))
		}
		if len(hits) < pageSize {
			break
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Sequence < out[j].Sequence })
	return out, nil
}

func (t *tables) commentsFor(ctx context.Context, id string) ([]Comment, error) {
	var out []Comment
	for offset := 0; ; offset += pageSize {
		hits, err := t.comments.Search(ctx, lancestore.Query{Filter: "task_id = " + quote(id), Limit: pageSize, Offset: offset})
		if err != nil {
			return nil, err
		}
		for _, hit := range hits {
			out = append(out, commentFromRow(hit.Row))
		}
		if len(hits) < pageSize {
			break
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Sequence < out[j].Sequence })
	return out, nil
}

func (t *tables) specRevisionsFor(ctx context.Context, id string) ([]SpecRevision, error) {
	var out []SpecRevision
	for offset := 0; ; offset += pageSize {
		hits, err := t.specRevisions.Search(ctx, lancestore.Query{Filter: "task_id = " + quote(id), Limit: pageSize, Offset: offset})
		if err != nil {
			return nil, err
		}
		for _, hit := range hits {
			out = append(out, specRevisionFromRow(hit.Row))
		}
		if len(hits) < pageSize {
			break
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].SourceRevision < out[j].SourceRevision })
	return out, nil
}

func quote(v string) string                  { return "'" + strings.ReplaceAll(v, "'", "''") + "'" }
func text(r lancestore.Row, k string) string { v, _ := r[k].(string); return v }
func number(r lancestore.Row, k string) int64 {
	switch v := r[k].(type) {
	case int64:
		return v
	case int:
		return int64(v)
	case float64:
		return int64(v)
	case string:
		n, _ := strconv.ParseInt(v, 10, 64)
		return n
	}
	return 0
}
func boolValue(r lancestore.Row, k string) bool { v, _ := r[k].(bool); return v }

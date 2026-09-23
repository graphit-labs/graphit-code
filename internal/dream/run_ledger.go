package dream

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
)

const dreamRunsTable = "dream_runs"

// RunRecord is deliberately operational. Model output and memory bodies never
// enter the ledger; the authoritative semantic result remains the Memory table.
type RunRecord struct {
	RunID                  string   `json:"run_id"`
	ProjectID              string   `json:"project_id,omitempty"`
	Agent                  string   `json:"agent,omitempty"`
	CLI                    string   `json:"cli,omitempty"`
	StartedAt              string   `json:"started_at"`
	FinishedAt             string   `json:"finished_at,omitempty"`
	Status                 string   `json:"status"`
	ToolCalls              int64    `json:"tool_calls"`
	MemoryMutationAttempts int64    `json:"memory_mutation_attempts"`
	TargetIDs              []string `json:"target_ids,omitempty"`
	ErrorSummary           string   `json:"error_summary,omitempty"`
}

type runLedger struct {
	store            *lancestore.Store
	table            *lancestore.Table
	localWriteTarget string
}

func openRunLedger(ctx context.Context, projectDir string) (*runLedger, error) {
	uri, s3, err := runLedgerStorage(ctx, projectDir)
	if err != nil {
		return nil, err
	}
	openCtx, guard, err := acquireLocalRunLedgerWrite(ctx, uri)
	if err != nil {
		return nil, fmt.Errorf("locking Dream run ledger: %w", err)
	}
	defer guard.Release()
	st, err := lancestore.Open(openCtx, lancestore.Config{
		URI: uri, S3: s3, Writable: true, StrongReadConsistency: true,
	})
	if err != nil {
		return nil, fmt.Errorf("opening Dream run ledger: %w", err)
	}
	table, err := st.EnsureTable(openCtx, dreamRunsTable, lancestore.Schema{Fields: []lancestore.Field{
		{Name: "run_id", Type: lancestore.FieldString},
		{Name: "project_id", Type: lancestore.FieldString, Nullable: true},
		{Name: "agent", Type: lancestore.FieldString, Nullable: true},
		{Name: "cli", Type: lancestore.FieldString, Nullable: true},
		{Name: "started_at", Type: lancestore.FieldString},
		{Name: "finished_at", Type: lancestore.FieldString, Nullable: true},
		{Name: "status", Type: lancestore.FieldString},
		{Name: "tool_calls", Type: lancestore.FieldInt64},
		{Name: "memory_mutation_attempts", Type: lancestore.FieldInt64},
		{Name: "target_ids_json", Type: lancestore.FieldString, Nullable: true},
		{Name: "error_summary", Type: lancestore.FieldString, Nullable: true},
	}})
	if err != nil {
		_ = st.Close()
		return nil, fmt.Errorf("opening Dream run table: %w", err)
	}
	localWriteTarget := ""
	if !strings.HasPrefix(uri, "s3://") {
		localWriteTarget = uri
	}
	return &runLedger{store: st, table: table, localWriteTarget: localWriteTarget}, nil
}

func (l *runLedger) Close() error {
	if l == nil || l.store == nil {
		return nil
	}
	return l.store.Close()
}

func (l *runLedger) Put(ctx context.Context, record RunRecord) error {
	if l == nil || l.table == nil {
		return fmt.Errorf("dream run ledger is not open")
	}
	targets, err := json.Marshal(record.TargetIDs)
	if err != nil {
		return err
	}
	row := lancestore.Row{
		"run_id": record.RunID, "project_id": record.ProjectID, "agent": record.Agent, "cli": record.CLI,
		"started_at": record.StartedAt, "finished_at": record.FinishedAt, "status": record.Status,
		"tool_calls": record.ToolCalls, "memory_mutation_attempts": record.MemoryMutationAttempts,
		"target_ids_json": string(targets), "error_summary": truncateRunError(record.ErrorSummary),
	}
	writeCtx := ctx
	var guard *storelifecycle.Guard
	if l.localWriteTarget != "" {
		var err error
		writeCtx, guard, err = storelifecycle.Acquire(ctx, l.localWriteTarget)
		if err != nil {
			return fmt.Errorf("locking Dream run write: %w", err)
		}
		defer guard.Release()
	}
	// A table handle is a snapshot. Refresh after taking the local cross-process
	// lock so an independent writer's row is part of this merge's base version.
	// S3 still refreshes here and relies on conditional commit retry for writers on
	// different hosts, where a filesystem lock cannot coordinate them.
	if err := l.table.Refresh(writeCtx); err != nil {
		return fmt.Errorf("refreshing Dream run ledger: %w", err)
	}
	return l.table.Upsert(writeCtx, "run_id", []lancestore.Row{row})
}

func acquireLocalRunLedgerWrite(ctx context.Context, uri string) (context.Context, *storelifecycle.Guard, error) {
	if strings.HasPrefix(uri, "s3://") {
		return ctx, nil, nil
	}
	return storelifecycle.Acquire(ctx, uri)
}

// LatestRun returns the newest operational record without exposing model output
// or Memory content. Absence is not an error.
func LatestRun(ctx context.Context, projectDir string) (*RunRecord, error) {
	uri, s3, err := runLedgerStorage(ctx, projectDir)
	if err != nil {
		return nil, err
	}
	if !strings.HasPrefix(uri, "s3://") {
		if _, err := os.Stat(uri); os.IsNotExist(err) {
			return nil, nil
		}
	}
	st, err := lancestore.Open(ctx, lancestore.Config{
		URI: uri, S3: s3, Writable: false, StrongReadConsistency: true,
	})
	if err != nil {
		return nil, err
	}
	defer func() { _ = st.Close() }()
	table, err := st.OpenTable(ctx, dreamRunsTable)
	if err != nil {
		if errors.Is(err, lancestore.ErrNoSuchTable) {
			return nil, nil
		}
		return nil, err
	}
	defer func() { _ = table.Close() }()
	rows, err := table.Rows(ctx)
	if err != nil {
		return nil, err
	}
	if len(rows) == 0 {
		return nil, nil
	}
	sort.Slice(rows, func(i, j int) bool {
		return rowString(rows[i], "started_at") > rowString(rows[j], "started_at")
	})
	row := rows[0]
	record := &RunRecord{
		RunID: rowString(row, "run_id"), ProjectID: rowString(row, "project_id"),
		Agent: rowString(row, "agent"), CLI: rowString(row, "cli"),
		StartedAt: rowString(row, "started_at"), FinishedAt: rowString(row, "finished_at"),
		Status: rowString(row, "status"), ToolCalls: rowInt64(row, "tool_calls"),
		MemoryMutationAttempts: rowInt64(row, "memory_mutation_attempts"),
		ErrorSummary:           truncateRunError(rowString(row, "error_summary")),
	}
	_ = json.Unmarshal([]byte(rowString(row, "target_ids_json")), &record.TargetIDs)
	return record, nil
}

func runLedgerStorage(ctx context.Context, projectDir string) (string, config.S3Config, error) {
	projectID := store.ProjectID(projectDir)
	if projectID == "" {
		return "", config.S3Config{}, fmt.Errorf("dream run ledger requires an initialized project identity")
	}
	local := func() (string, config.S3Config, error) {
		return store.DreamTableDir(projectID), config.S3Config{}, nil
	}
	s3 := config.ProjectS3Config(ctx, projectID, auth.BrokerStorageModuleDream)
	if s3.ResolutionError != nil {
		return "", config.S3Config{}, fmt.Errorf("resolving Dream storage: %w", s3.ResolutionError)
	}
	if !s3.Configured() {
		return local()
	}
	prefix := hubaccess.ProjectDreamPrefix(projectID)
	if prefix == "" {
		return "", config.S3Config{}, fmt.Errorf("invalid Dream project identity %q", projectID)
	}
	return s3store.URI(s3.Bucket, s3store.JoinKey(s3.Prefix, prefix)), s3, nil
}

func rowString(row lancestore.Row, key string) string {
	if value, ok := row[key].(string); ok {
		return value
	}
	return fmt.Sprint(row[key])
}

func rowInt64(row lancestore.Row, key string) int64 {
	switch value := row[key].(type) {
	case int64:
		return value
	case int:
		return int64(value)
	case float64:
		return int64(value)
	default:
		return 0
	}
}

func newRunRecord(projectDir, runID, agent, cli string, now time.Time) RunRecord {
	return RunRecord{
		RunID: runID, ProjectID: store.ProjectID(projectDir), Agent: agent, CLI: cli,
		StartedAt: now.UTC().Format(time.RFC3339Nano), Status: "running",
	}
}

func truncateRunError(value string) string {
	const max = 512
	value = strings.TrimSpace(value)
	if len(value) <= max {
		return value
	}
	return value[:max]
}

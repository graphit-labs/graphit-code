package lancestore

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

// RebuildPlan replaces a table whose schema must change while keeping every row it holds.
//
// Lance cannot alter a column's type in place, so a schema change means dropping the table
// and creating it again. Done naively — read rows into memory, drop, create, append — an
// interruption between the drop and the append loses the whole table, and so does a
// schema computed from a transient configuration. RebuildTable makes it recoverable: the
// rows are read first (a failed read aborts before anything is touched), then written to
// a durable snapshot, and only then is the table dropped. A snapshot left pending by an
// interrupted rebuild is replayed by RecoverRebuild the next time the table is opened.
type RebuildPlan struct {
	// Table is the table to rebuild.
	Table string
	// Schema is the schema the table is rebuilt with. It must be valid.
	Schema Schema
	// Adapt turns a stored row into one of Schema. Nil keeps the row's values and fills
	// every column the row lacks with its empty value.
	Adapt func(Row) Row
	// BranchKey identifies a row when a native Lance branch is replayed onto the rebuilt
	// table. Empty refuses to rebuild a table that has branches, since dropping the table
	// drops them with it.
	BranchKey []string
	// BackupDir is the local directory that receives the snapshot. Required.
	BackupDir string
}

// rebuildSnapshot is the durable state of a rebuild: the rows the table must hold
// afterwards, already adapted to the new schema.
type rebuildSnapshot struct {
	Version   int              `json:"v"`
	URI       string           `json:"uri"`
	Table     string           `json:"table"`
	CreatedAt string           `json:"created_at"`
	BranchKey []string         `json:"branch_key,omitempty"`
	Schema    Schema           `json:"schema"`
	Main      []Row            `json:"main"`
	Branches  map[string][]Row `json:"branches,omitempty"`
}

// ErrRebuildRefused reports a rebuild that would lose data and was therefore not started.
var ErrRebuildRefused = errors.New("lancestore: rebuilding this table would lose data")

// RebuildBackupDir is the snapshot directory for the store at uri under root: one per
// store, named by a hash of its URI so local paths and s3:// URIs both fit.
func RebuildBackupDir(root, uri string) string {
	sum := sha256.Sum256([]byte(uri))
	return filepath.Join(root, "backups", "lance-rebuild", hex.EncodeToString(sum[:8]))
}

// RebuildTable rebuilds plan.Table in the store cfg names with plan.Schema, keeping its
// rows (and its branches' rows when plan.BranchKey is set). cfg.Branch must be empty: a
// rebuild always works on the whole dataset.
func RebuildTable(ctx context.Context, cfg Config, plan RebuildPlan) error {
	if plan.BackupDir == "" || plan.Table == "" {
		return fmt.Errorf("lancestore: a rebuild needs a table and a backup directory")
	}
	if err := plan.Schema.Validate(); err != nil {
		return fmt.Errorf("%w: the target schema of %s is invalid: %w", ErrRebuildRefused, plan.Table, err)
	}
	cfg.Branch, cfg.NoFork, cfg.Writable = "", false, true
	unlock, err := lockRebuild(plan.BackupDir, plan.Table)
	if err != nil {
		return err
	}
	defer unlock()
	// A previous rebuild that did not finish must complete before a new one starts.
	if _, err := recoverRebuildLocked(ctx, cfg, plan.Table, plan.BackupDir); err != nil {
		return err
	}

	snapshot, err := readRebuildSnapshot(ctx, cfg, plan)
	if err != nil {
		return err
	}
	pending, err := writeRebuildSnapshot(plan.BackupDir, snapshot)
	if err != nil {
		return err
	}
	if err := replayRebuild(ctx, cfg, plan.Schema, snapshot); err != nil {
		return fmt.Errorf("lancestore: rebuilding %s (its rows are kept in %s and are restored on the next open): %w", plan.Table, pending, err)
	}
	return finishRebuild(pending, plan.Table)
}

// RecoverRebuild completes a rebuild of table that was interrupted, replaying its pending
// snapshot. It reports whether there was one. It is cheap when there is nothing to do.
func RecoverRebuild(ctx context.Context, cfg Config, table, backupDir string) (bool, error) {
	if backupDir == "" || len(pendingSnapshots(backupDir, table)) == 0 {
		return false, nil
	}
	cfg.Branch, cfg.NoFork, cfg.Writable = "", false, true
	unlock, err := lockRebuild(backupDir, table)
	if err != nil {
		return false, err
	}
	defer unlock()
	return recoverRebuildLocked(ctx, cfg, table, backupDir)
}

func recoverRebuildLocked(ctx context.Context, cfg Config, table, backupDir string) (bool, error) {
	pending := pendingSnapshots(backupDir, table)
	if len(pending) == 0 {
		return false, nil
	}
	// The newest pending snapshot is the state the table must reach.
	path := pending[len(pending)-1]
	data, err := os.ReadFile(path)
	if err != nil {
		return false, fmt.Errorf("lancestore: reading rebuild snapshot %s: %w", path, err)
	}
	var snapshot rebuildSnapshot
	if err := json.Unmarshal(data, &snapshot); err != nil {
		return false, fmt.Errorf("lancestore: decoding rebuild snapshot %s: %w", path, err)
	}
	if err := replayRebuild(ctx, cfg, snapshot.Schema, snapshot); err != nil {
		return false, fmt.Errorf("lancestore: completing the interrupted rebuild of %s from %s: %w", table, path, err)
	}
	for _, older := range pending {
		if err := finishRebuild(older, table); err != nil {
			return true, err
		}
	}
	return true, nil
}

func readRebuildSnapshot(ctx context.Context, cfg Config, plan RebuildPlan) (rebuildSnapshot, error) {
	snapshot := rebuildSnapshot{
		Version: 1, URI: cfg.URI, Table: plan.Table, CreatedAt: time.Now().UTC().Format(time.RFC3339Nano),
		BranchKey: plan.BranchKey, Schema: plan.Schema,
	}
	adapt := plan.Adapt
	if adapt == nil {
		adapt = func(row Row) Row { return row }
	}
	st, err := Open(ctx, cfg)
	if err != nil {
		return snapshot, err
	}
	defer func() { _ = st.Close() }()
	table, err := st.OpenTable(ctx, plan.Table)
	if err != nil {
		return snapshot, fmt.Errorf("%w: %s cannot be opened to save its rows: %w", ErrRebuildRefused, plan.Table, err)
	}
	rows, err := table.Rows(ctx)
	_ = table.Close()
	if err != nil {
		return snapshot, fmt.Errorf("%w: %s cannot be read in full: %w", ErrRebuildRefused, plan.Table, err)
	}
	snapshot.Main = adaptRows(rows, plan.Schema, adapt)

	branches, err := st.Branches(ctx, plan.Table)
	if err != nil && !errors.Is(err, ErrNoBranches) {
		return snapshot, fmt.Errorf("%w: the branches of %s cannot be listed: %w", ErrRebuildRefused, plan.Table, err)
	}
	if len(branches) == 0 {
		return snapshot, nil
	}
	if len(plan.BranchKey) == 0 {
		return snapshot, fmt.Errorf("%w: %s has %d project version branch(es) that dropping it would delete", ErrRebuildRefused, plan.Table, len(branches))
	}
	snapshot.Branches = map[string][]Row{}
	for name := range branches {
		bcfg := cfg
		bcfg.Branch = name
		bst, err := Open(ctx, bcfg)
		if err != nil {
			return snapshot, err
		}
		btable, err := bst.OpenTable(ctx, plan.Table)
		if err != nil {
			_ = bst.Close()
			return snapshot, fmt.Errorf("%w: branch %q of %s cannot be opened: %w", ErrRebuildRefused, name, plan.Table, err)
		}
		rows, err := btable.Rows(ctx)
		_ = btable.Close()
		_ = bst.Close()
		if err != nil {
			return snapshot, fmt.Errorf("%w: branch %q of %s cannot be read in full: %w", ErrRebuildRefused, name, plan.Table, err)
		}
		snapshot.Branches[name] = adaptRows(rows, plan.Schema, adapt)
	}
	return snapshot, nil
}

func adaptRows(rows []Row, schema Schema, adapt func(Row) Row) []Row {
	out := make([]Row, 0, len(rows))
	for _, row := range rows {
		next := adapt(row)
		shaped := Row{}
		for _, field := range schema.Fields {
			if value, ok := next[field.Name]; ok {
				shaped[field.Name] = value
			} else {
				shaped[field.Name] = EmptyValue(field)
			}
		}
		out = append(out, shaped)
	}
	return out
}

// EmptyValue is what a column holds for a row written before the column existed.
func EmptyValue(field Field) any {
	if field.Nullable {
		return nil
	}
	switch field.Type {
	case FieldInt64:
		return int64(0)
	case FieldFloat64:
		return float64(0)
	case FieldBool:
		return false
	case FieldVector:
		return nil
	default:
		return ""
	}
}

// replayRebuild makes the dataset hold exactly the snapshot: the table recreated with
// schema and the main rows, then every branch forked again and set to its rows.
func replayRebuild(ctx context.Context, cfg Config, schema Schema, snapshot rebuildSnapshot) error {
	st, err := Open(ctx, cfg)
	if err != nil {
		return err
	}
	names, err := st.TableNames(ctx)
	if err != nil {
		_ = st.Close()
		return err
	}
	for _, name := range names {
		if name == snapshot.Table {
			if err := st.DropTable(ctx, snapshot.Table); err != nil {
				_ = st.Close()
				return err
			}
			break
		}
	}
	table, err := st.CreateTable(ctx, snapshot.Table, schema)
	if err != nil {
		_ = st.Close()
		return err
	}
	if len(snapshot.Main) > 0 {
		if err := table.Append(ctx, coerceRows(schema, snapshot.Main)); err != nil {
			_ = st.Close()
			return err
		}
	}
	_ = st.Close()
	branchNames := make([]string, 0, len(snapshot.Branches))
	for name := range snapshot.Branches {
		branchNames = append(branchNames, name)
	}
	sort.Strings(branchNames)
	for _, name := range branchNames {
		bcfg := cfg
		bcfg.Branch = name
		bst, err := Open(ctx, bcfg)
		if err != nil {
			return err
		}
		btable, err := bst.OpenTable(ctx, snapshot.Table)
		if err == nil {
			_, err = btable.ReplaceSnapshot(ctx, snapshot.BranchKey, coerceRows(schema, snapshot.Branches[name]))
		}
		_ = bst.Close()
		if err != nil {
			return fmt.Errorf("restoring branch %q: %w", name, err)
		}
	}
	return nil
}

// coerceRows brings JSON-decoded values back to the types the schema stores.
func coerceRows(schema Schema, rows []Row) []Row {
	out := make([]Row, 0, len(rows))
	for _, row := range rows {
		next := Row{}
		for _, field := range schema.Fields {
			next[field.Name] = coerceValue(field, row[field.Name])
		}
		out = append(out, next)
	}
	return out
}

func coerceValue(field Field, value any) any {
	if value == nil {
		return EmptyValue(field)
	}
	switch field.Type {
	case FieldInt64:
		switch v := value.(type) {
		case float64:
			return int64(v)
		case json.Number:
			n, _ := v.Int64()
			return n
		}
	case FieldVector:
		switch v := value.(type) {
		case []any:
			vec := make([]float32, 0, len(v))
			for _, x := range v {
				f, _ := x.(float64)
				vec = append(vec, float32(f))
			}
			if field.Dim > 0 && len(vec) != field.Dim {
				return nil
			}
			return vec
		case []float32:
			if field.Dim > 0 && len(v) != field.Dim {
				return nil
			}
		}
	}
	return value
}

func writeRebuildSnapshot(dir string, snapshot rebuildSnapshot) (string, error) {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	data, err := json.Marshal(snapshot)
	if err != nil {
		return "", err
	}
	base := fmt.Sprintf("%s-%d", sanitizeTable(snapshot.Table), time.Now().UnixNano())
	path := filepath.Join(dir, base+".pending.json")
	if err := writeDurably(path, data); err != nil {
		return "", err
	}
	return path, nil
}

func writeDurably(path string, data []byte) error {
	tmp := path + ".tmp"
	f, err := os.OpenFile(tmp, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	if _, err := f.Write(data); err != nil {
		_ = f.Close()
		return err
	}
	if err := f.Sync(); err != nil {
		_ = f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// finishRebuild keeps the snapshot of a completed rebuild as a backup and keeps only the
// most recent few of that table.
func finishRebuild(pending, table string) error {
	done := strings.TrimSuffix(pending, ".pending.json") + ".done.json"
	if err := os.Rename(pending, done); err != nil {
		return err
	}
	dir := filepath.Dir(done)
	entries, _ := filepath.Glob(filepath.Join(dir, sanitizeTable(table)+"-*.done.json"))
	sort.Strings(entries)
	const keep = 5
	for len(entries) > keep {
		_ = os.Remove(entries[0])
		entries = entries[1:]
	}
	return nil
}

func pendingSnapshots(dir, table string) []string {
	matches, _ := filepath.Glob(filepath.Join(dir, sanitizeTable(table)+"-*.pending.json"))
	sort.Strings(matches)
	return matches
}

func lockRebuild(dir, table string) (func(), error) {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, err
	}
	lock, err := lockfile.Acquire(filepath.Join(dir, sanitizeTable(table)+".lock"), 2*time.Minute)
	if err != nil {
		return nil, fmt.Errorf("lancestore: waiting for another rebuild of %s: %w", table, err)
	}
	return lock.Release, nil
}

func sanitizeTable(name string) string {
	return strings.Map(func(r rune) rune {
		if r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '_' || r == '-' {
			return r
		}
		return '_'
	}, name)
}

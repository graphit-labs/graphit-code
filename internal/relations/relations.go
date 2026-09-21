// Package relations persists typed entity relationships in their source module's
// authorized store. It deliberately has no dependency on any domain module.
package relations

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"sort"
	"strconv"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

const TableName = "record_relations"

type Entity struct {
	Type    string `json:"type" yaml:"type"`
	ID      string `json:"id" yaml:"id"`
	Scope   string `json:"scope,omitempty" yaml:"scope,omitempty"`
	ScopeID string `json:"scope_id,omitempty" yaml:"scope_id,omitempty"`
	Context string `json:"context,omitempty" yaml:"context,omitempty"`
}
type Ref struct {
	Target   Entity `json:"target" yaml:"target"`
	Relation string `json:"relation" yaml:"relation"`
	Field    string `json:"field,omitempty" yaml:"field,omitempty"`
}
type Edge struct {
	Source   Entity `json:"source" yaml:"source"`
	Target   Entity `json:"target" yaml:"target"`
	Relation string `json:"relation" yaml:"relation"`
	Field    string `json:"field,omitempty" yaml:"field,omitempty"`
	Revision int64  `json:"source_revision" yaml:"source_revision"`
	Origin   string `json:"origin" yaml:"origin"`
}

func (e Entity) Key() string { data, _ := json.Marshal(e); return string(data) }
func Schema() lancestore.Schema {
	fields := []lancestore.Field{}
	for _, name := range []string{"key", "source_key", "source_type", "source_id", "source_scope", "source_scope_id", "source_context", "source_hash", "target_type", "target_id", "target_scope", "target_scope_id", "target_context", "relation", "field", "origin"} {
		fields = append(fields, lancestore.Field{Name: name, Type: lancestore.FieldString})
	}
	fields = append(fields, lancestore.Field{Name: "source_revision", Type: lancestore.FieldInt64}, lancestore.Field{Name: "marker", Type: lancestore.FieldBool})
	return lancestore.Schema{Fields: fields}
}
func quote(value string) string { return "'" + strings.ReplaceAll(value, "'", "''") + "'" }
func row(source Entity, ref Ref, revision int64, origin string, marker bool, snapshot string) lancestore.Row {
	identity := snapshot + source.Key() + ":" + strconv.FormatInt(revision, 10) + ":" + origin + ":" + ref.Target.Key() + ":" + ref.Relation + ":" + ref.Field
	hash := sha256.Sum256([]byte(identity))
	return lancestore.Row{"key": hex.EncodeToString(hash[:]), "source_key": source.Key(), "source_type": source.Type, "source_id": source.ID, "source_scope": source.Scope, "source_scope_id": source.ScopeID, "source_context": source.Context, "source_hash": snapshot, "target_type": ref.Target.Type, "target_id": ref.Target.ID, "target_scope": ref.Target.Scope, "target_scope_id": ref.Target.ScopeID, "target_context": ref.Target.Context, "relation": ref.Relation, "field": ref.Field, "origin": origin, "source_revision": revision, "marker": marker}
}

// Replace appends one complete immutable generation in ONE table commit. The
// marker represents an empty generation too. An older writer cannot resurrect
// links: readers select the highest revision for each source/origin pair.
func Replace(ctx context.Context, store *lancestore.Store, source Entity, revision int64, origin string, refs []Ref, snapshot ...string) error {
	if source.Type == "" || source.ID == "" || revision < 0 {
		return fmt.Errorf("relations: source type, id and nonnegative revision required")
	}
	table, err := store.EnsureTable(ctx, TableName, Schema())
	if err != nil {
		return err
	}
	defer table.Close()
	hash := ""
	if len(snapshot) > 0 {
		hash = snapshot[0]
	}
	rows := []lancestore.Row{row(source, Ref{}, revision, origin, true, hash)}
	seen := map[string]bool{}
	for _, ref := range refs {
		if ref.Target.Type == "" || ref.Target.ID == "" {
			continue
		}
		r := row(source, ref, revision, origin, false, hash)
		key := r["key"].(string)
		if !seen[key] {
			rows = append(rows, r)
			seen[key] = true
		}
	}
	_, err = table.Merge(ctx, lancestore.MergeOptions{KeyColumn: "key", MatchCondition: "false", InsertIfMissing: true}, rows)
	return err
}

// Read returns current persisted edges. It never parses or materializes records.
// A missing table identifies legacy data needing an explicit reconciliation.
func Read(ctx context.Context, store *lancestore.Store) ([]Edge, bool, error) {
	return ReadMatching(ctx, store, nil)
}

// ReadMatching checks persisted generations against authoritative current heads.
// This makes a partial cross-table write explicit instead of returning old edges.
func ReadMatching(ctx context.Context, store *lancestore.Store, heads map[string]string) ([]Edge, bool, error) {
	names, err := store.TableNames(ctx)
	if err != nil {
		return nil, false, err
	}
	exists := false
	for _, name := range names {
		if name == TableName {
			exists = true
		}
	}
	if !exists {
		return []Edge{}, heads != nil && len(heads) == 0, nil
	}
	table, err := store.OpenTable(ctx, TableName)
	if err != nil {
		return nil, false, err
	}
	defer table.Close()
	rows, err := table.Rows(ctx)
	if err != nil {
		return nil, true, err
	}
	latest := map[string]int64{}
	foundHeads := map[string]bool{}
	valid := func(r lancestore.Row) bool {
		if heads == nil {
			return true
		}
		key, _ := r["source_key"].(string)
		hash, exists := heads[key]
		if !exists {
			return false
		}
		if r["origin"] != "record" {
			return true
		}
		return r["source_hash"] == hash
	}
	number := func(v any) int64 {
		switch n := v.(type) {
		case int64:
			return n
		case int:
			return int64(n)
		case float64:
			return int64(n)
		}
		return 0
	}
	str := func(r lancestore.Row, key string) string { v, _ := r[key].(string); return v }
	for _, r := range rows {
		if !valid(r) {
			continue
		}
		if marker, _ := r["marker"].(bool); marker {
			if str(r, "origin") == "record" {
				foundHeads[str(r, "source_key")] = true
			}
			key := str(r, "source_key") + ":" + str(r, "origin")
			rev := number(r["source_revision"])
			if rev > latest[key] {
				latest[key] = rev
			}
		}
	}
	entity := func(r lancestore.Row, prefix string) Entity {
		return Entity{str(r, prefix+"type"), str(r, prefix+"id"), str(r, prefix+"scope"), str(r, prefix+"scope_id"), str(r, prefix+"context")}
	}
	out := []Edge{}
	for _, r := range rows {
		if !valid(r) {
			continue
		}
		if marker, _ := r["marker"].(bool); marker {
			continue
		}
		revision := number(r["source_revision"])
		if revision != latest[str(r, "source_key")+":"+str(r, "origin")] {
			continue
		}
		out = append(out, Edge{entity(r, "source_"), entity(r, "target_"), str(r, "relation"), str(r, "field"), revision, str(r, "origin")})
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i].Source.Key()+out[i].Target.Key()+out[i].Field < out[j].Source.Key()+out[j].Target.Key()+out[j].Field
	})
	complete := true
	for key := range heads {
		if !foundHeads[key] {
			complete = false
		}
	}
	return out, complete, nil
}

// Remove leaves an empty generation for every known source identity/origin.
// Domain deletion recovery may safely replay this operation.
func Remove(ctx context.Context, store *lancestore.Store, kind, id string) error {
	edges, _, err := Read(ctx, store)
	if err != nil {
		return err
	}
	versions := map[string]Edge{}
	for _, e := range edges {
		if e.Source.Type == kind && e.Source.ID == id {
			versions[e.Source.Key()+e.Origin] = e
		}
	}
	for _, e := range versions {
		if err := Replace(ctx, store, e.Source, e.Revision+1, e.Origin, nil); err != nil {
			return err
		}
	}
	return nil
}

func Fingerprint(value any) string {
	data, _ := json.Marshal(value)
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:])
}

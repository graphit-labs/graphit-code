package ast

import (
	"fmt"
	"reflect"
	"sort"
	"strings"
	"testing"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// The reference implementation: node rows as a slice of maps, which is what the exporter
// did before it streamed them into typed columns. It is kept here, and only here, so the
// streaming path has something to be proven equal to.
func referenceColumnsForLabel(label string, rows []map[string]any) ([]ladybug.Field, string) {
	seen := map[string]bool{}
	var names []string
	for _, r := range rows {
		for k := range r {
			if !seen[k] {
				seen[k] = true
				names = append(names, k)
			}
		}
	}
	ordered := make([]string, 0, len(names))
	for _, g := range graphColumnOrder {
		if seen[g] {
			ordered = append(ordered, g)
		}
	}
	var rest []string
	for _, n := range names {
		if !referenceSeenIn(ordered, n) {
			rest = append(rest, n)
		}
	}
	sort.Strings(rest)
	ordered = append(ordered, rest...)

	cols := make([]ladybug.Field, 0, len(ordered))
	for _, n := range ordered {
		var values []any
		for _, r := range rows {
			if v, ok := r[n]; ok {
				values = append(values, v)
			}
		}
		cols = append(cols, ladybug.Field{Name: n, Type: inferTypeFor(values)})
	}
	return cols, referencePrimaryKey(label, rows)
}

func referencePrimaryKey(label string, rows []map[string]any) string {
	for _, r := range rows {
		if _, ok := r["path"]; ok && (label == "File" || label == "Directory") {
			return "path"
		}
		break
	}
	return "uid"
}

func referenceSeenIn(list []string, s string) bool {
	for _, e := range list {
		if e == s {
			return true
		}
	}
	return false
}

func referenceSortedRows(rows []map[string]any, pk string) []map[string]any {
	out := append([]map[string]any(nil), rows...)
	sort.SliceStable(out, func(i, j int) bool {
		return strings.Compare(fmt.Sprint(out[i][pk]), fmt.Sprint(out[j][pk])) < 0
	})
	return out
}

func nodeColumnsFrom(rows []map[string]any) *nodeColumns {
	t := newNodeColumns()
	for _, r := range rows {
		t.appendRow(r)
	}
	return t
}

// The streaming, columnar table must be the map-built table: same columns in the same
// order, same types, same primary key, same row order, same value in every cell. A drift
// in any of those repoints edges at the wrong nodes without erroring.
func TestNodeColumnsMatchTheRowMapTable(t *testing.T) {
	cases := []struct {
		name  string
		label string
		rows  []map[string]any
	}{
		{
			name:  "entities carry the canonical column set",
			label: "Function",
			rows: []map[string]any{
				entityToJSON(cachedEntity{
					Label: "Function", UID: "b.go:Zeta", Name: "Zeta", Path: "b.go",
					Line: 3, EndLine: 9, Docstring: "second", Lang: "go", Complexity: 2,
					IsExported: true,
				}, false, "core"),
				entityToJSON(cachedEntity{
					Label: "Function", UID: "a.go:Alpha", Name: "Alpha", Path: "a.go",
					Line: 1, EndLine: 4, Lang: "go", Complexity: 7,
				}, false, "core"),
				stubJSON("fmt.Println", "go", "core"),
			},
		},
		{
			name:  "File keys on path",
			label: "File",
			rows: []map[string]any{
				{"path": "z.go", "name": "z.go", "relative_path": "z.go", "is_dependency": false, "lang": "go", "cluster": "core"},
				{"path": "a.go", "name": "a.go", "relative_path": "a.go", "is_dependency": true, "lang": "go", "cluster": "core"},
			},
		},
		{
			name:  "a key missing from some rows becomes a nullable column",
			label: "Module",
			rows: []map[string]any{
				{"uid": "m1", "name": "one", "lang": "go", "is_stub": false},
				{"uid": "m2", "name": "two", "lang": "go", "is_stub": false, "cluster": "extra"},
				{"uid": "m0", "name": "zero", "lang": "go", "is_stub": false, "full_import_name": "x/y"},
			},
		},
		{
			name:  "a column of mixed types collapses to STRING",
			label: "Mixed",
			rows: []map[string]any{
				{"uid": "a", "value": 42},
				{"uid": "b", "value": "text"},
				{"uid": "c", "value": true},
			},
		},
		{
			name:  "a key present with a nil value is a STRING column and a null cell",
			label: "Nullish",
			rows: []map[string]any{
				{"uid": "a", "value": nil},
				{"uid": "b", "value": nil},
			},
		},
		{
			name:  "an integer column stays INT64",
			label: "Ints",
			rows: []map[string]any{
				{"uid": "a", "line_number": 10, "end_line": int64(20)},
				{"uid": "b", "line_number": 3, "end_line": int64(4)},
			},
		},
		{
			name:  "a boolean column stays BOOL",
			label: "Bools",
			rows: []map[string]any{
				{"uid": "a", "is_exported": true},
				{"uid": "b", "is_exported": false},
			},
		},
		{
			name:  "equal primary keys keep their arrival order",
			label: "Dupes",
			rows: []map[string]any{
				{"uid": "same", "name": "first"},
				{"uid": "same", "name": "second"},
				{"uid": "same", "name": "third"},
			},
		},
		{
			name:  "an empty table",
			label: "Empty",
			rows:  nil,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			wantCols, wantPK := referenceColumnsForLabel(tc.label, tc.rows)
			table := nodeColumnsFrom(tc.rows)
			gotCols, columns, gotPK := table.fields(tc.label)

			if gotPK != wantPK {
				t.Errorf("primary key = %q, want %q", gotPK, wantPK)
			}
			if !reflect.DeepEqual(gotCols, wantCols) {
				t.Fatalf("columns =\n  %+v\nwant\n  %+v", gotCols, wantCols)
			}
			if table.rows != len(tc.rows) {
				t.Fatalf("rows = %d, want %d", table.rows, len(tc.rows))
			}

			wantRows := referenceSortedRows(tc.rows, wantPK)
			order, _ := table.sortedOrder(gotPK)
			for i, want := range wantRows {
				for ci, col := range columns {
					gotVal, gotSet := col.valueAt(int(order[i]))
					wantVal, wantSet := want[gotCols[ci].Name]
					if wantVal == nil {
						wantSet = false
					}
					if gotSet != wantSet {
						t.Errorf("row %d column %q: present = %v, want %v", i, gotCols[ci].Name, gotSet, wantSet)
						continue
					}
					if !gotSet {
						continue
					}
					if !sameCell(gotCols[ci].Type, gotVal, wantVal) {
						t.Errorf("row %d column %q: %#v, want %#v", i, gotCols[ci].Name, gotVal, wantVal)
					}
				}
			}
		})
	}
}

// sameCell compares what the streaming column stores against what appendArrowValueDirect
// would have written from the raw map value, for the column's declared type.
func sameCell(cypherType string, got, want any) bool {
	switch cypherType {
	case "INT64":
		return got == nodeInt64(want)
	case "BOOL":
		w, ok := want.(bool)
		return ok && got == w
	default:
		return got == nodeString(want)
	}
}

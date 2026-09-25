package ast

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/apache/arrow-go/v18/arrow"
	"github.com/apache/arrow-go/v18/parquet/file"
)

func TestDirectIcebugRelationshipPropertiesMatchSchemaOrder(t *testing.T) {
	ctx := context.Background()
	work := t.TempDir()
	if err := os.WriteFile(filepath.Join(work, "calls.go"), []byte("package demo\nfunc A() { B() }\nfunc B() {}\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	storeDir := t.TempDir()
	bundleDir := filepath.Join(storeDir, "graph.icebug")
	cfg := LadybugConfig{StoreDir: storeDir, IcebugDir: bundleDir}
	db := NewLadybugDB(cfg)
	if _, err := RunPipeline(ctx, db, work, PipelineOptions{CacheDir: storeDir, SkipExternal: true}); err != nil {
		t.Fatal(err)
	}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}

	schema, err := os.ReadFile(filepath.Join(bundleDir, "schema.cypher"))
	if err != nil {
		t.Fatal(err)
	}
	const declaration = "CREATE REL TABLE `calls__function_function`(FROM `Function` TO `Function`, `source_file` STRING, `line_number` INT64, `full_call_name` STRING, `receiver_type` STRING, `uid` STRING)"
	if !strings.Contains(string(schema), declaration) {
		t.Fatalf("unexpected CALLS schema:\n%s", schema)
	}
	parquetFile := filepath.Join(bundleDir, "indices_calls__function_function.parquet")
	reader, err := file.OpenParquetFile(parquetFile, false)
	if err != nil {
		t.Fatal(err)
	}
	physical := reader.MetaData().Schema
	columns := make([]string, physical.NumColumns())
	for i := range columns {
		columns[i] = physical.Column(i).Name()
	}
	reader.Close()
	if want := []string{"target", "source_file", "line_number", "full_call_name", "receiver_type", "uid"}; !reflect.DeepEqual(columns, want) {
		t.Fatalf("Parquet columns = %v, want %v", columns, want)
	}

	db = NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()
	result, err := db.Query(ctx, "MATCH (:Function)-[r:calls__function_function]->(:Function) RETURN r.source_file AS source_file, r.line_number AS line_number, r.uid AS uid", nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(result.Records) != 1 || result.Records[0]["source_file"] != "calls.go" || result.Records[0]["line_number"] != int64(2) || result.Records[0]["uid"] == "" {
		t.Fatalf("wrong relationship properties: %v", result.Records)
	}
	whole, err := db.Query(ctx, "MATCH (:Function)-[r:calls__function_function]->(:Function) RETURN r", nil)
	if err != nil || len(whole.Records) != 1 {
		t.Fatalf("materialize relationship: %v, %v", whole, err)
	}
	relationship, ok := whole.Records[0]["r"].(map[string]any)
	if !ok {
		t.Fatalf("unexpected relationship shape: %v", whole.Records[0])
	}
	properties, _ := relationship["Properties"].(map[string]any)
	if properties["source_file"] != "calls.go" || properties["line_number"] != int64(2) {
		t.Fatalf("wrong materialized relationship: %v", whole.Records[0])
	}
}

func TestDirectIcebugRelationshipColumnsValidateDeclaredShape(t *testing.T) {
	table := newNodeColumns()
	table.appendRow(map[string]any{"source_file": "a.go", "uid": "rel:1"})
	fields, columns, err := relationshipColumnsDirect(table, "REFERENCES")
	if err != nil {
		t.Fatal(err)
	}
	if len(fields) != 3 || fields[0].Name != "source_file" || fields[1].Name != "line_number" || fields[2].Name != "uid" || columns[1] != nil {
		t.Fatalf("missing declared property must retain a null column: fields=%v columns=%v", fields, columns)
	}
	arrowFields := make([]arrow.Field, len(fields))
	for i, field := range fields {
		arrowFields[i] = arrow.Field{Name: field.Name, Type: arrowTypeForCypherDirect(field.Type), Nullable: true}
	}
	missingFile := filepath.Join(t.TempDir(), "indices.parquet")
	if err := writeIndicesDirect(missingFile, csrMemberDirect{
		edges: []csrEdgeDirect{{source: 0, target: 1}}, order: []int32{0}, props: columns,
	}, arrowFields); err != nil {
		t.Fatal(err)
	}
	rdr, err := file.OpenParquetFile(missingFile, false)
	if err != nil {
		t.Fatal(err)
	}
	defer rdr.Close()
	column, err := rdr.RowGroup(0).Column(2)
	if err != nil {
		t.Fatal(err)
	}
	lineReader := column.(*file.Int64ColumnChunkReader)
	values := make([]int64, 1)
	levels := make([]int16, 1)
	rows, nonNull, err := lineReader.ReadBatch(1, values, levels, nil)
	if err != nil || rows != 1 || nonNull != 0 || levels[0] != 0 {
		t.Fatalf("missing line_number should be Parquet null: rows=%d values=%d levels=%v err=%v", rows, nonNull, levels, err)
	}

	for _, tc := range []struct {
		name, want string
		row        map[string]any
	}{
		{"extra", "undeclared property", map[string]any{"source_file": "a.go", "line_number": 2, "extra": true, "uid": "rel:1"}},
		{"wrong type", "declared INT64", map[string]any{"source_file": "a.go", "line_number": "2", "uid": "rel:1"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			bad := newNodeColumns()
			bad.appendRow(tc.row)
			if _, _, err := relationshipColumnsDirect(bad, "REFERENCES"); err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("error = %v, want %q", err, tc.want)
			}
		})
	}
}

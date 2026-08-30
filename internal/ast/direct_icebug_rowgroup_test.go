package ast

import (
	"fmt"
	"path/filepath"
	"testing"

	"github.com/apache/arrow-go/v18/arrow"
	"github.com/apache/arrow-go/v18/arrow/array"
	"github.com/apache/arrow-go/v18/parquet/file"
)

// THE INVARIANT THAT COST THE MOST TO LEARN: one row group per file. The direct exporter
// feeds a table to pqarrow in chunks so the whole thing is never Arrow-resident at once,
// and WriteBuffered is what makes that safe — FileWriter.Write would open a row group per
// chunk, and a multi-row-group file silently fails to resolve a bound node variable.
func TestWriteParquetDirectKeepsOneRowGroupAcrossChunks(t *testing.T) {
	schema := arrow.NewSchema([]arrow.Field{
		{Name: "uid", Type: arrow.BinaryTypes.String, Nullable: true},
		{Name: "line_number", Type: arrow.PrimitiveTypes.Int64, Nullable: true},
	}, icebugMetadataDirect())

	for _, rows := range []int{0, 1, parquetChunkRows - 1, parquetChunkRows, parquetChunkRows + 1, parquetChunkRows*3 + 7} {
		t.Run(fmt.Sprintf("rows=%d", rows), func(t *testing.T) {
			dest := filepath.Join(t.TempDir(), "nodes_Probe.parquet")
			err := writeParquetDirect(dest, schema, rows, func(b *array.RecordBuilder, from, to int) {
				uid := b.Field(0).(*array.StringBuilder)
				line := b.Field(1).(*array.Int64Builder)
				for i := from; i < to; i++ {
					uid.Append(fmt.Sprintf("pkg/mod/file%d.go:sym%d", i%64, i))
					line.Append(int64(i))
				}
			})
			if err != nil {
				t.Fatalf("writeParquetDirect: %v", err)
			}

			rdr, err := file.OpenParquetFile(dest, false)
			if err != nil {
				t.Fatalf("open: %v", err)
			}
			defer rdr.Close()

			if got := rdr.MetaData().NumRowGroups(); got != 1 {
				t.Errorf("row groups = %d, want exactly 1", got)
			}
			if got := rdr.NumRows(); got != int64(rows) {
				t.Errorf("rows = %d, want %d", got, rows)
			}
			if kv := rdr.MetaData().KeyValueMetadata(); kv == nil || kv.FindValue("icebug_disk_version") == nil {
				t.Error("icebug_disk_version metadata is missing")
			}
		})
	}
}

// A chunked write must not reorder or drop rows: the dense node ids are the row indices,
// so a row landing one position off silently repoints every edge that referenced it.
func TestWriteParquetDirectPreservesRowOrderAcrossChunks(t *testing.T) {
	schema := arrow.NewSchema([]arrow.Field{
		{Name: "n", Type: arrow.PrimitiveTypes.Int64, Nullable: true},
	}, icebugMetadataDirect())

	const rows = parquetChunkRows*2 + 123
	dest := filepath.Join(t.TempDir(), "indices_probe.parquet")
	err := writeParquetDirect(dest, schema, rows, func(b *array.RecordBuilder, from, to int) {
		nb := b.Field(0).(*array.Int64Builder)
		for i := from; i < to; i++ {
			nb.Append(int64(i))
		}
	})
	if err != nil {
		t.Fatalf("writeParquetDirect: %v", err)
	}

	rdr, err := file.OpenParquetFile(dest, false)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer rdr.Close()

	col, err := rdr.RowGroup(0).Column(0)
	if err != nil {
		t.Fatalf("column: %v", err)
	}
	reader, ok := col.(*file.Int64ColumnChunkReader)
	if !ok {
		t.Fatalf("column reader is %T, want *file.Int64ColumnChunkReader", col)
	}
	values := make([]int64, rows)
	var read int
	for read < rows {
		_, n, err := reader.ReadBatch(int64(rows-read), values[read:], nil, nil)
		if err != nil {
			t.Fatalf("read batch: %v", err)
		}
		if n == 0 {
			break
		}
		read += n
	}
	if read != rows {
		t.Fatalf("read %d rows, want %d", read, rows)
	}
	for i, v := range values {
		if v != int64(i) {
			t.Fatalf("row %d = %d, want %d", i, v, i)
		}
	}
}

package ast

import (
	"fmt"
	"sort"

	"github.com/apache/arrow-go/v18/arrow/array"
	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

type nodeColumnKind uint8

const (
	nodeColumnInt nodeColumnKind = iota
	nodeColumnBool
	nodeColumnString
)

type nodeColumn struct {
	name    string
	kind    nodeColumnKind
	str     []string
	i64     []int64
	bl      []bool
	present []bool
}

// nodeColumns accumulates streamed node rows column by column instead of retaining the
// row maps.
//
// SAFETY: the column set, its order and each column's type must stay what
// columnsForLabel derives from the same rows — union of the keys, ordered by
// graphColumnOrder then alphabetically, typed by inferTypeFor. A column is promoted to
// STRING the moment it sees a value its current kind cannot hold, which is the same
// answer inferTypeFor gives for a mixed column.
type nodeColumns struct {
	cols         []*nodeColumn
	index        map[string]int
	rows         int
	firstHasPath bool
}

func newNodeColumns() *nodeColumns {
	return &nodeColumns{index: map[string]int{}}
}

func (t *nodeColumns) appendRow(r map[string]any) {
	if t.rows == 0 {
		_, t.firstHasPath = r["path"]
	}
	for name, v := range r {
		ci, ok := t.index[name]
		if !ok {
			ci = len(t.cols)
			t.index[name] = ci
			t.cols = append(t.cols, &nodeColumn{name: name, kind: kindOfNodeValue(v)})
		}
		t.cols[ci].set(t.rows, v)
	}
	t.rows++
	for _, c := range t.cols {
		c.padTo(t.rows)
	}
}

func kindOfNodeValue(v any) nodeColumnKind {
	switch v.(type) {
	case bool:
		return nodeColumnBool
	case int, int8, int16, int32, int64, uint, uint8, uint16, uint32, uint64:
		return nodeColumnInt
	default:
		return nodeColumnString
	}
}

// nodeInt64 accepts the three widths an entity row actually carries and answers zero for
// every other value that inferTypeFor still classified as an integer. The permissive
// default is deliberate: inferTypeFor decides the column type from the FIRST row, so a
// later row of an unexpected width must not be able to fail the write.
func nodeInt64(v any) int64 {
	switch n := v.(type) {
	case int64:
		return n
	case int:
		return int64(n)
	case int32:
		return int64(n)
	default:
		return 0
	}
}

func nodeString(v any) string {
	if s, ok := v.(string); ok {
		return s
	}
	return fmt.Sprint(v)
}

func (c *nodeColumn) padTo(n int) {
	for len(c.present) < n {
		c.present = append(c.present, false)
		switch c.kind {
		case nodeColumnInt:
			c.i64 = append(c.i64, 0)
		case nodeColumnBool:
			c.bl = append(c.bl, false)
		default:
			c.str = append(c.str, "")
		}
	}
}

func (c *nodeColumn) set(row int, v any) {
	if v == nil {
		c.promoteToString()
		c.padTo(row + 1)
		return
	}
	if k := kindOfNodeValue(v); k != c.kind {
		c.promoteToString()
	}
	c.padTo(row)
	switch c.kind {
	case nodeColumnInt:
		c.i64 = append(c.i64, nodeInt64(v))
	case nodeColumnBool:
		c.bl = append(c.bl, v.(bool))
	default:
		c.str = append(c.str, nodeString(v))
	}
	c.present = append(c.present, true)
}

func (c *nodeColumn) promoteToString() {
	if c.kind == nodeColumnString {
		return
	}
	c.str = make([]string, 0, len(c.present))
	switch c.kind {
	case nodeColumnInt:
		for i, ok := range c.present {
			if ok {
				c.str = append(c.str, fmt.Sprint(c.i64[i]))
			} else {
				c.str = append(c.str, "")
			}
		}
		c.i64 = nil
	case nodeColumnBool:
		for i, ok := range c.present {
			if ok {
				c.str = append(c.str, fmt.Sprint(c.bl[i]))
			} else {
				c.str = append(c.str, "")
			}
		}
		c.bl = nil
	}
	c.kind = nodeColumnString
}

func (c *nodeColumn) cypherType() string {
	switch c.kind {
	case nodeColumnInt:
		return "INT64"
	case nodeColumnBool:
		return "BOOL"
	default:
		return "STRING"
	}
}

func (t *nodeColumns) fields(label string) ([]ladybug.Field, []*nodeColumn, string) {
	ordered := make([]*nodeColumn, 0, len(t.cols))
	taken := make([]bool, len(t.cols))
	for _, name := range graphColumnOrder {
		if ci, ok := t.index[name]; ok {
			ordered = append(ordered, t.cols[ci])
			taken[ci] = true
		}
	}
	rest := make([]*nodeColumn, 0, len(t.cols))
	for ci, c := range t.cols {
		if !taken[ci] {
			rest = append(rest, c)
		}
	}
	sort.Slice(rest, func(i, j int) bool { return rest[i].name < rest[j].name })
	ordered = append(ordered, rest...)

	fields := make([]ladybug.Field, len(ordered))
	for i, c := range ordered {
		fields[i] = ladybug.Field{Name: c.name, Type: c.cypherType()}
	}
	return fields, ordered, t.primaryKey(label)
}

func (t *nodeColumns) primaryKey(label string) string {
	if t.firstHasPath && (label == "File" || label == "Directory") {
		return "path"
	}
	return "uid"
}

func (t *nodeColumns) sortedOrder(pk string) ([]int32, []string) {
	keys := make([]string, t.rows)
	if ci, ok := t.index[pk]; ok {
		c := t.cols[ci]
		for i := 0; i < t.rows; i++ {
			keys[i] = c.stringAt(i)
		}
	} else {
		for i := range keys {
			keys[i] = fmt.Sprint(nil)
		}
	}
	order := make([]int32, t.rows)
	for i := range order {
		order[i] = int32(i)
	}
	sort.SliceStable(order, func(a, b int) bool { return keys[order[a]] < keys[order[b]] })
	return order, keys
}

func (c *nodeColumn) stringAt(i int) string {
	if !c.present[i] {
		return fmt.Sprint(nil)
	}
	switch c.kind {
	case nodeColumnInt:
		return fmt.Sprint(c.i64[i])
	case nodeColumnBool:
		return fmt.Sprint(c.bl[i])
	default:
		return c.str[i]
	}
}

func (c *nodeColumn) appendTo(b array.Builder, i int) {
	if !c.present[i] {
		b.AppendNull()
		return
	}
	switch bb := b.(type) {
	case *array.StringBuilder:
		bb.Append(c.str[i])
	case *array.Int64Builder:
		bb.Append(c.i64[i])
	case *array.BooleanBuilder:
		bb.Append(c.bl[i])
	default:
		b.AppendNull()
	}
}

func (c *nodeColumn) valueAt(i int) (any, bool) {
	if !c.present[i] {
		return nil, false
	}
	switch c.kind {
	case nodeColumnInt:
		return c.i64[i], true
	case nodeColumnBool:
		return c.bl[i], true
	default:
		return c.str[i], true
	}
}

func (t *nodeColumns) appendRowExcept(r map[string]any, skipA, skipB string) {
	for name, v := range r {
		if name == skipA || name == skipB {
			continue
		}
		ci, ok := t.index[name]
		if !ok {
			ci = len(t.cols)
			t.index[name] = ci
			t.cols = append(t.cols, &nodeColumn{name: name, kind: kindOfNodeValue(v)})
		}
		t.cols[ci].set(t.rows, v)
	}
	t.rows++
	for _, c := range t.cols {
		c.padTo(t.rows)
	}
}

func (t *nodeColumns) sortedFields() ([]ladybug.Field, []*nodeColumn) {
	ordered := append([]*nodeColumn(nil), t.cols...)
	sort.Slice(ordered, func(i, j int) bool { return ordered[i].name < ordered[j].name })
	fields := make([]ladybug.Field, len(ordered))
	for i, c := range ordered {
		fields[i] = ladybug.Field{Name: c.name, Type: c.cypherType()}
	}
	return fields, ordered
}

func (c *nodeColumn) len() int { return len(c.present) }

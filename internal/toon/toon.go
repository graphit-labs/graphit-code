// Package toon provides a generic TOON (Token-Optimized Object Notation) formatter
// that converts arbitrary Go values into a compact pipe-delimited text format
// designed to minimize token consumption for AI agents.
//
// TOON Format:
//
//	Slice of structs: results[N]{field1|field2|...}:\n  val1|val2|...\n
//	Single struct:    {field1:val1|field2:val2|...}
//	Slice of strings: items[N]:\n  item1\n  item2\n
//	Maps:             {key1:val1|key2:val2|...}
package toon

import (
	"fmt"
	"reflect"
	"sort"
	"strings"
	"time"
)

// FormatAny converts any Go value into compact TOON format using reflection.
// It handles slices of structs, single structs, maps, string slices, and primitives.
func FormatAny(v any) string {
	if v == nil {
		return "null"
	}

	rv := reflect.ValueOf(v)

	// Dereference pointer
	for rv.Kind() == reflect.Pointer {
		if rv.IsNil() {
			return "null"
		}
		rv = rv.Elem()
	}

	switch rv.Kind() {
	case reflect.Slice, reflect.Array:
		return formatSlice(rv)
	case reflect.Struct:
		return formatStruct(rv)
	case reflect.Map:
		return formatMap(rv)
	default:
		return fmt.Sprintf("%v", rv.Interface())
	}
}

func formatSlice(rv reflect.Value) string {
	if rv.Len() == 0 {
		return "results[0]{}:"
	}

	elem := rv.Type().Elem()
	// Dereference pointer element types
	for elem.Kind() == reflect.Pointer {
		elem = elem.Elem()
	}

	switch elem.Kind() {
	case reflect.Struct:
		return formatStructSlice(rv)
	case reflect.String:
		return formatStringSlice(rv)
	default:
		// Generic slice — just list values
		var sb strings.Builder
		fmt.Fprintf(&sb, "items[%d]:\n", rv.Len())
		for i := 0; i < rv.Len(); i++ {
			fmt.Fprintf(&sb, "  %v\n", deref(rv.Index(i)).Interface())
		}
		return strings.TrimRight(sb.String(), "\n")
	}
}

func formatStructSlice(rv reflect.Value) string {
	if rv.Len() == 0 {
		return "results[0]{}:"
	}

	// Get field names from the first element
	first := deref(rv.Index(0))
	fields := structFields(first.Type())

	var sb strings.Builder
	// Header
	names := make([]string, len(fields))
	for i, f := range fields {
		names[i] = jsonFieldName(f)
	}
	fmt.Fprintf(&sb, "results[%d]{%s}:\n", rv.Len(), strings.Join(names, "|"))

	// Rows
	for i := 0; i < rv.Len(); i++ {
		elem := deref(rv.Index(i))
		vals := make([]string, len(fields))
		for j, f := range fields {
			fv := elem.FieldByIndex(f.Index)
			vals[j] = formatFieldValue(fv)
		}
		sb.WriteString("  ")
		sb.WriteString(strings.Join(vals, "|"))
		sb.WriteByte('\n')
	}

	return strings.TrimRight(sb.String(), "\n")
}

func formatStringSlice(rv reflect.Value) string {
	var sb strings.Builder
	fmt.Fprintf(&sb, "items[%d]:\n", rv.Len())
	for i := 0; i < rv.Len(); i++ {
		fmt.Fprintf(&sb, "  %s\n", rv.Index(i).String())
	}
	return strings.TrimRight(sb.String(), "\n")
}

func formatStruct(rv reflect.Value) string {
	fields := structFields(rv.Type())
	if len(fields) == 0 {
		return "{}"
	}

	parts := make([]string, 0, len(fields))
	for _, f := range fields {
		fv := rv.FieldByIndex(f.Index)
		name := jsonFieldName(f)
		val := formatFieldValue(fv)
		if val == "" || val == "0" || val == "false" || val == "null" {
			// Skip zero values for compactness
			continue
		}
		parts = append(parts, name+":"+val)
	}

	return "{" + strings.Join(parts, "|") + "}"
}

func formatMap(rv reflect.Value) string {
	if rv.Len() == 0 {
		return "{}"
	}

	keys := rv.MapKeys()
	strs := make([]string, 0, len(keys))
	for _, k := range keys {
		strs = append(strs, fmt.Sprintf("%v", k.Interface()))
	}
	sort.Strings(strs)

	parts := make([]string, 0, len(strs))
	for _, ks := range strs {
		val := rv.MapIndex(reflect.ValueOf(ks))
		parts = append(parts, ks+":"+formatFieldValue(val))
	}

	return "{" + strings.Join(parts, "|") + "}"
}

// structFields returns all exported struct fields, including embedded.
func structFields(t reflect.Type) []reflect.StructField {
	var fields []reflect.StructField
	for i := 0; i < t.NumField(); i++ {
		f := t.Field(i)
		if !f.IsExported() {
			continue
		}
		// Skip fields with json:"-"
		tag := f.Tag.Get("json")
		if tag == "-" {
			continue
		}
		fields = append(fields, f)
	}
	return fields
}

// jsonFieldName extracts the JSON field name from a struct field tag,
// falling back to the Go field name in snake_case style.
func jsonFieldName(f reflect.StructField) string {
	tag := f.Tag.Get("json")
	if tag != "" {
		name := strings.SplitN(tag, ",", 2)[0]
		if name != "" && name != "-" {
			return name
		}
	}
	return strings.ToLower(f.Name)
}

func formatFieldValue(rv reflect.Value) string {
	// Dereference pointer
	for rv.Kind() == reflect.Pointer {
		if rv.IsNil() {
			return ""
		}
		rv = rv.Elem()
	}

	// Handle interface values
	if rv.Kind() == reflect.Interface {
		if rv.IsNil() {
			return ""
		}
		rv = rv.Elem()
	}

	iface := rv.Interface()

	// Handle time.Time specially
	if t, ok := iface.(time.Time); ok {
		if t.IsZero() {
			return ""
		}
		return t.Format(time.RFC3339)
	}

	switch rv.Kind() {
	case reflect.String:
		s := rv.String()
		// Sanitize pipe and newline characters
		s = strings.ReplaceAll(s, "|", "/")
		s = strings.ReplaceAll(s, "\n", " ")
		if len(s) > 200 {
			s = s[:200] + "…"
		}
		return s
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return fmt.Sprintf("%d", rv.Int())
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		return fmt.Sprintf("%d", rv.Uint())
	case reflect.Float32, reflect.Float64:
		f := rv.Float()
		if f == float64(int64(f)) {
			return fmt.Sprintf("%d", int64(f))
		}
		return fmt.Sprintf("%.2f", f)
	case reflect.Bool:
		if rv.Bool() {
			return "true"
		}
		return "false"
	case reflect.Slice, reflect.Array:
		if rv.Len() == 0 {
			return ""
		}
		parts := make([]string, rv.Len())
		for i := 0; i < rv.Len(); i++ {
			parts[i] = formatFieldValue(rv.Index(i))
		}
		return "[" + strings.Join(parts, ",") + "]"
	case reflect.Map:
		if rv.Len() == 0 {
			return ""
		}
		return formatMap(rv)
	case reflect.Struct:
		return formatStruct(rv)
	default:
		return fmt.Sprintf("%v", rv.Interface())
	}
}

func deref(rv reflect.Value) reflect.Value {
	for rv.Kind() == reflect.Pointer {
		if rv.IsNil() {
			return rv
		}
		rv = rv.Elem()
	}
	return rv
}

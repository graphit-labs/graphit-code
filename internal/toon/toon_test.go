package toon

import (
	"reflect"
	"strings"
	"testing"
	"time"
)

type sampleEntry struct {
	ID    string  `json:"id"`
	Name  string  `json:"name"`
	Score float64 `json:"score"`
}

type sampleStatus struct {
	Running   bool      `json:"running"`
	PID       int       `json:"pid"`
	StartedAt time.Time `json:"started_at,omitempty"`
	Status    string    `json:"status"`
}

func TestFormatAny_Nil(t *testing.T) {
	got := FormatAny(nil)
	if got != "null" {
		t.Errorf("FormatAny(nil) = %q; want %q", got, "null")
	}
}

func TestFormatAny_NilPointer(t *testing.T) {
	var p *sampleEntry
	got := FormatAny(p)
	if got != "null" {
		t.Errorf("FormatAny(nil ptr) = %q; want %q", got, "null")
	}
}

func TestFormatAny_EmptySlice(t *testing.T) {
	got := FormatAny([]sampleEntry{})
	if got != "results[0]{}:" {
		t.Errorf("FormatAny(empty slice) = %q; want %q", got, "results[0]{}:")
	}
}

func TestFormatAny_StructSlice(t *testing.T) {
	entries := []sampleEntry{
		{ID: "a", Name: "Alpha", Score: 0.95},
		{ID: "b", Name: "Beta", Score: 0.80},
	}
	got := FormatAny(entries)
	expected := "results[2]{id|name|score}:\n  a|Alpha|0.95\n  b|Beta|0.80"
	if got != expected {
		t.Errorf("FormatAny(struct slice) =\n%s\nwant:\n%s", got, expected)
	}
}

func TestFormatAny_StringSlice(t *testing.T) {
	items := []string{"foo", "bar", "baz"}
	got := FormatAny(items)
	expected := "items[3]:\n  foo\n  bar\n  baz"
	if got != expected {
		t.Errorf("FormatAny(string slice) =\n%s\nwant:\n%s", got, expected)
	}
}

func TestFormatAny_SingleStruct(t *testing.T) {
	s := sampleStatus{
		Running: true,
		PID:     1234,
		Status:  "active",
	}
	got := FormatAny(s)
	if got == "" {
		t.Error("FormatAny(struct) returned empty")
	}
	for _, want := range []string{"running:true", "pid:1234", "status:active"} {
		if !strings.Contains(got, want) {
			t.Errorf("FormatAny(struct) = %q; missing %q", got, want)
		}
	}
}

func TestFormatAny_Pointer(t *testing.T) {
	s := &sampleEntry{ID: "x", Name: "Test", Score: 1.0}
	got := FormatAny(s)
	if got == "null" || got == "" {
		t.Errorf("FormatAny(pointer) = %q; expected non-empty", got)
	}
}

func TestFormatAny_Map(t *testing.T) {
	m := map[string]string{"key1": "val1", "key2": "val2"}
	got := FormatAny(m)
	expected := "{key1:val1|key2:val2}"
	if got != expected {
		t.Errorf("FormatAny(map) = %q; want %q", got, expected)
	}
}

func TestFormatAny_PipeInString(t *testing.T) {
	entries := []sampleEntry{
		{ID: "a", Name: "Alpha|Beta", Score: 1.0},
	}
	got := FormatAny(entries)
	if !strings.Contains(got, `Alpha\|Beta`) {
		t.Errorf("expected pipe escaped as \\|, got: %s", got)
	}
}

func TestFormatAny_NewlineInString(t *testing.T) {
	type withBody struct {
		Name string `json:"name"`
		Body string `json:"body"`
	}
	s := withBody{Name: "doc", Body: "line1\nline2"}
	got := FormatAny(s)
	if !strings.Contains(got, `line1\nline2`) {
		t.Errorf("expected newline escaped as \\n, got: %s", got)
	}
}



func TestFormatAny_TimeField(t *testing.T) {
	ts := time.Date(2025, 6, 15, 12, 0, 0, 0, time.UTC)
	s := sampleStatus{
		Running:   true,
		PID:       42,
		StartedAt: ts,
		Status:    "ok",
	}
	got := FormatAny(s)
	if !strings.Contains(got, "2025-06-15T12:00:00Z") {
		t.Errorf("FormatAny should format time as RFC3339, got: %s", got)
	}
}

func TestFormatAny_NestedSliceField(t *testing.T) {
	type withTags struct {
		Name string   `json:"name"`
		Tags []string `json:"tags"`
	}
	s := withTags{Name: "test", Tags: []string{"a", "b"}}
	got := FormatAny(s)
	if !strings.Contains(got, "[a,b]") {
		t.Errorf("FormatAny should format slice fields as [a,b], got: %s", got)
	}
}

func TestFormatAny_EmptyStringSlice(t *testing.T) {
	got := FormatAny([]string{})
	if got != "results[0]{}:" {
		t.Errorf("FormatAny(empty string slice) = %q; want %q", got, "results[0]{}:")
	}
}

func TestJsonFieldName_FallbackToLowerCase(t *testing.T) {
	type noTag struct {
		MyField string
	}
	fields := structFields(reflect.TypeOf(noTag{}))
	if len(fields) != 1 {
		t.Fatalf("expected 1 field, got %d", len(fields))
	}
	name := jsonFieldName(fields[0])
	if name != "myfield" {
		t.Errorf("jsonFieldName fallback = %q; want %q", name, "myfield")
	}
}

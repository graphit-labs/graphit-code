package ast

import (
	"strings"
	"testing"
)

func TestContentNamedEntitiesGetDistinctUIDsEvenWithIdenticalText(t *testing.T) {
	pd := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	pf := parseFixture(t, pd, "main.go", `package main

// TODO: fix this
func a() {}

// TODO: fix this
func b() {}
`)

	entry := ConvertToCache(pf, pd, false, "")

	var comments []cachedEntity
	for _, e := range entry.Entities {
		if e.Label == LabelComment {
			comments = append(comments, e)
		}
	}
	if len(comments) != 2 {
		t.Fatalf("expected 2 distinct Comment entities, got %d: %+v", len(comments), comments)
	}
	if comments[0].UID == comments[1].UID {
		t.Errorf("two comments with identical text got the SAME uid (%q) — the second is indistinguishable from the first and would be merged instead of kept as its own node", comments[0].UID)
	}
	for _, c := range comments {
		if strings.Contains(c.UID, "\n") {
			t.Errorf("comment uid embeds a newline: %q", c.UID)
		}
		if strings.Contains(c.UID, c.Name) {
			t.Errorf("comment uid embeds the comment's own text verbatim: uid=%q name=%q", c.UID, c.Name)
		}
	}
}

// The uid must stay short and bounded regardless of how long the underlying comment
// or string literal is — proportional-to-content uids are a Parquet primary-key sort
// key, a Cypher literal, and a Go map key downstream, none of which should scale with
// source text size.
func TestContentNamedEntityUIDDoesNotScaleWithContentLength(t *testing.T) {
	longComment := "// " + strings.Repeat("this line explains something at length. ", 60)
	pd := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	pf := parseFixture(t, pd, "main.go", longComment+`
func a() {}
`)

	entry := ConvertToCache(pf, pd, false, "")

	var found bool
	for _, e := range entry.Entities {
		if e.Label != LabelComment {
			continue
		}
		found = true
		if len(e.Name) < 500 {
			t.Fatalf("test fixture comment is not actually long: %d bytes", len(e.Name))
		}
		if len(e.UID) > 100 {
			t.Errorf("uid grew with comment length: %d bytes (comment itself is %d bytes): %q", len(e.UID), len(e.Name), e.UID)
		}
	}
	if !found {
		t.Fatal("no Comment entity extracted")
	}
}

func TestContentNamedUIDIsStableAcrossRuns(t *testing.T) {
	if got, want := contentNamedUID("pkg/x.go", "comments", 3), contentNamedUID("pkg/x.go", "comments", 3); got != want {
		t.Errorf("contentNamedUID is not deterministic: %q != %q", got, want)
	}
	if contentNamedUID("pkg/x.go", "comments", 3) == contentNamedUID("pkg/x.go", "comments", 4) {
		t.Error("two different indices produced the same uid")
	}
}

package ast

import (
	"context"
	"errors"
	"reflect"
	"testing"
)

func TestBackendSelectionMechanism(t *testing.T) {
	parseFailure := errors.New("synthetic selected grammar failure")
	for _, tc := range []struct {
		name, grammar, wantParser string
		scip, overridden          bool
		selectedError             error
	}{
		{name: "SCIP success", scip: true, wantParser: "scip"},
		{name: "SCIP precedes explicit grammar", scip: true, overridden: true, grammar: "antlr-fixture", wantParser: "scip"},
		{name: "missing SCIP uses default", wantParser: "tree-sitter"},
		{name: "missing SCIP uses explicit ANTLR", overridden: true, grammar: "antlr-fixture", wantParser: "antlr4"},
		{name: "explicit grammar failure stays an error", overridden: true, grammar: "antlr-fixture", selectedError: parseFailure},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var scip *ParsedFile
			if tc.scip {
				scip = &ParsedFile{Parser: "scip"}
			}
			selected, defaultCalls := 0, 0
			pf, err := parseBackendSelection(scip, tc.grammar, tc.overridden,
				func(grammar string) (*ParsedFile, error) {
					selected++
					if grammar != tc.grammar {
						t.Fatalf("selected grammar=%q want=%q", grammar, tc.grammar)
					}
					if tc.selectedError != nil {
						return nil, tc.selectedError
					}
					return &ParsedFile{Parser: "antlr4"}, nil
				},
				func() (*ParsedFile, error) { defaultCalls++; return &ParsedFile{Parser: "tree-sitter"}, nil })
			wantSelected, wantDefault := 0, 0
			if !tc.scip {
				if tc.overridden {
					wantSelected = 1
				} else {
					wantDefault = 1
				}
			}
			if selected != wantSelected || defaultCalls != wantDefault {
				t.Fatalf("selected/default calls=%d/%d want=%d/%d", selected, defaultCalls, wantSelected, wantDefault)
			}
			if !errors.Is(err, tc.selectedError) {
				t.Fatalf("error=%v want=%v", err, tc.selectedError)
			}
			if tc.wantParser != "" && (pf == nil || pf.Parser != tc.wantParser) {
				t.Fatalf("parser=%+v want=%s", pf, tc.wantParser)
			}
		})
	}
}

func TestSyntaxParserFallbackMechanism(t *testing.T) {
	parseFailure := errors.New("synthetic parser failure")
	for _, tc := range []struct {
		name                        string
		ts, antlr, empty, nilResult bool
		tsError, wantError          error
		wantCalls                   []string
		wantParser                  string
	}{
		{name: "primary success", ts: true, antlr: true, wantCalls: []string{"tree-sitter"}, wantParser: "tree-sitter"},
		{name: "primary error", ts: true, antlr: true, tsError: parseFailure, wantCalls: []string{"tree-sitter", "antlr4"}, wantParser: "antlr4"},
		{name: "primary timeout", ts: true, antlr: true, tsError: context.DeadlineExceeded, wantCalls: []string{"tree-sitter", "antlr4"}, wantParser: "antlr4"},
		{name: "primary empty", ts: true, antlr: true, empty: true, wantCalls: []string{"tree-sitter", "antlr4"}, wantParser: "antlr4"},
		{name: "primary absent result", ts: true, antlr: true, nilResult: true, wantCalls: []string{"tree-sitter", "antlr4"}, wantParser: "antlr4"},
		{name: "operation cancelled", ts: true, antlr: true, tsError: context.Canceled, wantError: context.Canceled, wantCalls: []string{"tree-sitter"}},
		{name: "primary only", ts: true, wantCalls: []string{"tree-sitter"}, wantParser: "tree-sitter"},
		{name: "primary only error", ts: true, tsError: parseFailure, wantError: parseFailure, wantCalls: []string{"tree-sitter"}},
		{name: "secondary only", antlr: true, wantCalls: []string{"antlr4"}, wantParser: "antlr4"},
		{name: "no syntax parser"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var calls []string
			file := func() *ParsedFile {
				return &ParsedFile{Entities: map[string][]Entity{"Symbol": {{Name: "synthetic"}}}}
			}
			pf, err := parseSyntaxFallback(".fixture", tc.ts, tc.antlr,
				func() (*ParsedFile, error) {
					calls = append(calls, "tree-sitter")
					if tc.tsError != nil || tc.nilResult {
						return nil, tc.tsError
					}
					if tc.empty {
						return &ParsedFile{}, nil
					}
					return file(), nil
				},
				func() (*ParsedFile, error) { calls = append(calls, "antlr4"); return file(), nil })
			if !reflect.DeepEqual(calls, tc.wantCalls) {
				t.Fatalf("dispatch calls=%v want=%v", calls, tc.wantCalls)
			}
			if !tc.ts && !tc.antlr {
				if err == nil || pf != nil {
					t.Fatal("framework reported success without a parser")
				}
				return
			}
			if !errors.Is(err, tc.wantError) {
				t.Fatalf("error=%v want=%v", err, tc.wantError)
			}
			if tc.wantParser != "" && (pf == nil || pf.Parser != tc.wantParser) {
				t.Fatalf("parser result=%+v want=%s", pf, tc.wantParser)
			}
		})
	}
}

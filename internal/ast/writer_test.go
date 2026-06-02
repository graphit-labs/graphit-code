package ast

import (
	"testing"
)

// ---------------------------------------------------------------------------
// entityUID
// ---------------------------------------------------------------------------

func TestEntityUID(t *testing.T) {
	tests := []struct {
		name    string
		relFile string
		entName string
		ctxName string
		want    string
	}{
		{
			name:    "no_context",
			relFile: "cmd/main.go",
			entName: "Run",
			ctxName: "",
			want:    "cmd/main.go::Run",
		},
		{
			name:    "with_context",
			relFile: "internal/ast/writer.go",
			entName: "Write",
			ctxName: "GraphWriter",
			want:    "internal/ast/writer.go::GraphWriter.Write",
		},
		{
			name:    "empty_name",
			relFile: "a.go",
			entName: "",
			ctxName: "",
			want:    "a.go::",
		},
		{
			name:    "empty_file",
			relFile: "",
			entName: "Func",
			ctxName: "",
			want:    "::Func",
		},
		{
			name:    "nested_context",
			relFile: "pkg/utils.py",
			entName: "helper",
			ctxName: "MyClass",
			want:    "pkg/utils.py::MyClass.helper",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := entityUID(tt.relFile, tt.entName, tt.ctxName)
			if got != tt.want {
				t.Errorf("entityUID(%q, %q, %q) = %q, want %q",
					tt.relFile, tt.entName, tt.ctxName, got, tt.want)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// contextTypeToLabel
// ---------------------------------------------------------------------------

func TestContextTypeToLabel(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"", ""},
		{"class", "Class"},
		{"function", "Function"},
		{"package", "Package"},
		{"procedure", "Procedure"},
		{"struct", "Struct"},
		{"interface", "Interface"},
		{"enum", "Enum"},
		{"namespace", "Namespace"},
		{"trigger", "Trigger"},
		{"table", "Table"},
		{"view", "View"},
		{"type", "Type"},
		{"sequence", "Sequence"},
		{"synonym", "Synonym"},
		{"index", "Index"},
		// Case insensitivity
		{"CLASS", "Class"},
		{"Function", "Function"},
		{"PACKAGE", "Package"},
		// Unknown context type → title-cased
		{"widget", "Widget"},
		{"myType", "MyType"},
	}
	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got := contextTypeToLabel(tt.input)
			if got != tt.want {
				t.Errorf("contextTypeToLabel(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// canonicalModuleName
// ---------------------------------------------------------------------------

func TestCanonicalModuleName(t *testing.T) {
	tests := []struct {
		name       string
		importPath string
		filePath   string
		want       string
	}{
		{
			name:       "absolute_module",
			importPath: "react",
			filePath:   "src/App.tsx",
			want:       "react",
		},
		{
			name:       "relative_same_dir",
			importPath: "'./utils'",
			filePath:   "src/components/Button.tsx",
			want:       "src/components/utils",
		},
		{
			name:       "relative_parent_dir",
			importPath: "\"../helpers/format\"",
			filePath:   "src/components/Button.tsx",
			want:       "src/helpers/format",
		},
		{
			name:       "strip_js_extension",
			importPath: "./helper.js",
			filePath:   "src/index.js",
			want:       "src/helper",
		},
		{
			name:       "strip_ts_extension",
			importPath: "./types.ts",
			filePath:   "src/main.ts",
			want:       "src/types",
		},
		{
			name:       "strip_tsx_extension",
			importPath: "./App.tsx",
			filePath:   "src/index.tsx",
			want:       "src/App",
		},
		{
			name:       "strip_jsx_extension",
			importPath: "./Component.jsx",
			filePath:   "src/index.jsx",
			want:       "src/Component",
		},
		{
			name:       "strip_mjs_extension",
			importPath: "./mod.mjs",
			filePath:   "src/entry.mjs",
			want:       "src/mod",
		},
		{
			name:       "absolute_with_slash",
			importPath: "@angular/core",
			filePath:   "src/app.ts",
			want:       "@angular/core",
		},
		{
			name:       "quoted_absolute",
			importPath: "'lodash'",
			filePath:   "index.js",
			want:       "lodash",
		},
		{
			name:       "deep_relative",
			importPath: "'../../lib/utils'",
			filePath:   "src/features/auth/login.ts",
			want:       "src/lib/utils",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := canonicalModuleName(tt.importPath, tt.filePath)
			if got != tt.want {
				t.Errorf("canonicalModuleName(%q, %q) = %q, want %q",
					tt.importPath, tt.filePath, got, tt.want)
			}
		})
	}
}

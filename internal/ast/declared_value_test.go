package ast

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

// A query whose pattern does not compile is dropped with a log warning and
// nothing else: the entities it was meant to produce simply never appear, and no
// test fails. Every shipped pattern is compiled here against its own grammar so
// that a typo is a build failure instead of a silently emptier graph.
func TestEveryShippedQueryPatternCompiles(t *testing.T) {
	files, err := filepath.Glob(filepath.Join("queries", "*.yaml"))
	if err != nil || len(files) == 0 {
		t.Fatalf("no query files: %v", err)
	}

	checked, skipped := 0, 0
	for _, path := range files {
		body, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		qf, ok := parseQueryFile(body, path)
		if !ok {
			t.Errorf("%s: rejected by the loader", filepath.Base(path))
			continue
		}
		if qf.Parser == "antlr4" {
			// XPath patterns, not S-expressions — a different compiler.
			continue
		}
		grammar := qf.Grammar
		if grammar == "" {
			grammar = "tree-sitter-" + qf.Language
		}
		lang, err := resolveTreeSitterLang(qf.Language, grammar)
		if err != nil || lang == nil {
			skipped++
			t.Logf("%s: grammar %s unavailable, skipped", filepath.Base(path), grammar)
			continue
		}
		for i, q := range qf.Queries {
			compiled, qErr := sitter.NewQuery(lang, q.Pattern)
			if qErr != nil {
				t.Errorf("%s query %d (data_key=%s): pattern does not compile: %v\n  %s",
					filepath.Base(path), i, q.DataKey, qErr, q.Pattern)
				continue
			}
			checked++
			// A capture named in the YAML but absent from the pattern is the
			// same failure mode: the field is honoured and finds nothing.
			for label, name := range map[string]string{
				"name_capture":   q.NameCapture,
				"value_capture":  q.ValueCapture,
				"parent_capture": q.ParentCapture,
			} {
				if name == "" {
					continue
				}
				if _, found := compiled.CaptureIndexForName(name); !found {
					// name_capture defaults to "name"; a pattern that captures
					// nothing at all is a relation-only or predicate-only query
					// elsewhere, so only complain when the field was explicit.
					if label == "name_capture" && name == "name" {
						continue
					}
					t.Errorf("%s query %d (data_key=%s): %s=%q is not a capture of the pattern\n  %s",
						filepath.Base(path), i, q.DataKey, label, name, q.Pattern)
				}
			}
		}
	}
	t.Logf("compiled %d patterns; %d grammars unavailable", checked, skipped)
	if checked == 0 {
		t.Fatal("no pattern was compiled — the grammars did not resolve at all")
	}
}

// Declared values are nodes in every language, not just in the data formats.
//
// A constant told you it existed and nothing else: `const Endpoint =
// "https://api.acme.com/v2"` produced a Constant node named `Endpoint`, and the
// URL was nowhere — not a node, not a property, not in the search index. Asking
// "who holds this endpoint / this magic number / this feature flag" had no
// answer. Only literals are captured: an arbitrary expression is not a name.
func TestDeclaredValuesBecomeNodes(t *testing.T) {
	cases := []struct {
		name, lang, grammar, ext, queryFile, file, source string
		// want maps a declared name to the value node it must contain.
		want map[string]string
	}{
		{
			name: "go", lang: "go", grammar: "tree-sitter-go", ext: ".go",
			queryFile: "go.yaml", file: "a.go",
			source: `package p

const MaxRetries = 3
const Endpoint = "https://api.acme.com/v2"

var debug = false
`,
			want: map[string]string{
				"MaxRetries": "3",
				"Endpoint":   "https://api.acme.com/v2",
				"debug":      "false",
			},
		},
		{
			name: "python", lang: "python", grammar: "tree-sitter-python", ext: ".py",
			queryFile: "python.yaml", file: "a.py",
			source: `MAX_RETRIES = 3
ENDPOINT = "https://api.acme.com/v2"
DEBUG = False
`,
			want: map[string]string{
				"MAX_RETRIES": "3",
				"ENDPOINT":    "https://api.acme.com/v2",
				"DEBUG":       "False",
			},
		},
		{
			name: "typescript", lang: "typescript", grammar: "tree-sitter-typescript", ext: ".ts",
			queryFile: "typescript.yaml", file: "a.ts",
			source: `const ENDPOINT = "https://api.acme.com/v2";
let retries = 3;
enum Status { Active = "active" }
class Cfg { host: string = "db.example.com"; }
`,
			want: map[string]string{
				"ENDPOINT": "https://api.acme.com/v2",
				"retries":  "3",
				"Active":   "active",
				"host":     "db.example.com",
			},
		},
		{
			name: "javascript", lang: "javascript", grammar: "tree-sitter-javascript", ext: ".js",
			queryFile: "javascript.yaml", file: "a.js",
			source: `const ENDPOINT = "https://api.acme.com/v2";
var retries = 3;
`,
			want: map[string]string{"ENDPOINT": "https://api.acme.com/v2", "retries": "3"},
		},
		{
			name: "java", lang: "java", grammar: "tree-sitter-java", ext: ".java",
			queryFile: "java.yaml", file: "A.java",
			source: `class Cfg {
  static final int MAX_RETRIES = 3;
  private String host = "db.example.com";
}
`,
			want: map[string]string{"MAX_RETRIES": "3", "host": "db.example.com"},
		},
		{
			name: "csharp", lang: "c_sharp", grammar: "tree-sitter-c-sharp", ext: ".cs",
			queryFile: "csharp.yaml", file: "a.cs",
			source: `class Cfg {
  const int MaxRetries = 3;
  private string host = "db.example.com";
  public int Port { get; set; } = 5432;
}
enum Status { Active = 1 }
`,
			want: map[string]string{
				"MaxRetries": "3", "host": "db.example.com",
				"Port": "5432", "Active": "1",
			},
		},
		{
			name: "rust", lang: "rust", grammar: "tree-sitter-rust", ext: ".rs",
			queryFile: "rust.yaml", file: "a.rs",
			source: `const MAX_RETRIES: u32 = 3;
static ENDPOINT: &str = "https://api.acme.com/v2";
`,
			want: map[string]string{"MAX_RETRIES": "3", "ENDPOINT": "https://api.acme.com/v2"},
		},
		{
			name: "ruby", lang: "ruby", grammar: "tree-sitter-ruby", ext: ".rb",
			queryFile: "ruby.yaml", file: "a.rb",
			source: `MAX_RETRIES = 3
ENDPOINT = "https://api.acme.com/v2"
`,
			want: map[string]string{"MAX_RETRIES": "3", "ENDPOINT": "https://api.acme.com/v2"},
		},
		{
			name: "php", lang: "php", grammar: "tree-sitter-php", ext: ".php",
			queryFile: "php.yaml", file: "a.php",
			source: `<?php
const MAX_RETRIES = 3;
class Cfg { const HOST = "db.example.com"; public $port = 5432; }
`,
			want: map[string]string{"MAX_RETRIES": "3", "HOST": "db.example.com", "port": "5432"},
		},
		{
			name: "bash", lang: "bash", grammar: "tree-sitter-bash", ext: ".sh",
			queryFile: "bash.yaml", file: "a.sh",
			source: `MAX_RETRIES=3
ENDPOINT="https://api.acme.com/v2"
`,
			want: map[string]string{"MAX_RETRIES": "3", "ENDPOINT": "https://api.acme.com/v2"},
		},
		{
			name: "c", lang: "c", grammar: "tree-sitter-c", ext: ".c",
			queryFile: "c.yaml", file: "a.c",
			source: `const int MAX_RETRIES = 3;
const char *ENDPOINT = "https://api.acme.com/v2";
enum Status { Active = 1 };
`,
			want: map[string]string{
				"MAX_RETRIES": "3", "ENDPOINT": "https://api.acme.com/v2", "Active": "1",
			},
		},
		{
			name: "cpp", lang: "cpp", grammar: "tree-sitter-cpp", ext: ".cpp",
			queryFile: "cpp.yaml", file: "a.cpp",
			source: `constexpr int kMaxRetries = 3;
const char* kEndpoint = "https://api.acme.com/v2";
class Cfg { int port = 5432; };
enum Status { Active = 1 };
`,
			want: map[string]string{
				"kMaxRetries": "3", "kEndpoint": "https://api.acme.com/v2",
				"port": "5432", "Active": "1",
			},
		},
		{
			name: "kotlin", lang: "kotlin", grammar: "tree-sitter-kotlin", ext: ".kt",
			queryFile: "kotlin.yaml", file: "a.kt",
			source: `const val MAX_RETRIES = 3
val ENDPOINT = "https://api.acme.com/v2"
`,
			want: map[string]string{"MAX_RETRIES": "3", "ENDPOINT": "https://api.acme.com/v2"},
		},
		{
			name: "swift", lang: "swift", grammar: "tree-sitter-swift", ext: ".swift",
			queryFile: "swift.yaml", file: "a.swift",
			source: `let maxRetries = 3
let endpoint = "https://api.acme.com/v2"
`,
			want: map[string]string{"maxRetries": "3", "endpoint": "https://api.acme.com/v2"},
		},
		{
			name: "scala", lang: "scala", grammar: "tree-sitter-scala", ext: ".scala",
			queryFile: "scala.yaml", file: "a.scala",
			source: `val maxRetries = 3
val endpoint = "https://api.acme.com/v2"
class Cfg(val port: Int = 5432)
`,
			want: map[string]string{
				"maxRetries": "3", "endpoint": "https://api.acme.com/v2", "port": "5432",
			},
		},
		{
			name: "dart", lang: "dart", grammar: "tree-sitter-dart", ext: ".dart",
			queryFile: "dart.yaml", file: "a.dart",
			source: `const maxRetries = 3;
const endpoint = "https://api.acme.com/v2";
class Cfg { int port = 5432; }
`,
			want: map[string]string{
				"maxRetries": "3", "endpoint": "https://api.acme.com/v2", "port": "5432",
			},
		},
		{
			name: "lua", lang: "lua", grammar: "tree-sitter-lua", ext: ".lua",
			queryFile: "lua.yaml", file: "a.lua",
			source: `MAX_RETRIES = 3
ENDPOINT = "https://api.acme.com/v2"
local cfg = { port = 5432 }
`,
			want: map[string]string{
				"MAX_RETRIES": "3", "ENDPOINT": "https://api.acme.com/v2", "port": "5432",
			},
		},
		{
			name: "zig", lang: "zig", grammar: "tree-sitter-zig", ext: ".zig",
			queryFile: "zig.yaml", file: "a.zig",
			source: `const max_retries = 3;
const endpoint = "https://api.acme.com/v2";
`,
			want: map[string]string{"max_retries": "3", "endpoint": "https://api.acme.com/v2"},
		},
		{
			name: "r", lang: "r", grammar: "tree-sitter-r", ext: ".R",
			queryFile: "r.yaml", file: "a.R",
			source: `max_retries <- 3
endpoint <- "https://api.acme.com/v2"
`,
			want: map[string]string{"max_retries": "3", "endpoint": "https://api.acme.com/v2"},
		},
		{
			name: "julia", lang: "julia", grammar: "tree-sitter-julia", ext: ".jl",
			queryFile: "julia.yaml", file: "a.jl",
			source: `const MAX_RETRIES = 3
endpoint = "https://api.acme.com/v2"
`,
			want: map[string]string{"MAX_RETRIES": "3", "endpoint": "https://api.acme.com/v2"},
		},
		{
			name: "groovy", lang: "groovy", grammar: "tree-sitter-groovy", ext: ".groovy",
			queryFile: "groovy.yaml", file: "a.groovy",
			source: `def maxRetries = 3
def endpoint = "https://api.acme.com/v2"
`,
			want: map[string]string{"maxRetries": "3", "endpoint": "https://api.acme.com/v2"},
		},
		{
			name: "dockerfile", lang: "dockerfile", grammar: "tree-sitter-dockerfile", ext: ".dockerfile",
			queryFile: "dockerfile.yaml", file: "a.dockerfile",
			source: `FROM alpine:3.19 AS base
ENV APP_ENV=production
ARG VERSION=2.1.0
LABEL maintainer="acme@example.com"
`,
			want: map[string]string{
				"APP_ENV": "production", "VERSION": "2.1.0", "maintainer": "acme@example.com",
			},
		},
		{
			name: "protobuf", lang: "protobuf", grammar: "tree-sitter-proto", ext: ".proto",
			queryFile: "protobuf.yaml", file: "a.proto",
			source: `syntax = "proto3";
enum Status { ACTIVE = 7; }
message Order { string id = 4; }
`,
			// The wire contract: a field's number and an enum's value are how two
			// sides of an RPC agree, and neither was indexed.
			want: map[string]string{"ACTIVE": "7", "id": "4"},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			projectDir := stageGrammar(t, tc.lang, tc.grammar, tc.ext, tc.queryFile)
			pf := parseFixture(t, projectDir, tc.file, tc.source)

			// key name -> the value nodes contained by it
			contained := map[string][]string{}
			valueOf := map[string]string{}
			for _, ents := range pf.Entities {
				for _, e := range ents {
					if e.GraphLabel == "Value" && e.Context != "" {
						contained[e.Context] = append(contained[e.Context], e.Name)
					}
					if v := e.Properties["value"]; v != "" {
						valueOf[e.Name] = v
					}
				}
			}

			for key, want := range tc.want {
				if !containsStr(contained[key], want) {
					t.Errorf("%s: no Value node %q under %q; got %v",
						tc.name, want, key, contained[key])
				}
				if got := valueOf[key]; got != want {
					t.Errorf("%s: %q carries value %q, want %q", tc.name, key, got, want)
				}
			}
		})
	}
}

func containsStr(hay []string, needle string) bool {
	for _, h := range hay {
		if h == needle {
			return true
		}
	}
	return false
}

// Values that a grammar leaves wrapped in delimiters are unwrapped, including
// the backticks Go and JavaScript use and Python's triple quotes. A cutset trim
// would eat a quote that belongs to the value, so the stripping is per pair.
func TestValueDelimitersAreStrippedPerPair(t *testing.T) {
	for _, tc := range []struct{ in, want string }{
		{`"prod"`, "prod"},
		{`'prod'`, "prod"},
		{"`raw`", "raw"},
		{`"""abc"""`, "abc"},
		{`"say 'hi'"`, `say 'hi'`},
		{`it's`, `it's`},
		{`"`, `"`},
		{`""`, ""},
	} {
		if got := dataText(tc.in); got != tc.want {
			t.Errorf("dataText(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}

// The remaining data-format gaps: arrays of non-strings, inline tables, flow
// sequences, CDATA. Each held content that was parsed and then dropped.
func TestDataFormatCollectionsAndCDATA(t *testing.T) {
	t.Run("json arrays of non-strings", func(t *testing.T) {
		projectDir := stageGrammar(t, "json", "tree-sitter-json", ".json", "json.yaml")
		got := nodesOf(parseFixture(t, projectDir, "a.json",
			`{"ports": [80, 443], "flags": [true]}`))
		wantNode(t, got, "Value", "80", "ports", "Pair")
		wantNode(t, got, "Value", "443", "ports", "Pair")
		wantNode(t, got, "Value", "true", "flags", "Pair")
	})

	t.Run("yaml flow sequences", func(t *testing.T) {
		projectDir := stageGrammar(t, "yaml_lang", "tree-sitter-yaml", ".yaml", "yaml_lang.yaml")
		got := nodesOf(parseFixture(t, projectDir, "a.yaml", "ports: [80, 443]\n"))
		wantNode(t, got, "Value", "80", "ports", "Mapping")
		wantNode(t, got, "Value", "443", "ports", "Mapping")
	})

	t.Run("toml inline tables and numeric arrays", func(t *testing.T) {
		projectDir := stageGrammar(t, "toml", "tree-sitter-toml", ".toml", "toml.yaml")
		got := nodesOf(parseFixture(t, projectDir, "a.toml",
			"ports = [80, 443]\ninline = { host = \"db.example.com\" }\n"))
		wantNode(t, got, "Value", "80", "ports", "Pair")
		// A member of an inline table is a pair like any other, value included.
		wantNode(t, got, "Pair", "host", "", "")
		wantNode(t, got, "Value", "db.example.com", "host", "Pair")
	})

	t.Run("xml cdata", func(t *testing.T) {
		projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
		got := nodesOf(parseFixture(t, projectDir, "a.xml",
			`<q><sql><![CDATA[select 1 from dual]]></sql></q>`))
		wantNode(t, got, "Text", "select 1 from dual", "sql", "Element")
	})

	t.Run("hcl lists and objects", func(t *testing.T) {
		projectDir := stageGrammar(t, "hcl", "tree-sitter-hcl", ".tf", "hcl.yaml")
		got := nodesOf(parseFixture(t, projectDir, "a.tf", `resource "aws_db" "x" {
  ports = [80, 443]
  tags = { Name = "prod" }
}
`))
		wantNode(t, got, "Value", "443", "ports", "Attribute")
		wantNode(t, got, "Attribute", "Name", "tags", "Attribute")
		wantNode(t, got, "Value", "prod", "Name", "Attribute")
	})
}

// Enum members were missing entirely in several languages: the enum existed, its
// members did not, so neither the member nor its value could be found.
func TestEnumMembersAreNodes(t *testing.T) {
	for _, tc := range []struct {
		name, lang, grammar, ext, queryFile, file, source, member string
	}{
		{
			name: "typescript", lang: "typescript", grammar: "tree-sitter-typescript",
			ext: ".ts", queryFile: "typescript.yaml", file: "a.ts",
			source: "enum Status { Active = \"active\", Bare }\n", member: "Bare",
		},
		{
			name: "kotlin", lang: "kotlin", grammar: "tree-sitter-kotlin",
			ext: ".kt", queryFile: "kotlin.yaml", file: "a.kt",
			source: "enum class Status { Active }\n", member: "Active",
		},
		{
			name: "csharp", lang: "c_sharp", grammar: "tree-sitter-c-sharp",
			ext: ".cs", queryFile: "csharp.yaml", file: "a.cs",
			source: "enum Status { Active = 1, Bare }\n", member: "Bare",
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			projectDir := stageGrammar(t, tc.lang, tc.grammar, tc.ext, tc.queryFile)
			got := nodesOf(parseFixture(t, projectDir, tc.file, tc.source))
			if !got[[2]string{"EnumMember", tc.member}].present {
				var have []string
				for k := range got {
					if k[0] == "EnumMember" {
						have = append(have, k[1])
					}
				}
				t.Errorf("no EnumMember named %q; got %v", tc.member, strings.Join(have, ","))
			}
		})
	}
}

// Constant→Value is a label pair the CONTAINS rel table group has never had to
// carry. The group is built from whatever the cache holds, and LadybugDB rejects
// the whole statement when it names a node table that does not exist, so a new
// pair is worth proving against a real database rather than by analogy.
func TestDeclaredValueGraphIsQueryable(t *testing.T) {
	proj := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	pf := parseFixture(t, proj, "cfg.go", `package cfg

const Endpoint = "https://reporting-db.acme.com/v2"
const MaxRetries = 3
`)
	entry := ConvertToCache(pf, proj, true, "")
	if entry == nil {
		t.Fatal("nothing cached")
	}

	cacheDir := t.TempDir()
	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	defer func() { _ = cache.Close() }()
	if err := cache.Store(entry.RelPath, "h-"+entry.RelPath, entry); err != nil {
		t.Fatalf("store: %v", err)
	}

	dbPath := filepath.Join(t.TempDir(), "ladybugdb")
	writer := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := writer.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	ctx := context.Background()
	if err := RebuildFromJSON(ctx, writer, cache, nil, "", proj, nil); err != nil {
		_ = writer.Close()
		t.Fatalf("rebuild: %v", err)
	}
	_ = writer.Close()

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := db.connect(); err != nil {
		t.Fatalf("reopen after swap: %v", err)
	}
	defer func() { _ = db.Close() }()

	rows, err := db.Query(ctx,
		"MATCH (c:`Constant`)-[:CONTAINS]->(v:`Value`) RETURN c.name AS k, v.name AS v ORDER BY k", nil)
	if err != nil {
		t.Fatalf("query: %v", err)
	}
	if len(rows.Records) != 2 {
		t.Fatalf("got %d Constant→Value edges, want 2: %v", len(rows.Records), rows.Records)
	}
	want := map[string]string{
		"Endpoint":   "https://reporting-db.acme.com/v2",
		"MaxRetries": "3",
	}
	for _, rec := range rows.Records {
		k, _ := rec["k"].(string)
		if got := rec["v"]; got != want[k] {
			t.Errorf("%s → %v, want %q", k, got, want[k])
		}
	}

	// And the value is on the constant too, so the one-node answer works.
	rows, err = db.Query(ctx,
		"MATCH (c:`Constant`) WHERE c.name = 'Endpoint' RETURN c.value AS v", nil)
	if err != nil {
		t.Fatalf("query value property: %v", err)
	}
	if len(rows.Records) == 0 || rows.Records[0]["v"] != want["Endpoint"] {
		t.Errorf("Constant Endpoint has value %v, want %q", rows.Records, want["Endpoint"])
	}
}

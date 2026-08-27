package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Which labels get a vector is declared per grammar, in `embed_labels`. That
// makes it extensible — a grammar the binary has never seen still decides for
// itself — and it makes it silently skippable, which is what these two tests are
// for.
//
// The failure mode has no symptom at the point of the mistake: a grammar that
// says nothing, or names a label with a typo, still parses, still indexes, still
// answers keyword search. Only semantic search goes quiet for it, and a quiet
// half of a hybrid search reads exactly like a corpus that had no good match.

func shippedQueryFiles(t *testing.T) []ExternalQueryFile {
	t.Helper()

	paths, err := filepath.Glob(filepath.Join("queries", "*.yaml"))
	if err != nil || len(paths) == 0 {
		t.Fatalf("no query files: %v", err)
	}

	files := make([]ExternalQueryFile, 0, len(paths))
	for _, path := range paths {
		body, readErr := os.ReadFile(path)
		if readErr != nil {
			t.Fatalf("read %s: %v", path, readErr)
		}
		qf, ok := parseQueryFile(body, path)
		if !ok {
			t.Errorf("%s: rejected by the loader", filepath.Base(path))
			continue
		}
		files = append(files, qf)
	}
	return files
}

func TestEveryShippedGrammarDeclaresEmbedLabels(t *testing.T) {
	for _, qf := range shippedQueryFiles(t) {
		if len(qf.EmbedLabels) == 0 {
			t.Errorf("%s: declares no embed_labels, so nothing it indexes is ever "+
				"embedded and semantic search cannot reach this language. Add the "+
				"labels worth finding by meaning — Comment included, since a "+
				"comment's name is its prose.", qf.Language)
		}
	}
}

// TestEmbedLabelsAreLabelsTheGrammarProduces catches the typo. A label that no
// query of this grammar can emit matches no entity, so declaring it is a no-op
// that looks like a declaration.
func TestEmbedLabelsAreLabelsTheGrammarProduces(t *testing.T) {
	for _, qf := range shippedQueryFiles(t) {
		produced := map[string]bool{}
		for _, q := range qf.Queries {
			for _, l := range []string{q.GraphLabel, q.ValueLabel, q.ParentLabel} {
				if l != "" {
					produced[l] = true
				}
			}
		}
		// Comments are emitted by the engine's own pass rather than by a query,
		// so the grammar declaring comment_types is what makes Comment reachable.
		if len(qf.CommentTypes) > 0 {
			produced[LabelComment] = true
		}

		for _, want := range qf.EmbedLabels {
			if !produced[want] {
				t.Errorf("%s: embed_labels names %q, which no query of this grammar "+
					"emits (and comment_types does not cover). It will match nothing. "+
					"Produced here: %s",
					qf.Language, want, strings.Join(sortedKeys(produced), ", "))
			}
		}
	}
}

// embedLabelsTestLang is a language no shipped grammar declares, so a project
// file for it exercises the resolution rather than overriding a runtime answer.
const embedLabelsTestLang = "embedlabelslang"

// stageEmbedLabelsIn writes a project-level query file declaring embed_labels for
// embedLabelsTestLang under projectDir.
//
// Every embedder test stages its own grammar this way rather than naming a real
// language, because the shipped YAMLs a test process can see are the ones the
// launcher extracted to the runtime directory — which is machine state, not
// repository state. A test asserting on `go` would be asserting on whatever
// version of go.yaml this machine last installed. See the memory on query YAMLs
// not being go:embed.
func stageEmbedLabelsIn(t *testing.T, projectDir string, labels ...string) {
	t.Helper()

	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}

	yaml := "language: " + embedLabelsTestLang + "\n" +
		"grammar: tree-sitter-go\n" +
		"extensions: [\".embedlabels\"]\n" +
		"queries:\n" +
		"  - data_key: functions\n" +
		"    graph_label: Function\n" +
		"    pattern: '(function_declaration name: (identifier) @name)'\n" +
		"comment_types: [comment]\n" +
		"embed_labels:\n"
	for _, l := range labels {
		yaml += "  - " + l + "\n"
	}

	if err := os.WriteFile(filepath.Join(qdir, embedLabelsTestLang+".yaml"), []byte(yaml), 0o644); err != nil {
		t.Fatal(err)
	}
}

// stageEmbedLabelsGrammar is stageEmbedLabelsIn into a fresh project directory.
func stageEmbedLabelsGrammar(t *testing.T, labels ...string) string {
	t.Helper()
	projectDir := t.TempDir()
	stageEmbedLabelsIn(t, projectDir, labels...)
	return projectDir
}

func sortedKeys(m map[string]bool) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	// sort.Strings via the package's own helper keeps the message stable.
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j] < out[j-1]; j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out
}

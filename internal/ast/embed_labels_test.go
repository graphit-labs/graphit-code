package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const embedLabelsTestLang = "embedlabelslang"

func shippedQueryFiles(t *testing.T) []ExternalQueryFile {
	t.Helper()

	paths, err := filepath.Glob(filepath.Join("queries", "*.yaml"))
	if err != nil || len(paths) == 0 {
		t.Fatalf("query files: %v", err)
	}

	files := make([]ExternalQueryFile, 0, len(paths))
	for _, path := range paths {
		body, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		queryFile, ok := parseQueryFile(body, path)
		if !ok {
			t.Errorf("parse %s", filepath.Base(path))
			continue
		}
		files = append(files, queryFile)
	}
	return files
}

func TestShippedGrammarEmbedLabelsAreReachable(t *testing.T) {
	for _, queryFile := range shippedQueryFiles(t) {
		if len(queryFile.EmbedLabels) == 0 {
			t.Errorf("%s declares no embed labels", queryFile.Language)
			continue
		}

		produced := map[string]bool{}
		for _, query := range queryFile.Queries {
			for _, label := range []string{query.GraphLabel, query.ValueLabel, query.ParentLabel} {
				if label != "" {
					produced[label] = true
				}
			}
		}
		if len(queryFile.CommentTypes) > 0 {
			produced[LabelComment] = true
		}

		for _, label := range queryFile.EmbedLabels {
			if !produced[label] {
				t.Errorf("%s embeds %q, produced labels: %s",
					queryFile.Language, label, strings.Join(sortedKeys(produced), ", "))
			}
		}
	}
}

func stageEmbedLabelsIn(t *testing.T, projectDir string, labels ...string) {
	t.Helper()

	queryDir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(queryDir, 0o755); err != nil {
		t.Fatal(err)
	}

	body := "language: " + embedLabelsTestLang + "\n" +
		"grammar: tree-sitter-go\n" +
		"extensions: [\".embedlabels\"]\n" +
		"queries:\n" +
		"  - data_key: functions\n" +
		"    graph_label: Function\n" +
		"    pattern: '(function_declaration name: (identifier) @name)'\n" +
		"comment_types: [comment]\n" +
		"embed_labels:\n"
	for _, label := range labels {
		body += "  - " + label + "\n"
	}

	if err := os.WriteFile(filepath.Join(queryDir, embedLabelsTestLang+".yaml"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func stageEmbedLabelsGrammar(t *testing.T, labels ...string) string {
	t.Helper()
	projectDir := t.TempDir()
	stageEmbedLabelsIn(t, projectDir, labels...)
	return projectDir
}

func sortedKeys(values map[string]bool) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	for i := 1; i < len(keys); i++ {
		for j := i; j > 0 && keys[j] < keys[j-1]; j-- {
			keys[j], keys[j-1] = keys[j-1], keys[j]
		}
	}
	return keys
}

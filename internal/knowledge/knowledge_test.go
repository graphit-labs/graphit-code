package knowledge

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestKnowledgePathsAndIgnore(t *testing.T) {
	tempHome, err := os.MkdirTemp("", "graphit-knowledge-test-*")
	if err != nil {
		t.Fatalf("failed to create temp home: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempHome) }()

	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// 1. WikiDir checks
	projWiki := WikiDir()
	expectedProjWiki := filepath.Join(brand.DotDir(), "knowledge", "project")
	if projWiki != expectedProjWiki {
		t.Errorf("expected %s, got %s", expectedProjWiki, projWiki)
	}

	wikiCtxProj := WikiDirForContext("")
	if wikiCtxProj != projWiki {
		t.Errorf("expected Project Wiki, got %s", wikiCtxProj)
	}

	wikiCtxOther := WikiDirForContext("context-abc")
	expectedOther := filepath.Join(tempHome, "."+brand.Brand, "knowledge", "context-abc")
	if wikiCtxOther != expectedOther {
		t.Errorf("expected %s, got %s", expectedOther, wikiCtxOther)
	}

	// 2. EnsureContextCopy
	EnsureContextCopy("context-abc")
	linkDir := filepath.Join(brand.DotDir(), "knowledge", "context-abc")
	info, err := os.Lstat(linkDir)
	if err != nil {
		t.Errorf("expected directory at %s, got error: %v", linkDir, err)
	} else if info.Mode()&os.ModeSymlink != 0 {
		t.Errorf("expected real directory at %s, got symlink", linkDir)
	} else if !info.IsDir() {
		t.Errorf("expected directory at %s, got file", linkDir)
	}

	// 3. NewKnowledgeIgnoreChecker
	checker := NewKnowledgeIgnoreChecker(tempHome)
	if checker == nil {
		t.Fatal("expected non-nil IgnoreChecker")
	}
	if !checker.IsIgnored("node_modules/some-file.js", false) {
		t.Error("expected node_modules file to be ignored")
	}
}

func TestAutoLinking(t *testing.T) {
	titlesMap := map[string]string{
		"Daemon Service": "Daemon_Service",
		"AST Indexer":    "AST_Indexer",
	}

	// 1. Basic auto-linking
	content := "This is a document about Daemon Service and how AST Indexer behaves."
	linked, refs := autoLinkContent(content, titlesMap, "Some_Other_Page")
	expectedLinked := "This is a document about [[Daemon_Service|Daemon Service]] and how [[AST_Indexer|AST Indexer]] behaves."
	if linked != expectedLinked {
		t.Errorf("expected %q, got %q", expectedLinked, linked)
	}
	if len(refs) != 2 || refs[0] != "AST_Indexer" || refs[1] != "Daemon_Service" {
		t.Errorf("unexpected refs: %v", refs)
	}

	// 2. Do not link self
	contentSelf := "Daemon Service talks to AST Indexer."
	linkedSelf, refsSelf := autoLinkContent(contentSelf, titlesMap, "Daemon_Service")
	expectedSelf := "Daemon Service talks to [[AST_Indexer|AST Indexer]]."
	if linkedSelf != expectedSelf {
		t.Errorf("expected %q, got %q", expectedSelf, linkedSelf)
	}
	if len(refsSelf) != 1 || refsSelf[0] != "AST_Indexer" {
		t.Errorf("unexpected refsSelf: %v", refsSelf)
	}

	// 3. Ignore code blocks and inline code and existing links
	contentIgnored := "Use `Daemon Service` and block:\n```go\nvar d = Daemon Service\n```\nAnd existing link [[Daemon_Service|Daemon Service]]."
	linkedIgnored, _ := autoLinkContent(contentIgnored, titlesMap, "Some_Other_Page")
	if linkedIgnored != contentIgnored {
		t.Errorf("expected no auto-linking for code blocks or existing links, got %q", linkedIgnored)
	}
}

func TestSplitDocByHeaders(t *testing.T) {
	longContent := strings.Repeat("word ", 160)
	doc := knowledgeDoc{
		title:   "Test Document",
		docType: "guide",
		path:    "test.md",
		body: `---
title: Test Document
---
This is the parent introduction.

## Section One
` + longContent + `

## Section Two
Short section content.

## Empty Section

`,
	}

	splits := splitDocByHeaders(doc)
	// We expect 2 documents: the parent (which keeps Section Two and Empty Section) and Section One (which is split because it is long).
	if len(splits) != 2 {
		t.Fatalf("expected 2 split docs, got %d", len(splits))
	}

	parent := splits[0]
	if parent.title != "Test Document" {
		t.Errorf("expected parent title Test Document, got %q", parent.title)
	}
	if !strings.Contains(parent.body, "## Section One\nSee: [[Test Document - Section One]]") {
		t.Errorf("parent body missing link to Section One: %q", parent.body)
	}
	if !strings.Contains(parent.body, "## Section Two\nShort section content.") {
		t.Errorf("parent body should retain Section Two inline: %q", parent.body)
	}
	if !strings.Contains(parent.body, "## Empty Section") {
		t.Errorf("parent body should retain Empty Section: %q", parent.body)
	}

	s1 := splits[1]
	if s1.title != "Test Document - Section One" {
		t.Errorf("unexpected title for section one: %q", s1.title)
	}
	if s1.body != strings.TrimSpace(longContent) {
		t.Errorf("unexpected body for section one: %q", s1.body)
	}
	if s1.parentTitle != "Test Document" {
		t.Errorf("expected parentTitle Test Document, got %q", s1.parentTitle)
	}
}

func TestResolveWikiLinksInBody(t *testing.T) {
	titlesMap := map[string]string{
		"Test Document":              "Test_Document",
		"Test Document - Section One": "Test_Document_-_Section_One",
	}

	// 1. Exact matches and labels
	body := "Read [[Test Document - Section One]] or look at [[Test Document|Custom Label]]."
	resolved := resolveWikiLinksInBody(body, titlesMap)
	expected := "Read [[Test_Document_-_Section_One]] or look at [[Test_Document|Custom Label]]."
	if resolved != expected {
		t.Errorf("expected %q, got %q", expected, resolved)
	}

	// 2. Case-insensitive matches
	bodyCI := "Read [[test document - section one]] or [[TEST DOCUMENT]]."
	resolvedCI := resolveWikiLinksInBody(bodyCI, titlesMap)
	expectedCI := "Read [[Test_Document_-_Section_One]] or [[Test_Document]]."
	if resolvedCI != expectedCI {
		t.Errorf("expected %q, got %q", expectedCI, resolvedCI)
	}

	// 3. Trigram fuzzy matches (typos)
	bodyFuzzy := "Read [[Test Docment]] or [[Test Document - Sectin One]]."
	resolvedFuzzy := resolveWikiLinksInBody(bodyFuzzy, titlesMap)
	expectedFuzzy := "Read [[Test_Document]] or [[Test_Document_-_Section_One]]."
	if resolvedFuzzy != expectedFuzzy {
		t.Errorf("expected %q, got %q", expectedFuzzy, resolvedFuzzy)
	}
}

func TestSafeFilenameEmojis(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"AI Engine Specification - 🔌 Embedding Backends", "AI_Engine_Specification_-_Embedding_Backends"},
		{"AST Module Specification - 🗄️ Database Architecture - LadybugDB", "AST_Module_Specification_-_Database_Architecture_-_LadybugDB"},
		{"🤖 AI-Agent Self-Discovery Loop", "AI-Agent_Self-Discovery_Loop"},
		{"Simple Title", "Simple_Title"},
		{"Title/With/Slashes", "Title-With-Slashes"},
		{"Some 🚀 emoji", "Some_emoji"},
	}

	for _, tc := range tests {
		got := safeFilename(tc.input)
		if got != tc.want {
			t.Errorf("safeFilename(%q) = %q; want %q", tc.input, got, tc.want)
		}
	}
}



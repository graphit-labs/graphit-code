package knowledge

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/wiki"

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

	// 1. Every wiki resolves into the global directory, never into the project.
	projectDir := t.TempDir()
	global := filepath.Join(tempHome, brand.DotDir())

	projWiki := WikiDirFor(projectDir)
	if !strings.HasPrefix(projWiki, global) {
		t.Errorf("project wiki %s is not under the global dir %s", projWiki, global)
	}
	if strings.HasPrefix(projWiki, projectDir) {
		t.Errorf("project wiki %s leaked into the project directory", projWiki)
	}

	if got := WikiDirForContextIn(projectDir, ""); got != projWiki {
		t.Errorf("empty context should resolve to the project wiki, got %s", got)
	}
	if got := WikiDirForContextIn(projectDir, "__project__"); got != projWiki {
		t.Errorf("__project__ should resolve to the project wiki, got %s", got)
	}

	wikiCtxOther := WikiDirForContextIn(projectDir, "context-abc")
	expectedOther := filepath.Join(global, "wiki", "knowledge", "context", "context-abc")
	if wikiCtxOther != expectedOther {
		t.Errorf("expected %s, got %s", expectedOther, wikiCtxOther)
	}

	// 2. There is ONE shape for a context now: every install path places the compiled
	// wiki AT the context directory. A `wiki/` subdirectory left inside it is part of
	// the payload, not an alternative location to be probed for — which is what this
	// used to have to do, because the branch and the artifact paths disagreed.
	sub := filepath.Join(wikiCtxOther, "wiki")
	if err := os.MkdirAll(sub, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(sub, "index.md"), []byte("# Wiki\n"), 0o644); err != nil {
		t.Fatalf("write index: %v", err)
	}
	if got := WikiDirForContextIn(projectDir, "context-abc"); got != wikiCtxOther {
		t.Errorf("expected the context directory %s, got %s", wikiCtxOther, got)
	}

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

	compiledTargets := wiki.BuildAutoLinkTargets(titlesMap)

	// 1. Basic auto-linking
	content := "This is a document about Daemon Service and how AST Indexer behaves."
	linked, refs := wiki.AutoLinkContent(content, compiledTargets, "Some_Other_Page")
	expectedLinked := "This is a document about [Daemon Service](Daemon_Service.md) and how [AST Indexer](AST_Indexer.md) behaves."
	if linked != expectedLinked {
		t.Errorf("expected %q, got %q", expectedLinked, linked)
	}
	if len(refs) != 2 || refs[0] != "AST_Indexer" || refs[1] != "Daemon_Service" {
		t.Errorf("unexpected refs: %v", refs)
	}

	// 2. Do not link self
	contentSelf := "Daemon Service talks to AST Indexer."
	linkedSelf, refsSelf := wiki.AutoLinkContent(contentSelf, compiledTargets, "Daemon_Service")
	expectedSelf := "Daemon Service talks to [AST Indexer](AST_Indexer.md)."
	if linkedSelf != expectedSelf {
		t.Errorf("expected %q, got %q", expectedSelf, linkedSelf)
	}
	if len(refsSelf) != 1 || refsSelf[0] != "AST_Indexer" {
		t.Errorf("unexpected refsSelf: %v", refsSelf)
	}

	// 3. Ignore code blocks and inline code and existing links
	contentIgnored := "Use `Daemon Service` and block:\n```go\nvar d = Daemon Service\n```\nAnd existing link [Daemon Service](Daemon_Service.md)."
	linkedIgnored, _ := wiki.AutoLinkContent(contentIgnored, compiledTargets, "Some_Other_Page")
	if linkedIgnored != contentIgnored {
		t.Errorf("expected no auto-linking for code blocks or existing links, got %q", linkedIgnored)
	}
}
func TestResolveWikiLinksInBody(t *testing.T) {
	titlesMap := map[string]string{
		"Test Document":               "Test_Document",
		"Test Document - Section One": "Test_Document_-_Section_One",
	}

	// 1. Exact matches and labels
	body := "Read [[Test Document - Section One]] or look at [[Test Document|Custom Label]]."
	resolved := wiki.ResolveWikiLinksInBody(body, titlesMap)
	expected := "Read [Test Document - Section One](Test_Document_-_Section_One.md) or look at [Custom Label](Test_Document.md)."
	if resolved != expected {
		t.Errorf("expected %q, got %q", expected, resolved)
	}

	// 2. Case-insensitive matches
	bodyCI := "Read [[test document - section one]] or [[TEST DOCUMENT]]."
	resolvedCI := wiki.ResolveWikiLinksInBody(bodyCI, titlesMap)
	expectedCI := "Read [test document - section one](Test_Document_-_Section_One.md) or [TEST DOCUMENT](Test_Document.md)."
	if resolvedCI != expectedCI {
		t.Errorf("expected %q, got %q", expectedCI, resolvedCI)
	}

	// 3. Trigram fuzzy matches (typos)
	bodyFuzzy := "Read [[Test Docment]] or [[Test Document - Sectin One]]."
	resolvedFuzzy := wiki.ResolveWikiLinksInBody(bodyFuzzy, titlesMap)
	expectedFuzzy := "Read [Test Docment](Test_Document.md) or [Test Document - Sectin One](Test_Document_-_Section_One.md)."
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
		got := wiki.SafeSlug(tc.input)
		if got != tc.want {
			t.Errorf("wiki.SafeSlug(%q) = %q; want %q", tc.input, got, tc.want)
		}
	}
}

package daemon

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Query files are re-read while the daemon runs. Grammar libraries are not, and
// cannot be: resolveTreeSitterLang memoises each language for the life of the
// process because a *sitter.Language backs live parse state, and swapping one
// under a parse in flight is not something a lock makes safe.
//
// So installing a grammar into a daemon that has been up for days used to do
// nothing at all until someone restarted it by hand. The daemon already exits
// for replacement when the launcher stamp moves; a new grammar now goes through
// that same door.

func withGrammarSigs(t *testing.T, seed map[string]string) *Daemon {
	t.Helper()
	d := &Daemon{grammarSigs: map[string]string{}}
	for k, v := range seed {
		d.grammarSigs[k] = v
	}
	return d
}

func installFakeGrammar(t *testing.T, projectDir, name string) {
	t.Helper()
	dir := filepath.Join(projectDir, brand.DotDir(), "grammars", "treesitter")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte("not really a library"), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestGrammarInstallTriggersReplacement(t *testing.T) {
	projectDir := t.TempDir()

	// The daemon has already accepted this project's grammar directory as it was.
	d := withGrammarSigs(t, map[string]string{
		"":         ast.GrammarSignature(""),
		projectDir: ast.GrammarSignature(projectDir),
	})
	d.supervisors = map[string]*ProjectSupervisor{
		"p": {projectDir: projectDir},
	}

	if _, changed := d.grammarsChanged(); changed {
		t.Fatal("nothing has changed yet")
	}

	installFakeGrammar(t, projectDir, "libtree-sitter-elm.so")

	where, changed := d.grammarsChanged()
	if !changed {
		t.Fatal("installing a grammar did not ask for a replacement — a long-lived " +
			"daemon would keep ignoring it")
	}
	if where != projectDir {
		t.Errorf("reported %q as the changed location, want %q", where, projectDir)
	}

	// The signature is updated when it fires, so it does not fire forever.
	if _, again := d.grammarsChanged(); again {
		t.Error("the same install asked for a second replacement")
	}
}

// A project discovered with grammars already in place is not a reason to
// restart; without this the daemon would bounce whenever a new project appeared.
func TestNewlyDiscoveredProjectDoesNotTriggerReplacement(t *testing.T) {
	projectDir := t.TempDir()
	installFakeGrammar(t, projectDir, "libtree-sitter-elm.so")

	d := withGrammarSigs(t, map[string]string{"": ast.GrammarSignature("")})
	d.supervisors = map[string]*ProjectSupervisor{
		"p": {projectDir: projectDir},
	}

	if _, changed := d.grammarsChanged(); changed {
		t.Error("a project that already had grammars when it was discovered forced a restart")
	}
	// Having been recorded, a later install in it must still fire.
	installFakeGrammar(t, projectDir, "libtree-sitter-nim.so")
	if _, changed := d.grammarsChanged(); !changed {
		t.Error("an install after discovery was missed")
	}
}

// Removing a grammar counts too: the process is still holding the old library.
func TestGrammarRemovalTriggersReplacement(t *testing.T) {
	projectDir := t.TempDir()
	installFakeGrammar(t, projectDir, "libtree-sitter-elm.so")

	d := withGrammarSigs(t, map[string]string{
		"":         ast.GrammarSignature(""),
		projectDir: ast.GrammarSignature(projectDir),
	})
	d.supervisors = map[string]*ProjectSupervisor{"p": {projectDir: projectDir}}

	if err := os.Remove(filepath.Join(projectDir, brand.DotDir(),
		"grammars", "treesitter", "libtree-sitter-elm.so")); err != nil {
		t.Fatal(err)
	}
	if _, changed := d.grammarsChanged(); !changed {
		t.Error("removing a grammar left the daemon running with it still loaded")
	}
}

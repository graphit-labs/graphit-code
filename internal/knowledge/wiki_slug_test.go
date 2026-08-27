package knowledge

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// TestPathDerivedSlugIsPlatformIndependent pins that the fallback slug for an
// ambiguous title is the same on Windows as on Unix.
//
// The slug is stored in xrefs and in every [[wikilink]] on disk, so a slug that
// changed with the path separator would repoint links for everyone who checked the
// repository out on the other platform.
func TestPathDerivedSlugIsPlatformIndependent(t *testing.T) {
	unix := "docs/specs/config_module.md"
	windows := `docs\specs\config_module.md`

	slugOf := func(p string) string {
		relSlash := filepath.ToSlash(p)
		return wiki.SafeSlug(strings.TrimSuffix(relSlash, ".md"))
	}

	got, want := slugOf(windows), slugOf(unix)
	if got != want {
		t.Errorf("slug differs by separator: %q vs %q", got, want)
	}
	if strings.ContainsAny(want, `/\`) {
		t.Errorf("slug must not carry a separator: %q", want)
	}
	if want != "docs-specs-config_module" {
		t.Errorf("unexpected slug %q", want)
	}
}

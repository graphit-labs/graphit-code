package wiki

import (
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// OKFVersion is the Open Knowledge Format revision these wikis target.
//
// Declared in one place because §12 permits it in exactly one place: the frontmatter of a
// bundle-root index.md, which is the only index.md allowed to carry frontmatter at all.
const OKFVersion = "0.2"

// OKFActor names the producer of a generated page in the actor convention of §7.
//
// §7 gives three forms: `<producer>/<version>` for an agent or tool, `human:<id>` for a
// person, `process:<id>` for an automated process. A wiki page is written by the indexing
// daemon with no human in the loop, so `process:` is the honest one — and it is also the
// stable one. Under `<producer>/<version>` every release would rewrite the `generated.by`
// line of every page in every wiki, and since a page is written only when its bytes change
// (see writePageIfChanged), that is a full rewrite of the corpus per upgrade for a field
// nobody queries by version.
//
// The prefix matters beyond taste: §5.3 derives the trust tier from whether an actor
// carries the `human:` prefix, so a generated page must not claim one.
func OKFActor(module string) string {
	return "process:" + brand.Brand + "-" + module + "-wiki"
}

// WriteOKFGenerated writes the trust family's `generated` field (§5.2).
//
// It is a MAPPING with a REQUIRED `by`, not a scalar. The pre-existing generator wrote a
// flat key literally named `generated.at:` — the spec's prose path notation transcribed as
// a YAML key — which no OKF consumer resolves and which our own frontmatter scanner could
// not even see, because its key regex stopped at the dot.
//
// `at` is passed through rather than taken from the clock: a page's generation stamp comes
// from its source's mtime so that regenerating an unchanged wiki produces byte-identical
// pages. ISO 8601 permits a date without a time, and a date is the real granularity here;
// padding it to midnight would assert a precision the input does not carry.
func WriteOKFGenerated(b *strings.Builder, actor, at string) {
	_, _ = fmt.Fprintf(b, "generated: { by: %s, at: %s }\n", actor, at)
}

// WriteOKFSources writes the provenance family's `sources` list (§5.1).
//
// Each entry is a mapping whose `resource` is REQUIRED — a path, a URL, or a scope
// descriptor. A bare string in the list is not an entry, which is what the first pass at
// OKF emitted.
func WriteOKFSources(b *strings.Builder, resources ...string) {
	var written bool
	for _, r := range resources {
		r = strings.TrimSpace(r)
		if r == "" {
			continue
		}
		if !written {
			b.WriteString("sources:\n")
			written = true
		}
		_, _ = fmt.Fprintf(b, "  - resource: %s\n", r)
	}
}

package wiki

import (
	"fmt"
	"strconv"
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

// YAMLScalar renders a free-text value as a YAML scalar that always parses.
//
// This is the first conformance criterion, not a nicety: §11 requires that every concept
// document contain a PARSEABLE YAML frontmatter block, and these blocks are assembled by
// string concatenation rather than by a YAML encoder. Free text walks straight into that.
// A description lifted from a source document's own folded scalar arrives as
// `> Where every compiled artifact lives`, and `description: > Where ...` is a block scalar
// header with trailing content — a parse error that takes the WHOLE frontmatter with it,
// including the `type` the spec requires. A title containing `: ` breaks it the same way.
//
// Values that are already unambiguous are left bare, because a wiki page is read by people
// as often as by parsers and quoting everything makes the block harder to scan. Anything
// that could be read as YAML syntax — an indicator character, a key separator, a comment,
// a bare boolean or number, leading or trailing space — is quoted and escaped.
func YAMLScalar(v string) string {
	if v == "" {
		return `""`
	}
	if needsYAMLQuoting(v) {
		return strconv.Quote(v)
	}
	return v
}

func needsYAMLQuoting(v string) bool {
	if strings.TrimSpace(v) != v {
		return true
	}
	if strings.ContainsAny(v, "\n\r\t") {
		return true
	}
	// Indicator characters are only special in the FIRST position of a plain scalar.
	if strings.ContainsRune(`-?:,[]{}#&*!|>'"%@`+"`", rune(v[0])) {
		return true
	}
	// A key separator or an inline comment anywhere ends the scalar early.
	if strings.Contains(v, ": ") || strings.HasSuffix(v, ":") || strings.Contains(v, " #") {
		return true
	}
	// A plain scalar that would be read as something other than a string.
	switch strings.ToLower(v) {
	case "true", "false", "yes", "no", "on", "off", "null", "~":
		return true
	}
	if _, err := strconv.ParseFloat(v, 64); err == nil {
		return true
	}
	return false
}

package ast

import (
	"bytes"
	"fmt"
	"log/slog"
	"sort"
	"strconv"
	"strings"
	"sync"
	"unicode/utf8"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

// Embedded language parsing: the body of a single-file component's <script> and
// <style>, parsed with the grammar of the language it is actually written in.
//
// tree-sitter-vue, tree-sitter-svelte and tree-sitter-html all hand that body over
// as a single `raw_text` node — not a limitation of our queries, it is what the
// grammars produce. Before this existed, a .vue contributed no IMPORTS edge, no
// entity from its script, and no export flag: `import { ref } from 'vue'` bought
// the file no dependency at all. The body was still findable as TEXT, because the
// whole file source is an indexed FTS5 column; what was missing was STRUCTURE.
//
// The mechanism is a second parse of the block's text with the inner language's
// config, followed by a merge into the outer result. Three things make that correct
// rather than nearly correct:
//
//   - THE LINE OFFSET. The inner parse sees only the block, so every line it
//     reports is relative to the block. The offset is the row the text node starts
//     on, applied once by shiftParsedLines. This is the part that fails silently,
//     which is why the tests put the script at the END of the file: with the script
//     first, an offset of zero passes.
//   - THE PATH. The inner parse is told the OUTER file's path, deliberately: the
//     IMPORTS edge has to leave the .vue, not a synthetic path nobody can open.
//   - THE SELECTOR IS A QUERY. A block is selected by a tree-sitter pattern, the
//     same language every `queries[].pattern` is written in. There is ONE mechanism:
//     the `<script>` of a Vue component and the `<execute>` of some project's XML go
//     down the same path, because "which node is this block" is the same question in
//     both and only a query answers it generally. A node kind cannot say "this
//     element and not its siblings", and hardcoded attribute kinds cannot read an
//     attribute in a grammar that spells them differently.
//
// The optional-attribute case — `<script lang="ts">` and `<script>` are both valid —
// is what makes ORDER part of the design. A block declares two patterns, specific
// first, and the FIRST block whose pattern matches a given body node claims it. The
// claim happens at the MATCH, not after the language resolves, which is what makes
// `lang="scss"` skip rather than fall through to the generic block's `default: css`
// and be parsed as CSS.

// maxEmbedDepth bounds the sub-parse.
//
// One level is all any real component needs — a <script> holds TypeScript, and
// TypeScript declares no embedded blocks. The bound exists because the config is
// declarative and nothing stops a language from naming ITSELF as the language of
// its own block, which without this recurses until the stack ends.
const maxEmbedDepth = 1

// embedWarnOnce keeps the misconfiguration warnings to one per language, not one
// per file. A project of 400 components would otherwise log 400 identical lines.
var embedWarnOnce sync.Map // map[string]struct{}

func embedWarn(key string, msg string, args ...any) {
	if _, loaded := embedWarnOnce.LoadOrStore(key, struct{}{}); loaded {
		return
	}
	slog.Warn(msg, args...)
}

// parseEmbedded runs every block's pattern over the tree, in declaration order, and
// folds each body it finds into out.
//
// Order is load-bearing, not incidental: `claimed` records the body nodes already
// taken, so a later block never revisits one an earlier block matched. That is how
// two patterns express one optional attribute — the specific one first, the generic
// one as the fallback — with a single mechanism instead of a special case.
//
// Failures are quiet by design, and there are two kinds. A block whose language is
// not one we ship — `lang="scss"`, `lang="stylus"` — is skipped in silence: that is
// the common case, and a warning per block is a log line per file. A block whose
// CONFIG cannot be right — a pattern that does not compile, a capture that is not in
// it — warns once, because that is a bug in a query file and nothing else reports it.
func (t *TreeSitterParser) parseEmbedded(path string, root *sitter.Node, src []byte,
	lang *sitter.Language, host *ExternalQueryFile, out *ParsedFile,
	lineOffset, embedDepth int, isDepend bool, opts ParseOptions) {

	if embedDepth >= maxEmbedDepth || root == nil || host == nil || len(host.Embedded) == 0 {
		return
	}
	claimed := make(map[uintptr]bool)
	for i := range host.Embedded {
		t.parseEmbeddedBlock(path, root, src, lang, host, &host.Embedded[i], out, claimed,
			lineOffset, embedDepth, isDepend, opts)
	}
}

// embeddedQueryCache holds the compiled pattern of a pattern-form block.
//
// Compilation is a cgo call, and a block is matched once per file: without this it
// would be recompiled for every file of the language. Keyed by the pattern text and
// the grammar, because the same pattern compiled against a different grammar is a
// different query.
var embeddedQueryCache sync.Map // map[string]*sitter.Query

// compileEmbeddedPattern compiles a block's pattern against a grammar, memoised.
func compileEmbeddedPattern(lang *sitter.Language, pattern string) (*sitter.Query, error) {
	if lang == nil {
		return nil, fmt.Errorf("no grammar")
	}
	key := fmt.Sprintf("%p|%s", lang, pattern)
	if q, ok := embeddedQueryCache.Load(key); ok {
		return q.(*sitter.Query), nil
	}
	q, err := sitter.NewQuery(lang, pattern)
	if err != nil {
		return nil, err
	}
	embeddedQueryCache.Store(key, q)
	return q, nil
}

// parseEmbeddedBlock runs one block's pattern over the tree.
//
// The pattern is the general selector, and the reason it is worth reusing rather
// than inventing one: `#eq?` and `#match?` on a captured tag name express "only
// <execute>" and "any tag matching ^sql", which a node kind cannot, and the language
// value is located by the pattern itself rather than by node kinds the engine
// hardcodes.
func (t *TreeSitterParser) parseEmbeddedBlock(path string, root *sitter.Node, src []byte,
	lang *sitter.Language, host *ExternalQueryFile, blk *EmbeddedBlock, out *ParsedFile,
	claimed map[uintptr]bool,
	lineOffset, embedDepth int, isDepend bool, opts ParseOptions) {

	q, err := compileEmbeddedPattern(lang, blk.Pattern)
	if err != nil {
		// A pattern that does not compile selects nothing, in silence. That is the
		// failure mode this warning exists to break.
		embedWarn("pattern|"+blk.Pattern, "embedded pattern does not compile",
			"pattern", blk.Pattern, "path", path, "error", err)
		return
	}
	textIdx := captureIndex(q, blk.TextCapture)
	if textIdx < 0 {
		embedWarn("textcap|"+blk.Pattern, "embedded text_capture is not in the pattern",
			"text_capture", blk.TextCapture, "pattern", blk.Pattern, "path", path)
		return
	}
	langIdx := captureIndex(q, blk.LangCapture)

	qc := queryCursorPool.Get().(*sitter.QueryCursor)
	defer queryCursorPool.Put(qc)
	matches := qc.Matches(q, root, src)
	for {
		if opts.Cancelled != nil && opts.Cancelled() {
			return
		}
		match := matches.Next()
		if match == nil {
			break
		}
		textNode := captureNodeAt(match, textIdx)
		if SafeIsNull(textNode) {
			continue
		}
		// The claim is taken HERE, on the match, and not after the language
		// resolves. A `<style lang="scss">` matches the specific block, which maps
		// no language for it and skips — and the claim is what stops the generic
		// block behind it from picking the same body up and parsing SCSS as CSS.
		if claimed[textNode.Id()] {
			continue
		}
		claimed[textNode.Id()] = true

		langValue := ""
		if langIdx >= 0 {
			langValue = strings.TrimSpace(captureTextAt(match, langIdx, src))
			// A grammar may or may not include the quotes in the value node —
			// tree-sitter-xml's AttValue spans them, tree-sitter-html's
			// attribute_value does not. dataText already knows this.
			if v := dataText(langValue); v != "" {
				langValue = v
			}
		}
		t.parseEmbeddedBody(path, textNode, src, host, blk, langValue, langIdx >= 0,
			out, lineOffset, embedDepth, isDepend, opts)
	}
}

// parseEmbeddedBody resolves the language for one matched body, sub-parses it, and
// merges the result in.
func (t *TreeSitterParser) parseEmbeddedBody(path string, textNode *sitter.Node, src []byte,
	host *ExternalQueryFile, blk *EmbeddedBlock, langValue string, langSelected bool,
	out *ParsedFile,
	lineOffset, embedDepth int, isDepend bool, opts ParseOptions) {

	innerLang := resolveEmbeddedLang(blk, langValue, langSelected)
	if innerLang == "" {
		return
	}
	lc, ok := embeddedLangConfig(t.projectDir, innerLang)
	if !ok {
		// The language is named but we do not ship it — `scss`, `stylus`. Skipping
		// matches how an extension with no grammar is skipped, and a project full of
		// `lang="scss"` must not produce a log line per file.
		return
	}

	body := src[textNode.StartByte():textNode.EndByte()]
	if blk.Normalize != "" && host != nil {
		if n, ok := host.TextNormalizers[blk.Normalize]; ok {
			body = applyTextNormalizer(body, &n)
		}
	}
	if len(strings.TrimSpace(string(body))) == 0 {
		return
	}
	// A fragment becomes a compilation unit. After normalising, because the wrapping is
	// syntax and the normaliser is about escaping; and neither side may contain a line
	// break, which the loader already enforced — so line 1 of the sub-parse is still the
	// block's first line. See EmbeddedBlock.WrapPrefix.
	if blk.WrapPrefix != "" || blk.WrapSuffix != "" {
		wrapped := make([]byte, 0, len(blk.WrapPrefix)+len(body)+len(blk.WrapSuffix))
		wrapped = append(wrapped, blk.WrapPrefix...)
		wrapped = append(wrapped, body...)
		wrapped = append(wrapped, blk.WrapSuffix...)
		body = wrapped
	}

	// The row the text node STARTS on is the offset, not the row after it: the text
	// begins immediately after the `>`, on that same row, so the block's own line 1
	// is that row. Line 1 of the inner parse plus that row is the absolute line.
	innerOffset := lineOffset + int(textNode.StartPosition().Row)

	innerOpts := opts
	// The outer parse already holds the whole file's source; a second copy of the
	// block's text would be stored nowhere and discarded immediately.
	innerOpts.IndexSource = false

	var inner *ParsedFile
	var err error
	switch {
	case lc.ts != nil:
		// The tree-sitter path applies the offset itself, inside parseSource, because
		// that is also where a nested embedded block is resolved.
		inner, err = t.parseSource(path, lc.ext, lc.ts, body,
			innerOffset, embedDepth+1, isDepend, innerOpts)
	case lc.antlr != nil:
		// The ANTLR backend was already source-based — parseWithConfig takes the
		// bytes and driver.Parse works on them — so embedding it needed no new
		// plumbing there. The offset is applied here instead of inside, because
		// shiftParsedLines works on a *ParsedFile and knows nothing about which
		// backend produced it. That is the whole reason it was written that way.
		inner, err = (&AntlrParser{projectDir: t.projectDir}).
			parseWithConfig(path, lc.ext, lc.antlr, body, isDepend, innerOpts)
		if err == nil {
			shiftParsedLines(inner, innerOffset)
		}
	}
	if err != nil || inner == nil {
		// A grammar that fails to load is a fact about the installation, not about
		// this file, so it is worth exactly one line per language.
		embedWarn("parse|"+innerLang, "embedded block parse failed",
			"language", innerLang, "pattern", blk.Pattern, "error", err)
		return
	}
	// Before the merge, while the block's own position is still in hand.
	//
	// The block declares which of the host's entities are units (`host_labels`); with
	// none declared the host is whatever strictly contains the block.
	//
	// The block's FIRST LINE is innerOffset + 1, not innerOffset: the offset is what a
	// 1-based line of the sub-parse is shifted by, so it names the line BEFORE the
	// block. Passing it as the block's position asked "what is around the line above
	// this block", and in indented XML the answer is the preceding sibling — the
	// `<key>` of the `<entry>` whose `<value>` carries the statement.
	attributeToHostEntity(out, inner,
		innerOffset+1, lineOffset+int(textNode.EndPosition().Row)+1, blk.HostLabels)
	mergeParsedInto(out, inner)
}

// attributeToHostEntity makes the host's own entity the source of what an embedded
// block does, instead of the file.
//
// A statement inside an embedded block has no enclosing entity IN ITS OWN LANGUAGE —
// a bare `INSERT INTO t …` in a config value is not inside a procedure — so it fell
// back to the file, and the graph could only say "this FILE writes to t". But the
// block is not floating in the file: it sits inside something the HOST grammar named,
// and that name is the answer to "which unit of this configuration touches that
// table".
//
// This is the half that makes a project's own grammar pay off. The engine deliberately
// knows nothing about what the host format models — a task, a step, a job, a handler
// are all just entities some grammar declared — so whatever a project names, it gets
// attributed. Without this the consumer can declare the perfect entity and never reach
// it, which is a generic apparatus that stops one step short of useful.
//
// Only items with NO source of their own are stamped: a block that really does declare
// a procedure keeps that procedure as the source of its statements.
func attributeToHostEntity(outer, inner *ParsedFile, blockFirstLine, blockLastLine int,
	hostLabels []string) {

	if outer == nil || inner == nil {
		return
	}
	host, hostLabel := hostEntityWithLabel(outer, blockFirstLine, blockLastLine, hostLabels)
	if host == "" {
		return
	}
	for i := range inner.References {
		if inner.References[i].SourceName == "" {
			inner.References[i].SourceName = host
		}
	}
	for i := range inner.CallSites {
		if inner.CallSites[i].SourceName == "" {
			inner.CallSites[i].SourceName = host
			// And the host's LABEL, or the edge is written from a caller of the inner
			// language's default label — a Function that does not exist — and dropped.
			inner.CallSites[i].SourceType = hostLabel
		}
	}
}

// hostEntityAt names the INNERMOST entity of pf that CONTAINS the whole block.
//
// Innermost — smallest span — because a document nests: the element carrying a value
// is inside the one describing a step, which is inside the one describing the flow.
// The outermost would make the root element the source of everything in the file,
// which is the File answer again with extra steps.
//
// CONTAINING the block, and STRICTLY — extending beyond it on at least one side — is
// what separates a host from a wrapper. Two failures come out of that one rule:
//
//   - An entity that ends INSIDE the block is not hosting it. In a data grammar an
//     entity's span runs from its name to the end of that name's parent — the START
//     TAG, for `(STag (Name) @name)` — so the element that literally holds the text
//     spans one line and is indistinguishable from the sibling above it.
//   - An entity whose span COINCIDES with the block is the thing carrying the block,
//     not a unit around it. A one-line `<value>select …</value>` is the case: the
//     element's tag and the statement share the only line there is, so containment
//     alone would pick it and the answer would be the word "value". Measured on a real
//     corpus: 3 of 28 statements in one flow are written on one line.
//
// What is left is whatever entity the grammar declared with a span wide enough to hold
// the block. That is what `span_capture` is for; without it, a data grammar has no host
// to offer and the source stays the file, which is the honest answer rather than a
// nearby node.
//
// Content-named labels are skipped on top of that. The block's text IS a Text/CharData
// node, so its span equals the block's and it would qualify, and attributing a
// statement to the text of that statement says nothing.
//
// A block that DECLARES its host labels replaces both filters with that list, and drops
// the strictness: those labels are the format's units by declaration, so an entity of one
// of them spanning exactly the block is the unit that holds it — which is what a block
// living in an attribute of its own unit's element looks like.
//
// Ties are broken by the later start and then by name, because Entities is a map and
// map order is not an order — without it the same file could attribute differently
// between two runs of the same binary.
func hostEntityAt(pf *ParsedFile, firstLine, lastLine int, hostLabels []string) string {
	name, _ := hostEntityWithLabel(pf, firstLine, lastLine, hostLabels)
	return name
}

// hostEntityWithLabel is hostEntityAt plus the host's graph label.
//
// The label is not decoration: a CALL carries its caller's label explicitly
// (`CallInfo.SourceType`, which becomes the FROM end of the CALLS pair in the schema),
// and it comes from the INNER language's context — empty for a bare statement, which
// then defaults to Function. Stamping the name without the label writes an edge from a
// Function that does not exist, and the writer drops it: the DML edge appeared and the
// CALLS edge did not, from the very same block. A reference needs no equivalent, because
// its source label is derived from the uid at rebuild time.
func hostEntityWithLabel(pf *ParsedFile, firstLine, lastLine int, hostLabels []string) (string, string) {
	dataKeys := make([]string, 0, len(pf.Entities))
	for k := range pf.Entities {
		dataKeys = append(dataKeys, k)
	}
	sort.Strings(dataKeys)

	declared := map[string]bool{}
	for _, l := range hostLabels {
		declared[l] = true
	}

	best, bestLabel, bestSpan, bestLine := "", "", 0, 0
	for _, dataKey := range dataKeys {
		for _, e := range pf.Entities[dataKey] {
			if e.Name == "" || e.Line > firstLine || e.EndLine < lastLine {
				continue
			}
			label := entityLabelOf(dataKey, e)
			if len(declared) > 0 {
				if !declared[label] {
					continue
				}
			} else {
				if e.Line == firstLine && e.EndLine == lastLine {
					continue
				}
				if contentNamedLabels[label] {
					continue
				}
			}
			span := e.EndLine - e.Line
			better := best == "" || span < bestSpan ||
				(span == bestSpan && (e.Line > bestLine ||
					(e.Line == bestLine && e.Name < best)))
			if better {
				best, bestLabel, bestSpan, bestLine = e.Name, label, span, e.Line
			}
		}
	}
	return best, bestLabel
}

// resolveEmbeddedLang decides which language a matched body is written in.
//
// Returns "" for "skip this body", which is what a value with no mapping means —
// `lang="scss"` on a block that maps no style language.
//
// langSelected says whether the block declares a `lang_capture` at all: a block
// without one is always Default, which is a different thing from one that declares a
// capture and found it empty.
func resolveEmbeddedLang(blk *EmbeddedBlock, value string, langSelected bool) string {
	if !langSelected || value == "" {
		return blk.Default
	}
	if name, ok := blk.Languages[strings.ToLower(value)]; ok {
		return name
	}
	// The author said which language this is and it is not one we map. Honouring
	// `default` here would parse SCSS as CSS and report entities from a grammar
	// that never saw the file's real syntax.
	return ""
}

// embeddedLang is a language resolved by name, on either backend.
//
// The config names a LANGUAGE; which backend parses it is the engine's problem.
// `plsql` (ANTLR) and `sql` (tree-sitter) are both valid answers for the same
// block, and whoever writes the YAML should not have to know the difference —
// especially since the dialect grammars are the ones that know what a `SELECT`
// reads, which is the whole point of embedding SQL in the first place.
type embeddedLang struct {
	ts    *tsLangConfig
	antlr *antlrLangConfig
	ext   string
}

// embeddedLangConfig resolves a language name across both backends, project files
// first and then the global tables — the same precedence as everywhere else.
//
// Tree-sitter wins a tie. Nothing ships under both names today, and if something
// ever did, the in-process backend is the cheaper one to reach for.
func embeddedLangConfig(projectDir, name string) (embeddedLang, bool) {
	name = strings.ToLower(strings.TrimSpace(name))
	if name == "" {
		return embeddedLang{}, false
	}
	if cfg, ok := tsLangConfigByName(projectDir, name); ok {
		return embeddedLang{ts: cfg, ext: primaryExtOf(cfg)}, true
	}
	if cfg, ok := antlrLangConfigByName(projectDir, name); ok {
		ext := ""
		if len(cfg.Extensions) > 0 {
			ext = strings.ToLower(cfg.Extensions[0])
		}
		return embeddedLang{antlr: cfg, ext: ext}, true
	}
	return embeddedLang{}, false
}

// antlrLangConfigByName resolves an ANTLR language by name. The counterpart of
// tsLangConfigByName: antlrExtMap answers "what parses .sql" and antlrGrammarMap
// "what is antlr-plsql", and neither answers "what is the language called plsql".
func antlrLangConfigByName(projectDir, name string) (*antlrLangConfig, bool) {
	name = strings.ToLower(strings.TrimSpace(name))
	if name == "" {
		return nil, false
	}
	if projectDir != "" {
		for _, qf := range effectiveProjectQueryFiles(projectDir) {
			if qf.Parser == "antlr4" && strings.EqualFold(qf.Language, name) {
				if !grammarEnabledIn(projectDir, qf.Language, effectiveGrammarName(qf)) {
					return nil, false
				}
				return antlrConfigOf(qf), true
			}
		}
	}
	extTablesMu.RLock()
	match, ok := antlrConfigByLanguage(name)
	extTablesMu.RUnlock()
	if !ok || !grammarEnabledIn(projectDir, match.Language, match.Grammar) {
		return nil, false
	}
	return match, true
}

// antlrConfigByLanguage scans the registered grammars for one language name. The
// caller holds extTablesMu.
func antlrConfigByLanguage(name string) (*antlrLangConfig, bool) {
	for _, cfg := range antlrGrammarMap {
		if strings.EqualFold(cfg.Language, name) {
			return cfg, true
		}
	}
	return nil, false
}

// shiftParsedLines moves every line-bearing record in pf by delta.
//
// One place, applied after every pass has run, rather than a `+ lineOffset` at each
// of the dozen sites that compute a line from a node position. The offset is the
// part of embedded parsing that fails silently — a wrong line is still a plausible
// line — so it is worth having exactly one implementation to test.
func shiftParsedLines(pf *ParsedFile, delta int) {
	if pf == nil || delta == 0 {
		return
	}
	for _, ents := range pf.Entities {
		for i := range ents {
			ents[i].Line += delta
			ents[i].EndLine += delta
		}
	}
	for i := range pf.CallSites {
		pf.CallSites[i].Line += delta
	}
	for i := range pf.References {
		pf.References[i].Line += delta
	}
	// Identity includes the line, so every position the index holds is now stale.
	// Dropping it makes the next AddOrMergeEntity rebuild.
	pf.mergeIdx = nil
}

// mergeParsedInto folds an embedded block's parse into the file's.
//
// Entities go through AddOrMergeEntity rather than AddEntity for the reason that
// function exists: the two parses can describe the same node. Keys are merged in
// sorted order because map iteration is not deterministic, and the order entities
// land in decides which row wins when ConvertToCache completes a repeated one.
//
// EVERYTHING THE INNER PARSE PRODUCED IS STAMPED WITH THE INNER LANGUAGE, and that
// is the whole reason this function takes it rather than only concatenating. The
// merge used to hand its output to the outer file unlabelled, so the language
// downstream was the HOST's: SQL embedded in XML arrived as `xml`. Two things then
// resolve against the wrong world:
//
//   - resolveNamed refuses to cross languages, on purpose — a `fill()` in .tsx must
//     not bind to the Go function of the same name. Under the host's language the
//     embedded SQL could never reach a table declared in a .sql file, because those
//     are `plsql` and it claimed to be `xml`.
//   - refRule picks the TargetRule by language, and the DML rules with `fallback:
//     Table` belong to the SQL grammars. Under `xml` there is no such rule, so an
//     unresolved target fell back to the file and the edge became File → File.
//
// A nested block keeps the innermost language: only an empty Lang is filled, so the
// stamp applied by a deeper merge survives this one.
func mergeParsedInto(outer, inner *ParsedFile) {
	if outer == nil || inner == nil {
		return
	}
	lang := inner.Language
	keys := make([]string, 0, len(inner.Entities))
	for k := range inner.Entities {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, dataKey := range keys {
		for _, e := range inner.Entities[dataKey] {
			e.Lang = langOr(e.Lang, lang)
			outer.AddOrMergeEntity(dataKey, e)
		}
	}
	for _, c := range inner.CallSites {
		c.Lang = langOr(c.Lang, lang)
		outer.CallSites = append(outer.CallSites, c)
	}
	for _, r := range inner.References {
		r.Lang = langOr(r.Lang, lang)
		outer.References = append(outer.References, r)
	}
	if inner.HasError {
		outer.HasError = true
	}
}

// langOr is "the language this thing was produced by, or the file's".
//
// Empty is the overwhelmingly common case — a file parsed by one grammar — so the
// per-item field stays unset except where an embedded parse filled it.
func langOr(itemLang, fileLang string) string {
	if itemLang != "" {
		return itemLang
	}
	return fileLang
}

// applyTextNormalizer turns a language's escaped text back into what it represents.
//
// The engine knows NO escaping scheme: the substitutions come from the host
// language's `text_normalizers`, because how a language escapes its text is a fact
// about that language. This function only applies what it was handed.
//
// THE INVARIANT: the number of newlines must not change. The load-time validator
// drops any replacement that contains a line break, and decodeNumericCharRef below
// refuses the two references that would produce one. Every line the sub-parse
// reports is shifted by the block's start row in the host file, so a normalizer that
// added a line would move every entity after it inside the block — a wrong line
// number instead of a visible syntax error.
func applyTextNormalizer(b []byte, n *TextNormalizer) []byte {
	if n == nil || len(b) == 0 {
		return b
	}
	// Longest key first, so a scheme declaring both `&amp;` and `&amp;amp;` resolves
	// the specific one. Map order is not an order.
	keys := make([]string, 0, len(n.Replace))
	for k := range n.Replace {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool {
		if len(keys[i]) != len(keys[j]) {
			return len(keys[i]) > len(keys[j])
		}
		return keys[i] < keys[j]
	})

	out := make([]byte, 0, len(b))
	for i := 0; i < len(b); {
		matched := false
		for _, k := range keys {
			if i+len(k) <= len(b) && string(b[i:i+len(k)]) == k {
				out = append(out, n.Replace[k]...)
				i += len(k)
				matched = true
				break
			}
		}
		if matched {
			continue
		}
		if n.NumericCharRefs && b[i] == '&' {
			if r, width, ok := decodeNumericCharRef(b[i:]); ok {
				out = utf8.AppendRune(out, r)
				i += width
				continue
			}
		}
		out = append(out, b[i])
		i++
	}
	return out
}

// decodeNumericCharRef reads `&#62;` or `&#x3E;` from the head of b, refusing
// anything that would become a line break — see the invariant above — and anything
// that is not a valid rune. Returns the rune and how many bytes it consumed.
func decodeNumericCharRef(b []byte) (rune, int, bool) {
	end := bytes.IndexByte(b, ';')
	// A reference is short. Without this bound, an ampersand used as an operator
	// would scan to the next semicolon anywhere in the statement.
	if end < 3 || end > 12 || b[1] != '#' {
		return 0, 0, false
	}
	digits, base := string(b[2:end]), 10
	if digits[0] == 'x' || digits[0] == 'X' {
		digits, base = digits[1:], 16
	}
	if digits == "" {
		return 0, 0, false
	}
	n, err := strconv.ParseInt(digits, base, 32)
	if err != nil || n <= 0 || n > utf8.MaxRune || n == '\n' || n == '\r' {
		return 0, 0, false
	}
	return rune(n), end + 1, true
}

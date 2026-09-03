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

const maxEmbedDepth = 1

var embedWarnOnce sync.Map

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

var embeddedQueryCache sync.Map

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

func (t *TreeSitterParser) parseEmbeddedBlock(path string, root *sitter.Node, src []byte,
	lang *sitter.Language, host *ExternalQueryFile, blk *EmbeddedBlock, out *ParsedFile,
	claimed map[uintptr]bool,
	lineOffset, embedDepth int, isDepend bool, opts ParseOptions) {

	q, err := compileEmbeddedPattern(lang, blk.Pattern)
	if err != nil {
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
		if claimed[textNode.Id()] {
			continue
		}
		claimed[textNode.Id()] = true

		langValue := ""
		if langIdx >= 0 {
			langValue = strings.TrimSpace(captureTextAt(match, langIdx, src))
			if v := dataText(langValue); v != "" {
				langValue = v
			}
		}
		t.parseEmbeddedBody(path, textNode, src, host, blk, langValue, langIdx >= 0,
			out, lineOffset, embedDepth, isDepend, opts)
	}
}

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
	if blk.WrapPrefix != "" || blk.WrapSuffix != "" {
		wrapped := make([]byte, 0, len(blk.WrapPrefix)+len(body)+len(blk.WrapSuffix))
		wrapped = append(wrapped, blk.WrapPrefix...)
		wrapped = append(wrapped, body...)
		wrapped = append(wrapped, blk.WrapSuffix...)
		body = wrapped
	}

	innerOffset := lineOffset + int(textNode.StartPosition().Row)

	innerOpts := opts
	innerOpts.IndexSource = false

	var inner *ParsedFile
	var err error
	switch {
	case lc.ts != nil:
		inner, err = t.parseSource(path, lc.ext, lc.ts, body,
			innerOffset, embedDepth+1, isDepend, innerOpts)
	case lc.antlr != nil:
		inner, err = (&AntlrParser{projectDir: t.projectDir}).
			parseWithConfig(path, lc.ext, lc.antlr, body, isDepend, innerOpts)
		if err == nil {
			shiftParsedLines(inner, innerOffset)
		}
	}
	if err != nil || inner == nil {
		embedWarn("parse|"+innerLang, "embedded block parse failed",
			"language", innerLang, "pattern", blk.Pattern, "error", err)
		return
	}
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
			inner.CallSites[i].SourceType = hostLabel
		}
	}
}

func hostEntityAt(pf *ParsedFile, firstLine, lastLine int, hostLabels []string) string {
	name, _ := hostEntityWithLabel(pf, firstLine, lastLine, hostLabels)
	return name
}

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

type embeddedLang struct {
	ts    *tsLangConfig
	antlr *antlrLangConfig
	ext   string
}

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

func antlrConfigByLanguage(name string) (*antlrLangConfig, bool) {
	for _, cfg := range antlrGrammarMap {
		if strings.EqualFold(cfg.Language, name) {
			return cfg, true
		}
	}
	return nil, false
}

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
	pf.mergeIdx = nil
}

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

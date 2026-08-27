package ast

import (
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
)

// grammarFilter is one project's answer to "may this language be used", built
// from ast.grammars_whitelist and ast.grammars_blacklist.
//
// See docs/specs/config_module.md for the keys and docs/specs/ast_module.md for
// where the filter is enforced.
type grammarFilter struct {
	whitelist map[string]bool
	blacklist map[string]bool
}

// inert reports that the filter allows everything, which is the default.
func (f grammarFilter) inert() bool {
	return len(f.whitelist) == 0 && len(f.blacklist) == 0
}

func (f grammarFilter) allows(language, grammar string) bool {
	if f.inert() {
		return true
	}
	aliases := grammarAliases(language, grammar)
	if len(f.whitelist) > 0 && !anyListed(f.whitelist, aliases) {
		return false
	}
	return !anyListed(f.blacklist, aliases)
}

func (f grammarFilter) allowsFile(qf ExternalQueryFile) bool {
	if f.inert() {
		return true
	}
	return f.allows(qf.Language, effectiveGrammarName(qf))
}

// keepFiles returns the files whose language is enabled. The input slice is
// returned untouched when nothing is filtered, which is the common case.
func (f grammarFilter) keepFiles(files []ExternalQueryFile) []ExternalQueryFile {
	if f.inert() || len(files) == 0 {
		return files
	}
	kept := make([]ExternalQueryFile, 0, len(files))
	for _, qf := range files {
		if f.allowsFile(qf) {
			kept = append(kept, qf)
		}
	}
	if len(kept) == len(files) {
		return files
	}
	return kept
}

// grammarAliases is every name a language answers to: its own name, its grammar
// name, and that grammar name without the backend prefix.
//
// The three differ often enough that matching only one of them would make the
// obvious entry do nothing — yaml_lang.yaml declares `language: yaml_lang` and
// `grammar: tree-sitter-yaml`, so "yaml" matches neither.
func grammarAliases(language, grammar string) []string {
	aliases := make([]string, 0, 3)
	if l := normalizeGrammarName(language); l != "" {
		aliases = append(aliases, l)
	}
	g := normalizeGrammarName(grammar)
	if g != "" && !containsString(aliases, g) {
		aliases = append(aliases, g)
	}
	if bare := stripGrammarPrefix(g); bare != "" && !containsString(aliases, bare) {
		aliases = append(aliases, bare)
	}
	return aliases
}

// effectiveGrammarName is the grammar a query file resolves to, applying the
// same default tsConfigOf and antlrConfigOf apply when `grammar:` is absent.
func effectiveGrammarName(qf ExternalQueryFile) string {
	if qf.Grammar != "" {
		return qf.Grammar
	}
	if qf.Language == "" {
		return ""
	}
	if qf.Parser == "antlr4" {
		return "antlr-" + qf.Language
	}
	return "tree-sitter-" + qf.Language
}

func stripGrammarPrefix(name string) string {
	for _, prefix := range []string{"tree-sitter-", "antlr-"} {
		if strings.HasPrefix(name, prefix) {
			return strings.TrimPrefix(name, prefix)
		}
	}
	return ""
}

func normalizeGrammarName(name string) string {
	return strings.ToLower(strings.TrimSpace(name))
}

func anyListed(set map[string]bool, names []string) bool {
	for _, n := range names {
		if set[n] {
			return true
		}
	}
	return false
}

func containsString(haystack []string, needle string) bool {
	for _, h := range haystack {
		if h == needle {
			return true
		}
	}
	return false
}

func parseGrammarList(val string) map[string]bool {
	if val == "" {
		return nil
	}
	set := make(map[string]bool)
	for _, entry := range strings.Split(val, ",") {
		if n := normalizeGrammarName(entry); n != "" {
			set[n] = true
		}
	}
	if len(set) == 0 {
		return nil
	}
	return set
}

// grammarFilterState is one project's cached filter plus the configuration it was
// built from.
//
// Deliberately the same shape as queryDirState: resolving the keys reads
// ~/.graphit/config.json from disk on every call, and this is consulted once per
// file discovered, so the re-resolve sits behind the same rate limit — which is
// also what makes a config change land on a running daemon without a restart.
type grammarFilterState struct {
	mu        sync.Mutex
	loaded    bool
	filter    grammarFilter
	signature string
	lastCheck time.Time
}

func (s *grammarFilterState) get(projectDir string) (grammarFilter, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now()
	if s.loaded && now.Sub(s.lastCheck) < queryStaleCheckInterval {
		return s.filter, false
	}
	s.lastCheck = now

	var projectCfg config.ConfigMap
	if projectDir != "" {
		projectCfg = config.LoadProjectConfig(projectDir)
	}
	white := config.ResolveASTGrammarsWhitelist(nil, projectCfg)
	black := config.ResolveASTGrammarsBlacklist(nil, projectCfg)

	sig := white + "\x00" + black
	if s.loaded && sig == s.signature {
		return s.filter, false
	}

	s.filter = grammarFilter{
		whitelist: parseGrammarList(white),
		blacklist: parseGrammarList(black),
	}
	s.signature, s.loaded = sig, true
	return s.filter, true
}

var grammarFilterStates sync.Map // map[string]*grammarFilterState

// grammarFilterFor returns the filter for one project, re-resolving the keys at
// most once per staleness interval.
//
// An empty projectDir is a supported key, not a mistake: it resolves the
// environment variable, the global config and the compiled defaults, which is the
// right answer for a caller with no project at hand.
func grammarFilterFor(projectDir string) grammarFilter {
	v, _ := grammarFilterStates.LoadOrStore(projectDir, &grammarFilterState{})
	filter, changed := v.(*grammarFilterState).get(projectDir)
	if changed {
		// Outside the state's own lock: invalidateDerivedQueryCaches rebuilds the
		// extension tables, and mergedQueryCache is derived from the filter.
		invalidateDerivedQueryCaches()
	}
	return filter
}

// grammarEnabledIn reports whether a language may be used in one project.
func grammarEnabledIn(projectDir, language, grammar string) bool {
	return grammarFilterFor(projectDir).allows(language, grammar)
}

// invalidateGrammarFilters forces the next lookup to re-resolve the keys, without
// waiting out the staleness interval.
func invalidateGrammarFilters() {
	grammarFilterStates.Range(func(_, v any) bool {
		st := v.(*grammarFilterState)
		st.mu.Lock()
		st.loaded = false
		st.mu.Unlock()
		return true
	})
}

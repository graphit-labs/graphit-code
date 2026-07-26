package ast

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

func DetectProjectConfig(ctx context.Context, db GraphDB, rootPath string) map[string]string {
	detected := make(map[string]string)

	for _, entry := range ResolveEcosystems(rootPath) {
		if entry.Glob || strings.Contains(entry.Filename, "*") {
			matches, _ := filepath.Glob(filepath.Join(rootPath, entry.Filename))
			if len(matches) > 0 {
				detected[entry.Ecosystem] = entry.Language
			}
			continue
		}
		path := filepath.Join(rootPath, entry.Filename)
		if _, err := os.Stat(path); err == nil {
			detected[entry.Ecosystem] = entry.Language
		}
	}

	goModPath := filepath.Join(rootPath, "go.mod")
	if data, err := os.ReadFile(goModPath); err == nil {
		for _, line := range strings.Split(string(data), "\n") {
			line = strings.TrimSpace(line)
			if strings.HasPrefix(line, "module ") {
				detected["go_module"] = strings.TrimPrefix(line, "module ")
				break
			}
		}
	}

	pkgPath := filepath.Join(rootPath, "package.json")
	if data, err := os.ReadFile(pkgPath); err == nil {
		var pkg struct {
			Name string `json:"name"`
			Main string `json:"main"`
		}
		if json.Unmarshal(data, &pkg) == nil && pkg.Name != "" {
			detected["npm_package"] = pkg.Name
			if pkg.Main != "" {
				detected["npm_main"] = pkg.Main
			}
		}
	}

	return detected
}

func DetectFrameworks(ctx context.Context, db GraphDB, projectDir string) map[string]string {
	detected := make(map[string]string)

	// Build lookup tables from YAML frameworks (4-level resolution chain)
	decoratorLookup := make(map[string][2]string) // decorator -> [framework, category]
	heritageLookup := make(map[string][2]string)  // parent -> [framework, category]
	type importEntry struct {
		framework string
		category  string
		match     string
		regex     *regexp.Regexp // compiled regex for match=regex
	}
	importLookup := make(map[string]importEntry) // pattern -> importEntry

	for _, fw := range ResolveFrameworks(projectDir) {
		fwName := fw.Framework
		for _, d := range fw.DecoratorDetection {
			name := d.FrameworkName
			if name == "" {
				name = fwName
			}
			decoratorLookup[d.Name] = [2]string{name, d.Category}
		}
		for _, h := range fw.HeritageDetection {
			name := h.FrameworkName
			if name == "" {
				name = fwName
			}
			heritageLookup[h.Parent] = [2]string{name, h.Category}
		}
		for _, imp := range fw.ImportDetection {
			name := imp.FrameworkName
			if name == "" {
				name = fwName
			}
			importLookup[imp.Pattern] = importEntry{
				framework: name,
				category:  imp.Category,
				match:     imp.Match,
				regex:     compileImportRegex(imp.Match, imp.Pattern),
			}
		}
	}

	// Also load import detection from language YAMLs (language-level rules)
	for _, langCfg := range ResolveAllLangConfigs(projectDir) {
		for _, imp := range langCfg.ImportDetection {
			// Only add if not already overridden by a framework file
			if _, exists := importLookup[imp.Pattern]; !exists {
				name := imp.FrameworkName
				if name == "" {
					name = langCfg.Language
				}
				importLookup[imp.Pattern] = importEntry{
					framework: name,
					category:  imp.Category,
					match:     imp.Match,
					regex:     compileImportRegex(imp.Match, imp.Pattern),
				}
			}
		}
	}

	matchDec := func(dec string) {
		if dec == "" {
			return
		}
		for decName, rule := range decoratorLookup {
			if dec == decName || strings.HasSuffix(dec, "."+decName) ||
				strings.HasSuffix(dec, "\\"+decName) {
				detected[rule[0]] = rule[1]
			}
		}
	}

	for _, label := range []string{"Function", "Method", "Class"} {
		// No len()/size() predicate: LadybugDB has no LEN function, so the old
		// query failed with "Catalog exception: function LEN does not exist" on
		// every run — silently, because the caller skips failing queries. Decorator
		// based framework detection was therefore dead. Empty values are filtered
		// in Go below, and inside a transaction the failing query also aborted it.
		q := fmt.Sprintf(`MATCH (n:%s) WHERE n.decorators IS NOT NULL RETURN n.decorators AS decorators LIMIT 1000`, label)
		res, err := db.Query(ctx, q, nil)
		if err != nil {
			continue
		}
		for _, rec := range res.Records {
			switch v := rec["decorators"].(type) {
			case []any:
				for _, d := range v {
					if s, ok := d.(string); ok && s != "" {
						matchDec(s)
					}
				}
			case string:
				if v != "" {
					for _, dec := range strings.Split(v, ",") {
						matchDec(strings.TrimSpace(dec))
					}
				}
			}
		}
	}

	for _, relType := range []string{"INHERITS", "IMPLEMENTS"} {
		q := fmt.Sprintf(`MATCH (c)-[:%s]->(p) RETURN p.name AS parent LIMIT 500`, relType)
		if res, err := db.Query(ctx, q, nil); err == nil {
			for _, rec := range res.Records {
				parent, _ := rec["parent"].(string)
				if rule, ok := heritageLookup[parent]; ok {
					detected[rule[0]] = rule[1]
				}
			}
		}
	}

	q3 := `MATCH (f:File)-[:IMPORTS]->(m:Module) RETURN m.name AS name, m.full_import_name AS full_name LIMIT 1000`
	if res, err := db.Query(ctx, q3, nil); err == nil {
		for _, rec := range res.Records {
			name, _ := rec["name"].(string)
			fullName, _ := rec["full_name"].(string)

			for pattern, entry := range importLookup {
				matched := false
				switch entry.match {
				case "exact":
					matched = fullName == pattern || name == pattern
				case "contains":
					matched = strings.Contains(fullName, pattern) || strings.Contains(name, pattern)
				case "suffix":
					matched = strings.HasSuffix(fullName, pattern) || strings.HasSuffix(name, pattern)
				case "regex":
					if entry.regex != nil {
						matched = entry.regex.MatchString(fullName) || entry.regex.MatchString(name)
					}
				default: // "prefix" or empty
					matched = fullName == pattern || strings.HasPrefix(fullName, pattern+"/") ||
						name == pattern || strings.HasPrefix(name, pattern)
				}
				if matched {
					detected[entry.framework] = entry.category
				}
			}
		}
	}

	return detected
}

func compileImportRegex(match, pattern string) *regexp.Regexp {
	if match != "regex" {
		return nil
	}
	re, err := regexp.Compile(pattern)
	if err != nil {
		slog.Warn("invalid import regex pattern", "pattern", pattern, "error", err)
		return nil
	}
	return re
}

// ScoreEntryPoints computes entry-point scores. When scopePaths is non-empty
// only functions from those files are (re)scored — the incremental path passes
// the changed files so the work is O(delta) instead of O(graph).
func ScoreEntryPoints(ctx context.Context, db GraphDB, projectDir string, scopePaths []string) {

	frameworks := ResolveFrameworks(projectDir)
	var nameRules []NameScoreRule
	var decRules []DecoratorScoreRule
	exportedBonus := 10
	maxScore := 100

	for _, fw := range frameworks {
		if fw.EntryPoints != nil {
			nameRules = append(nameRules, fw.EntryPoints.Names...)
			decRules = append(decRules, fw.EntryPoints.Decorators...)
			if fw.EntryPoints.ExportedBonus > 0 {
				exportedBonus = fw.EntryPoints.ExportedBonus
			}
			if fw.EntryPoints.MaxScore > 0 {
				maxScore = fw.EntryPoints.MaxScore
			}
		}
	}

	// Also load entry point rules from language YAMLs (language-level base scoring)
	for _, langCfg := range ResolveAllLangConfigs(projectDir) {
		if langCfg.EntryPoints != nil {
			nameRules = append(nameRules, langCfg.EntryPoints.Names...)
			decRules = append(decRules, langCfg.EntryPoints.Decorators...)
			if langCfg.EntryPoints.ExportedBonus > 0 {
				exportedBonus = langCfg.EntryPoints.ExportedBonus
			}
			if langCfg.EntryPoints.MaxScore > 0 {
				maxScore = langCfg.EntryPoints.MaxScore
			}
		}
	}

	// Scope to the changed files when the caller knows them (incremental
	// rebuild): entry-point scoring is per-function local — it depends only on
	// the function's own name, decorators, is_exported and lang — so rescoring
	// untouched functions cannot change their score. On a large graph the
	// unscoped query is also truncated by its LIMIT, so scoping is strictly more
	// accurate for the files that did change.
	var (
		q      string
		params map[string]any
	)
	if len(scopePaths) > 0 {
		q = `MATCH (f:Function) WHERE f.path IN $paths RETURN f.uid AS uid, f.name AS name, f.decorators AS decorators, f.is_exported AS is_exported, f.lang AS lang`
		params = map[string]any{"paths": scopePaths}
	} else {
		q = `MATCH (f:Function) RETURN f.uid AS uid, f.name AS name, f.decorators AS decorators, f.is_exported AS is_exported, f.lang AS lang LIMIT 5000`
	}
	res, err := db.Query(ctx, q, params)
	if err != nil {
		return
	}

	// Collect scores and write them in one batch: the previous code issued one
	// MATCH ... SET round-trip per function (up to 5000 per rebuild).
	scored := make([]map[string]any, 0, len(res.Records))

	for _, rec := range res.Records {
		uid, _ := rec["uid"].(string)
		name, _ := rec["name"].(string)
		isExported, _ := rec["is_exported"].(bool)
		lang, _ := rec["lang"].(string)

		var decorators string
		switch v := rec["decorators"].(type) {
		case []any:
			parts := make([]string, 0, len(v))
			for _, d := range v {
				if s, ok := d.(string); ok {
					parts = append(parts, s)
				}
			}
			decorators = strings.Join(parts, ",")
		case string:
			decorators = v
		}

		if uid == "" || name == "" {
			continue
		}

		score := scoreFunctionYAML(name, decorators, isExported, lang, nameRules, decRules, exportedBonus, maxScore)
		if score == 0 {
			continue
		}

		scored = append(scored, map[string]any{"uid": uid, "score": score})
	}

	if len(scored) == 0 {
		return
	}
	uq := `UNWIND $rows AS row MATCH (f:Function {uid: row.uid}) SET f.entry_point_score = row.score`
	if _, err := db.Execute(ctx, uq, map[string]any{"rows": scored}); err != nil {
		// Fall back to per-row writes so a batch-level failure does not silently
		// drop every score.
		for _, row := range scored {
			_, _ = db.Execute(ctx, `MATCH (f:Function {uid: $uid}) SET f.entry_point_score = $score`, row)
		}
	}
}

// scoreFunctionYAML scores a function using YAML-loaded rules.
func scoreFunctionYAML(name, decorators string, isExported bool, lang string,
	nameRules []NameScoreRule, decRules []DecoratorScoreRule,
	exportedBonus, maxScore int) int {
	score := 0
	nameLower := strings.ToLower(name)

	for _, rule := range nameRules {
		pattern := rule.Pattern
		patternLower := strings.ToLower(pattern)

		matched := false
		if strings.HasPrefix(pattern, "*") && strings.HasSuffix(pattern, "*") {
			// contains
			matched = strings.Contains(nameLower, strings.Trim(patternLower, "*"))
		} else if strings.HasSuffix(pattern, "*") {
			// prefix
			matched = strings.HasPrefix(nameLower, strings.TrimSuffix(patternLower, "*"))
		} else if strings.HasPrefix(pattern, "*") {
			// suffix
			matched = strings.HasSuffix(nameLower, strings.TrimPrefix(patternLower, "*"))
		} else {
			// exact
			matched = name == pattern || nameLower == patternLower
		}

		if matched {
			score += rule.Score
		}
	}

	if isExported {
		score += exportedBonus
	}

	if decorators != "" {
		for _, dec := range strings.Split(decorators, ",") {
			dec = strings.TrimSpace(dec)
			for _, rule := range decRules {
				if dec == rule.Name || strings.HasSuffix(dec, "."+rule.Name) {
					score += rule.Score
				}
			}
		}
	}

	if score > maxScore {
		score = maxScore
	}
	return score
}

// RunEnrichment refreshes derived graph properties. scopePaths, when non-empty,
// restricts per-function enrichment to those files (incremental rebuild).
func RunEnrichment(ctx context.Context, db GraphDB, rootPath string, scopePaths []string, logger *slog.Logger) {
	log := slogutil.Resolve(logger)

	configs := DetectProjectConfig(ctx, db, rootPath)
	if len(configs) > 0 {

		props := make([]string, 0, len(configs))
		for k, v := range configs {
			props = append(props, k+"="+v)
		}
		configStr := strings.Join(props, ",")
		q := `MERGE (c:File {path: '__config__'}) SET c.name = 'project_config', c.source = $config`
		_, _ = db.Execute(ctx, q, map[string]any{"config": configStr})
	}

	frameworks := DetectFrameworks(ctx, db, rootPath)
	if len(frameworks) > 0 {
		fwParts := make([]string, 0, len(frameworks))
		for fw, cat := range frameworks {
			fwParts = append(fwParts, fw+":"+cat)
		}
		fwStr := strings.Join(fwParts, ",")
		q := `MERGE (c:File {path: '__config__'}) SET c.lang = $frameworks`
		_, _ = db.Execute(ctx, q, map[string]any{"frameworks": fwStr})
	}

	ScoreEntryPoints(ctx, db, rootPath, scopePaths)

	if len(configs) > 0 || len(frameworks) > 0 {
		log.Debug("enrichment complete", "configs", len(configs), "frameworks", len(frameworks))
	}
}

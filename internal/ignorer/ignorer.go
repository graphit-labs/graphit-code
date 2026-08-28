package ignorer

import (
	"bufio"
	"os"
	"path/filepath"
	"strings"

	gogitignore "github.com/go-git/go-git/v5/plumbing/format/gitignore"
)

// Ignore files, and why the go-git dependency is not a git dependency.
//
// gogitignore is a pure-Go implementation of gitignore PATTERN SEMANTICS — negation, anchoring,
// directory-only patterns, per-file domains. It shells out to nothing and requires no repository.
// Keeping it is what makes `.astignore` and `.wikiignore` behave the way anyone who has written a
// `.gitignore` expects, which is the whole point of using that syntax.
//
// What DID depend on git was the boundary — how far up the tree ignore files are collected from was
// answered by looking for a `.git`. It no longer is: see collectIgnoreFiles.

type IgnoreChecker struct {
	matcher              gogitignore.Matcher
	negationPrefixes     []string
	rootPath             string
	customFileName       string
	basePatterns         []gogitignore.Pattern
	baseNegations        []string
	dirPatterns          map[string][]gogitignore.Pattern
	dirNegations         map[string][]string
}

// DirScope is the ignore rules in force inside one directory of the tree: the
// project's ones plus every ignore file from the root down to that directory.
// A walker crossing directories deepens its scope via At; fswatch aliases it as
// its Ignorer so both sides share the same contract.
type DirScope interface {
	IsIgnored(relPath string, isDir bool) bool
	ShouldDescend(dirRelPath string) bool
	At(dirRelPath string) DirScope
}

func New(rootPath, startDir, customFileName string, defaultPatterns []string) *IgnoreChecker {
	absRoot, _ := filepath.Abs(rootPath)

	// The project is the boundary, and nothing above it. See collectIgnoreFiles.
	boundary := absRoot

	if startDir == "" {
		startDir = absRoot
	} else {
		startDir, _ = filepath.Abs(startDir)
	}

	var allPatterns []gogitignore.Pattern
	var negPrefixes []string

	gitignoreFiles := collectIgnoreFiles(startDir, boundary, ".gitignore")
	for _, gf := range gitignoreFiles {
		domain := domainForFile(gf, absRoot)
		allPatterns = append(allPatterns, readPatternsFromFile(gf, domain)...)
		negPrefixes = append(negPrefixes, readNegationPrefixesFromFile(gf, domain)...)
	}

	if customFileName != "" {
		customFiles := collectIgnoreFiles(startDir, boundary, customFileName)
		for _, cf := range customFiles {
			domain := domainForFile(cf, absRoot)
			allPatterns = append(allPatterns, readPatternsFromFile(cf, domain)...)
			negPrefixes = append(negPrefixes, readNegationPrefixesFromFile(cf, domain)...)
		}
	}

	for _, p := range defaultPatterns {
		p = strings.TrimSpace(p)
		if p == "" || strings.HasPrefix(p, "#") {
			continue
		}
		allPatterns = append(allPatterns, gogitignore.ParsePattern(p, nil))
		if strings.HasPrefix(p, "!") {
			negPrefixes = append(negPrefixes, negationToPrefix(p[1:], nil))
		}
	}

	return &IgnoreChecker{
		matcher:          gogitignore.NewMatcher(allPatterns),
		negationPrefixes: negPrefixes,
		rootPath:         absRoot,
		customFileName:   customFileName,
		basePatterns:     allPatterns,
		baseNegations:    negPrefixes,
		dirPatterns:      make(map[string][]gogitignore.Pattern),
		dirNegations:     make(map[string][]string),
	}
}

// At returns a checker that ALSO understands the ignore files of every directory
// from the root down to dirRelPath, inclusive — which is what git does. A
// `.gitignore` (or the custom file) in a subdirectory scopes its patterns to that
// directory: `node_modules` inside `.opencode/.gitignore` excludes
// `.opencode/node_modules/` and nothing else.
//
// dirRelPath is project-relative, slash-separated; "." or "" return the checker
// unchanged. The accumulation is monotonic, so crossing into a directory picks
// up its patterns on top of its ancestors'.
func (ic *IgnoreChecker) At(dirRelPath string) DirScope {
	if ic == nil {
		return nil
	}
	dirRelPath = strings.Trim(filepath.ToSlash(strings.TrimSpace(dirRelPath)), "/")
	if dirRelPath == "" || dirRelPath == "." {
		return ic
	}

	segments := strings.Split(dirRelPath, "/")
	combined := make([]gogitignore.Pattern, 0, len(ic.basePatterns)+len(segments)*2)
	combined = append(combined, ic.basePatterns...)
	negs := make([]string, 0, len(ic.baseNegations)+len(segments)*2)
	negs = append(negs, ic.baseNegations...)

	prefix := ""
	for _, seg := range segments {
		if prefix == "" {
			prefix = seg
		} else {
			prefix += "/" + seg
		}
		pats, negs2 := ic.loadDirFiles(prefix)
		combined = append(combined, pats...)
		negs = append(negs, negs2...)
	}

	clone := *ic
	clone.matcher = gogitignore.NewMatcher(combined)
	clone.negationPrefixes = negs
	return &clone
}

// loadDirFiles reads the ignore files of one directory below the root and caches
// them. The cache is keyed by directory, so a walk that crosses each directory
// once pays one stat+read per file per directory.
func (ic *IgnoreChecker) loadDirFiles(dirRel string) ([]gogitignore.Pattern, []string) {
	if pats, ok := ic.dirPatterns[dirRel]; ok {
		return pats, ic.dirNegations[dirRel]
	}
	domain := strings.Split(dirRel, "/")
	dirAbs := filepath.Join(ic.rootPath, filepath.FromSlash(dirRel))
	var pats []gogitignore.Pattern
	var negs []string
	for _, name := range []string{".gitignore", ic.customFileName} {
		if name == "" {
			continue
		}
		p := filepath.Join(dirAbs, name)
		if _, err := os.Stat(p); err != nil {
			continue
		}
		pats = append(pats, readPatternsFromFile(p, domain)...)
		negs = append(negs, readNegationPrefixesFromFile(p, domain)...)
	}
	ic.dirPatterns[dirRel] = pats
	ic.dirNegations[dirRel] = negs
	return pats, negs
}

// A nil checker ignores nothing. Callers hold this behind an interface
// (fswatch.Ignorer), where a nil *IgnoreChecker is not a nil interface value, so
// their own nil check cannot catch it — the guard has to live here.
func (ic *IgnoreChecker) IsIgnored(relPath string, isDir bool) bool {
	if ic == nil {
		return false
	}
	relPath = filepath.ToSlash(relPath)
	if relPath == "" || relPath == "." {
		return false
	}
	return ic.matcher.Match(strings.Split(relPath, "/"), isDir)
}

// ShouldDescend reports whether the walker should enter a directory that
// IsIgnored has already flagged as ignored. It returns true when at least one
// negation pattern ("!") targets a child path inside dirRelPath, meaning that
// skipping this directory would hide files the user explicitly re-included.
func (ic *IgnoreChecker) ShouldDescend(dirRelPath string) bool {
	if ic == nil {
		return false
	}
	dirRelPath = filepath.ToSlash(dirRelPath)
	if dirRelPath == "" || dirRelPath == "." {
		return false
	}
	prefix := dirRelPath + "/"
	for _, np := range ic.negationPrefixes {
		if strings.HasPrefix(np, prefix) || strings.HasPrefix(prefix, np+"/") {
			return true
		}
	}
	return false
}

// collectIgnoreFiles gathers an ignore file from startDir up to rootDir, inclusive.
//
// rootDir is the PROJECT, and collection must never pass it. That is not a policy choice, it is
// what the domain arithmetic allows: domainForFile computes a pattern's domain with
// filepath.Rel(project, dir), so a file above the project yields a domain of ".." segments, and
// gogitignore can never match a real path against that. Such a file was collected and silently
// inert.
//
// This is also where the last git dependency was. The boundary used to be found by walking up for a
// `.git`, which meant three things, all bad: a project without a repository fell back to itself
// anyway, a project INSIDE a repository collected the repository's ignore files as inert patterns,
// and a project under a directory that happened to be a repository — a dotfiles $HOME, say —
// reached up into it. Every test in this package created a `.git` purely to give that walk
// something to find.
//
// KNOWN LIMITATION, and it predates this: in a monorepo, patterns in the repository-root
// .gitignore (node_modules/, dist/) do NOT apply to a sub-project. Making them apply needs domains
// computed against the collection root rather than the project, which is a larger change than
// removing git was.
func collectIgnoreFiles(startDir, rootDir, filename string) []string {
	var files []string
	curr := startDir
	for {
		candidate := filepath.Join(curr, filename)
		if _, err := os.Stat(candidate); err == nil {
			files = append(files, candidate)
		}
		if curr == rootDir {
			break
		}
		parent := filepath.Dir(curr)
		if parent == curr {
			break
		}
		curr = parent
	}
	return files
}

func domainForFile(ignoreFilePath, rootPath string) []string {
	dir := filepath.Dir(ignoreFilePath)
	rel, err := filepath.Rel(rootPath, dir)
	if err != nil || rel == "." {
		return nil
	}
	return strings.Split(filepath.ToSlash(rel), "/")
}

func readPatternsFromFile(path string, domain []string) []gogitignore.Pattern {
	f, err := os.Open(path)
	if err != nil {
		return nil
	}
	defer func() { _ = f.Close() }()

	var patterns []gogitignore.Pattern
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := scanner.Text()
		trimmed := strings.TrimSpace(line)

		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		patterns = append(patterns, gogitignore.ParsePattern(trimmed, domain))
	}
	return patterns
}

// readNegationPrefixesFromFile reads an ignore file and returns normalized
// path prefixes for every negation ("!") line.
func readNegationPrefixesFromFile(path string, domain []string) []string {
	f, err := os.Open(path)
	if err != nil {
		return nil
	}
	defer func() { _ = f.Close() }()

	var prefixes []string
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		trimmed := strings.TrimSpace(scanner.Text())
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		if !strings.HasPrefix(trimmed, "!") {
			continue
		}
		prefixes = append(prefixes, negationToPrefix(trimmed[1:], domain))
	}
	return prefixes
}

// negationToPrefix converts a negation pattern body (without the leading '!')
// into its longest literal directory prefix.
// Examples:
//
//	"internal/ast/antlr/common/"  → "internal/ast/antlr/common"
//	"internal/ast/antlr/*/driver.go" → "internal/ast/antlr"
func negationToPrefix(body string, domain []string) string {
	body = strings.TrimRight(body, " ")
	body = strings.TrimSuffix(body, "/")
	body = filepath.ToSlash(body)

	parts := strings.Split(body, "/")

	// Keep only the leading literal segments (no globs).
	var literal []string
	for _, seg := range parts {
		if strings.ContainsAny(seg, "*?[") {
			break
		}
		literal = append(literal, seg)
	}

	// Prepend domain (the directory containing the ignore file, relative to root).
	if len(domain) > 0 {
		literal = append(domain, literal...)
	}

	return strings.Join(literal, "/")
}

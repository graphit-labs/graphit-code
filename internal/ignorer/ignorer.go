package ignorer

import (
	"bufio"
	"os"
	"path/filepath"
	"strings"

	gogitignore "github.com/go-git/go-git/v5/plumbing/format/gitignore"
)

type IgnoreChecker struct {
	matcher gogitignore.Matcher

	// negationPrefixes holds normalized path prefixes derived from negation
	// patterns (lines starting with '!'). Used by ShouldDescend to determine
	// whether a walker must enter an otherwise-ignored directory.
	negationPrefixes []string

	rootPath string
}

func New(rootPath, startDir, customFileName string, defaultPatterns []string) *IgnoreChecker {
	absRoot, _ := filepath.Abs(rootPath)

	gitRoot := findGitRoot(absRoot)
	if gitRoot == "" {
		gitRoot = absRoot
	}

	if startDir == "" {
		startDir = absRoot
	} else {
		startDir, _ = filepath.Abs(startDir)
	}

	var allPatterns []gogitignore.Pattern
	var negPrefixes []string

	gitignoreFiles := collectIgnoreFiles(startDir, gitRoot, ".gitignore")
	for _, gf := range gitignoreFiles {
		domain := domainForFile(gf, absRoot)
		patterns := readPatternsFromFile(gf, domain)
		allPatterns = append(allPatterns, patterns...)
		negPrefixes = append(negPrefixes, readNegationPrefixesFromFile(gf, domain)...)
	}

	if customFileName != "" {
		customFiles := collectIgnoreFiles(startDir, gitRoot, customFileName)
		for _, cf := range customFiles {
			domain := domainForFile(cf, absRoot)
			patterns := readPatternsFromFile(cf, domain)
			allPatterns = append(allPatterns, patterns...)
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
	}
}

func (ic *IgnoreChecker) IsIgnored(relPath string, isDir bool) bool {
	relPath = filepath.ToSlash(relPath)
	if relPath == "" || relPath == "." {
		return false
	}

	pathSegments := strings.Split(relPath, "/")
	return ic.matcher.Match(pathSegments, isDir)
}

// ShouldDescend reports whether the walker should enter a directory that
// IsIgnored has already flagged as ignored. It returns true when at least one
// negation pattern ("!") targets a child path inside dirRelPath, meaning that
// skipping this directory would hide files the user explicitly re-included.
func (ic *IgnoreChecker) ShouldDescend(dirRelPath string) bool {
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

func findGitRoot(startDir string) string {
	curr := startDir
	for {
		if fi, err := os.Stat(filepath.Join(curr, ".git")); err == nil && fi != nil {
			return curr
		}
		parent := filepath.Dir(curr)
		if parent == curr {
			return ""
		}
		curr = parent
	}
}

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

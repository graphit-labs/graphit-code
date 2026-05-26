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

	gitignoreFiles := collectIgnoreFiles(startDir, gitRoot, ".gitignore")
	for _, gf := range gitignoreFiles {
		domain := domainForFile(gf, absRoot)
		patterns := readPatternsFromFile(gf, domain)
		allPatterns = append(allPatterns, patterns...)
	}

	if customFileName != "" {
		customFiles := collectIgnoreFiles(startDir, gitRoot, customFileName)
		for _, cf := range customFiles {
			domain := domainForFile(cf, absRoot)
			patterns := readPatternsFromFile(cf, domain)
			allPatterns = append(allPatterns, patterns...)
		}
	}

	for _, p := range defaultPatterns {
		p = strings.TrimSpace(p)
		if p == "" || strings.HasPrefix(p, "#") {
			continue
		}

		allPatterns = append(allPatterns, gogitignore.ParsePattern(p, nil))
	}

	return &IgnoreChecker{
		matcher:  gogitignore.NewMatcher(allPatterns),
		rootPath: absRoot,
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

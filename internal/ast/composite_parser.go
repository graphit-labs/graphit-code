package ast

import (
	"context"
	"errors"
	"fmt"
	"strings"
)

// CompositeParser dispatches parsing to either tree-sitter or ANTLR
// based on which grammar is registered for a file extension.
// Tree-sitter takes precedence when both are available for the same extension.
// If tree-sitter fails or extracts nothing, ANTLR is tried as fallback.
//
// When grammarOverrides is set (via --grammar), the specified grammar is used
// directly — the grammar name determines the backend (antlr-* → ANTLR,
// tree-sitter-* → tree-sitter) with no fallback.
type CompositeParser struct {
	treeSitter       *TreeSitterParser
	antlr            *AntlrParser
	grammarOverrides map[string]string
	scipEntries      map[string]*parseCacheEntry
}

// SetSCIPEntries supplies the per-pipeline index. It is set before workers
// start, then read concurrently without mutation.
func (c *CompositeParser) SetSCIPEntries(entries map[string]*parseCacheEntry) {
	c.scipEntries = entries
}

func NewCompositeParser(projectDir string, grammarOverrides map[string]string) *CompositeParser {
	return &CompositeParser{
		treeSitter: &TreeSitterParser{
			projectDir: projectDir,
		},
		antlr: &AntlrParser{
			projectDir: projectDir,
		},
		grammarOverrides: grammarOverrides,
	}
}

func (c *CompositeParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	if opts.Cancelled != nil && opts.Cancelled() {
		return nil, context.Canceled
	}
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])

	var scipResult *ParsedFile
	if scipFamilyFor(c.treeSitter.projectDir, ext) != "" {
		if entry := c.scipEntries[path]; entry != nil {
			parsedEntry := *entry
			parsedEntry.IsDepend = isDepend
			scipResult = &ParsedFile{Path: path, RepoPath: entry.RelPath, Language: entry.Language,
				Parser: "scip", IsDepend: isDepend, SCIPEntry: &parsedEntry}
		}
		// Missing/failed SCIP output is handled by the established parser chain.
	}
	grammar, overridden := c.grammarOverrides[ext]
	return parseBackendSelection(scipResult, grammar, overridden,
		func(grammar string) (*ParsedFile, error) {
			return c.parseWithGrammar(path, grammar, isDepend, opts)
		},
		func() (*ParsedFile, error) {
			return parseSyntaxFallback(ext,
				HasTreeSitterForExtensionIn(c.treeSitter.projectDir, ext, opts.Logger),
				HasAntlrForExtensionIn(c.treeSitter.projectDir, ext, opts.Logger),
				func() (*ParsedFile, error) { return c.treeSitter.Parse(path, isDepend, opts) },
				func() (*ParsedFile, error) { return c.antlr.Parse(path, isDepend, opts) })
		})
}

// parseBackendSelection keeps syntax selection independent of the SCIP language.
// An explicit grammar replaces the default syntax chain, including Tree-sitter.
func parseBackendSelection(scipResult *ParsedFile, grammar string, overridden bool,
	parseGrammar func(string) (*ParsedFile, error), parseDefault func() (*ParsedFile, error)) (*ParsedFile, error) {
	if scipResult != nil {
		return scipResult, nil
	}
	if overridden {
		return parseGrammar(grammar)
	}
	return parseDefault()
}

// parseSyntaxFallback is the framework policy shared by every language when
// SCIP has no usable document. Grammar availability controls the chain.
func parseSyntaxFallback(ext string, hasTS, hasAntlr bool,
	parseTreeSitter, parseAntlr func() (*ParsedFile, error)) (*ParsedFile, error) {
	if hasAntlr && !hasTS {
		pf, err := parseAntlr()
		if pf != nil {
			pf.Parser = "antlr4"
		}
		return pf, err
	}

	if hasTS && hasAntlr {
		pf, err := parseTreeSitter()
		if errors.Is(err, context.Canceled) {
			return nil, err
		}
		if err == nil && pf != nil && pf.EntityCount() > 0 {
			pf.Parser = "tree-sitter"
			return pf, nil
		}
		pf, err = parseAntlr()
		if pf != nil {
			pf.Parser = "antlr4"
		}
		return pf, err
	}

	if hasTS {
		pf, err := parseTreeSitter()
		if pf != nil {
			pf.Parser = "tree-sitter"
		}
		return pf, err
	}

	return nil, fmt.Errorf("no parser for %s", ext)
}

func (c *CompositeParser) parseWithGrammar(path, grammar string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	if strings.HasPrefix(grammar, "antlr-") {
		pf, err := c.antlr.ParseWithGrammar(path, grammar, isDepend, opts)
		if pf != nil {
			pf.Parser = "antlr4"
		}
		return pf, err
	}

	pf, err := c.treeSitter.ParseWithGrammar(path, grammar, isDepend, opts)
	if pf != nil {
		pf.Parser = "tree-sitter"
	}
	return pf, err
}

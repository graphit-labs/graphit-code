package ast

import (
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
	grammarOverrides map[string]string // ext → grammar name (e.g. ".sql" → "antlr-plsql")
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
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])

	// Grammar override: use the specified grammar directly, no fallback.
	if grammar, ok := c.grammarOverrides[ext]; ok {
		return c.parseWithGrammar(path, grammar, isDepend, opts)
	}

	hasTS := HasTreeSitterForExtensionIn(c.treeSitter.projectDir, ext)
	hasAntlr := HasAntlrForExtensionIn(c.treeSitter.projectDir, ext)

	if hasAntlr && !hasTS {
		pf, err := c.antlr.Parse(path, isDepend, opts)
		if pf != nil {
			pf.Parser = "antlr4"
		}
		return pf, err
	}

	// Both engines support this extension: try tree-sitter first,
	// fall back to ANTLR if it fails or extracts nothing useful.
	if hasTS && hasAntlr {
		pf, err := c.treeSitter.Parse(path, isDepend, opts)
		if err == nil && pf.EntityCount() > 0 {
			pf.Parser = "tree-sitter"
			return pf, nil
		}
		pf, err = c.antlr.Parse(path, isDepend, opts)
		if pf != nil {
			pf.Parser = "antlr4"
		}
		return pf, err
	}

	if hasTS {
		pf, err := c.treeSitter.Parse(path, isDepend, opts)
		if pf != nil {
			pf.Parser = "tree-sitter"
		}
		return pf, err
	}

	return nil, fmt.Errorf("no parser for %s", ext)
}

// parseWithGrammar dispatches to the correct backend based on grammar name prefix.
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

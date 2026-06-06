package ast

import (
	"fmt"
	"strings"
)

// CompositeParser dispatches parsing to either tree-sitter or ANTLR
// based on which grammar is registered for a file extension.
// Tree-sitter takes precedence when both are available for the same extension.
// If tree-sitter fails or extracts nothing, ANTLR is tried as fallback.
type CompositeParser struct {
	treeSitter     *TreeSitterParser
	antlr          *AntlrParser
	forceAntlrExts map[string]bool
}

func NewCompositeParser(projectDir string, wm *WorkerModules, awm *AntlrWorkerModules, forceAntlrExts map[string]bool) *CompositeParser {
	return &CompositeParser{
		treeSitter: &TreeSitterParser{
			projectDir:    projectDir,
			workerModules: wm,
		},
		antlr: &AntlrParser{
			projectDir:    projectDir,
			workerModules: awm,
		},
		forceAntlrExts: forceAntlrExts,
	}
}

func (c *CompositeParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])

	hasTS := HasTreeSitterForExtension(ext)
	hasAntlr := HasAntlrForExtension(ext)

	// Forced ANTLR for this extension (--force-antlr flag)
	if c.forceAntlrExts[ext] && hasAntlr {
		pf, err := c.antlr.Parse(path, isDepend, opts)
		if pf != nil {
			pf.Parser = "antlr4"
		}
		return pf, err
	}

	// ANTLR-only extension
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

	// Tree-sitter only
	if hasTS {
		pf, err := c.treeSitter.Parse(path, isDepend, opts)
		if pf != nil {
			pf.Parser = "tree-sitter"
		}
		return pf, err
	}

	return nil, fmt.Errorf("no parser for %s", ext)
}

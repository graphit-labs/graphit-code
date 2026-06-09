package ast

import (
	"fmt"
	"strings"
)

// antlrExtMap maps file extensions to ANTLR language configs.
// antlrGrammarMap maps grammar names (e.g. "antlr-plsql") to configs.
// Both populated during init from YAML query files with parser=antlr4.
// The YAML is the single source of truth — grammar binaries are loaded lazily.
var antlrExtMap map[string]*antlrLangConfig
var antlrGrammarMap map[string]*antlrLangConfig

type antlrLangConfig struct {
	Language   string
	Grammar    string // Binary name (e.g. "antlr-plsql")
	Extensions []string
	StartRule  string
}

func initAntlrExtMap() {
	antlrExtMap = make(map[string]*antlrLangConfig)
	antlrGrammarMap = make(map[string]*antlrLangConfig)

	runtimeQ := loadRuntimeCached()
	for _, qf := range runtimeQ {
		if qf.Parser != "antlr4" {
			continue
		}
		grammar := qf.Grammar
		if grammar == "" {
			grammar = "antlr-" + qf.Language
		}
		cfg := &antlrLangConfig{
			Language:   qf.Language,
			Grammar:    grammar,
			Extensions: qf.Extensions,
			StartRule:  qf.StartRule,
		}
		for _, ext := range qf.Extensions {
			antlrExtMap[ext] = cfg
		}
		antlrGrammarMap[grammar] = cfg
	}
}

func init() {
	initAntlrExtMap()
}

// AntlrParser implements LanguageParser for ANTLR v4 grammars.
type AntlrParser struct {
	projectDir    string
}

func (a *AntlrParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfg, ok := antlrExtMap[ext]
	if !ok {
		// Try plug-and-play: check for ANTLR grammar configs in the project
		langName := strings.TrimPrefix(ext, ".")
		qfs := resolveQueriesForLang(a.projectDir, langName, ext)
		for _, qf := range qfs {
			if qf.Parser == "antlr4" {
				grammar := qf.Grammar
				if grammar == "" {
					grammar = "antlr-" + qf.Language
				}
				cfg = &antlrLangConfig{
					Language:   qf.Language,
					Grammar:    grammar,
					Extensions: qf.Extensions,
					StartRule:  qf.StartRule,
				}
				break
			}
		}
		if cfg == nil {
			return nil, fmt.Errorf("no ANTLR grammar for %s", ext)
		}
	}

	return a.parseWithConfig(path, ext, cfg, isDepend, opts)
}

// ParseWithGrammar parses using a specific ANTLR grammar name (e.g. "antlr-plsql"),
// bypassing the extension-based lookup. Used by CompositeParser for --grammar overrides.
func (a *AntlrParser) ParseWithGrammar(path, grammarName string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	cfg, ok := antlrGrammarMap[grammarName]
	if !ok {
		return nil, fmt.Errorf("unknown ANTLR grammar: %s", grammarName)
	}
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	return a.parseWithConfig(path, ext, cfg, isDepend, opts)
}

func (a *AntlrParser) parseWithConfig(path, ext string, cfg *antlrLangConfig, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}

	rpcParser, err := getParserPlugin(a.projectDir)
	if err != nil {
		return nil, err
	}

	var langConfig *ExternalQueryFile
	var queries []tsQueryDef

	if a.projectDir != "" {
		resolved := resolveQueriesForLang(a.projectDir, cfg.Language, ext)
		for _, qf := range resolved {
			if qf.Parser == "antlr4" {
				langConfig = &qf
				break
			}
		}
		if langConfig != nil {
			for _, eq := range langConfig.Queries {
				queries = append(queries, tsQueryDef(eq))
			}
		}
	}

	var rpcQueries []ExternalQueryDef
	for _, q := range queries {
		rpcQueries = append(rpcQueries, ExternalQueryDef(q))
	}

	req := ParseRequest{
		Path:       path,
		Content:    src,
		Grammar:    cfg.Grammar,
		ParserType: "antlr4",
		Language:   cfg.Language,
		IsDepend:   isDepend,
		Queries:    rpcQueries,
		LangConfig: langConfig,
	}

	resp, err := rpcParser.Parse(req)
	if err != nil {
		return nil, err
	}
	if resp.Error != "" {
		return nil, fmt.Errorf("plugin error: %s", resp.Error)
	}

	return resp.ParsedFile, nil
}

// HasAntlrForExtension returns true if there's an ANTLR grammar for the extension.
func HasAntlrForExtension(ext string) bool {
	_, ok := antlrExtMap[strings.ToLower(ext)]
	return ok
}

// AntlrConfigForGrammar returns the config for a named ANTLR grammar, or nil.
func AntlrConfigForGrammar(name string) *antlrLangConfig {
	return antlrGrammarMap[name]
}

// AntlrLangForExtension returns the language name for an ANTLR-supported extension.
func AntlrLangForExtension(ext string) string {
	if cfg, ok := antlrExtMap[strings.ToLower(ext)]; ok {
		return cfg.Language
	}
	return ""
}

// HasParserForExtension returns true if any parser (tree-sitter or ANTLR) handles the extension.
func HasParserForExtension(ext string) bool {
	return HasTreeSitterForExtension(ext) || HasAntlrForExtension(ext)
}

// AntlrSupportedExtensions returns all file extensions handled by ANTLR grammars.
func AntlrSupportedExtensions() []string {
	var exts []string
	for ext := range antlrExtMap {
		exts = append(exts, ext)
	}
	return exts
}

// AllSupportedExtensions returns all extensions supported by any parser.
func AllSupportedExtensions() []string {
	seen := make(map[string]bool)
	var exts []string
	for ext := range tsExtMap {
		if !seen[ext] {
			seen[ext] = true
			exts = append(exts, ext)
		}
	}
	for ext := range antlrExtMap {
		if !seen[ext] {
			seen[ext] = true
			exts = append(exts, ext)
		}
	}
	return exts
}

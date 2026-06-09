package ast

import (
	"fmt"
	"strings"
)

type tsLangConfig struct {
	Language   string
	Grammar    string
	Extensions []string
}

// tsQueryDef mirrors ExternalQueryDef for direct struct cast.
type tsQueryDef struct {
	DataKey      string
	GraphLabel   string
	Pattern      string
	NameCapture  string
	Type         string
	RelationType string
}

var tsExtMap map[string]*tsLangConfig
var tsGrammarMap map[string]*tsLangConfig

func initTsExtMap() {
	tsExtMap = make(map[string]*tsLangConfig)
	tsGrammarMap = make(map[string]*tsLangConfig)

	runtimeQ := loadRuntimeCached()
	for _, qf := range runtimeQ {
		if qf.Parser == "antlr4" {
			continue
		}
		grammar := qf.Grammar
		if grammar == "" {
			grammar = "tree-sitter-" + qf.Language
		}
		cfg := &tsLangConfig{
			Language:   qf.Language,
			Grammar:    grammar,
			Extensions: qf.Extensions,
		}
		for _, ext := range qf.Extensions {
			tsExtMap[ext] = cfg
		}
		tsGrammarMap[grammar] = cfg
	}
}

func init() {
	initTsExtMap()
}

type TreeSitterParser struct {
	projectDir    string
}

func (t *TreeSitterParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfg, ok := tsExtMap[ext]
	if !ok {
		return nil, fmt.Errorf("no grammar for %s", ext)
	}
	return t.parseWithConfig(path, ext, cfg, isDepend, opts)
}

// ParseWithGrammar parses using a specific tree-sitter grammar name (e.g. "tree-sitter-sql"),
// bypassing the extension-based lookup. Used by CompositeParser for --grammar overrides.
func (t *TreeSitterParser) ParseWithGrammar(path, grammarName string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	cfg, ok := tsGrammarMap[grammarName]
	if !ok {
		return nil, fmt.Errorf("unknown tree-sitter grammar: %s", grammarName)
	}
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	return t.parseWithConfig(path, ext, cfg, isDepend, opts)
}

func (t *TreeSitterParser) parseWithConfig(path, ext string, cfg *tsLangConfig, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}

	rpcParser, err := getParserPlugin(t.projectDir)
	if err != nil {
		return nil, err
	}

	var langConfig *ExternalQueryFile
	var queries []tsQueryDef
	if t.projectDir != "" {
		queries = mergedQueriesFor(t.projectDir, cfg.Language, ext, nil)
		langConfig = resolvedLangConfigFor(t.projectDir, cfg.Language, ext)
	}

	var rpcQueries []ExternalQueryDef
	for _, q := range queries {
		rpcQueries = append(rpcQueries, ExternalQueryDef(q))
	}

	req := ParseRequest{
		Path:       path,
		Content:    src,
		Grammar:    cfg.Grammar,
		ParserType: "tree-sitter",
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

func HasTreeSitterForExtension(ext string) bool {
	_, ok := tsExtMap[strings.ToLower(ext)]
	return ok
}

// TSConfigForGrammar returns the config for a named tree-sitter grammar, or nil.
func TSConfigForGrammar(name string) *tsLangConfig {
	return tsGrammarMap[name]
}

func TreeSitterLangForExtension(ext string) string {
	if cfg, ok := tsExtMap[strings.ToLower(ext)]; ok {
		return cfg.Language
	}
	return ""
}




func TreeSitterSupportedExtensions() []string {
	var exts []string
	for ext := range tsExtMap {
		exts = append(exts, ext)
	}
	return exts
}

package main

import (
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/hashicorp/go-plugin"
)

type ParserImpl struct{}

func (p *ParserImpl) Parse(req ast.ParseRequest) (ast.ParseResponse, error) {
	var pf *ast.ParsedFile
	var err error

	if req.ParserType == "antlr4" {
		pf, err = parseAntlr(req)
	} else {
		pf, err = parseTreeSitter(req)
	}

	if err != nil {
		return ast.ParseResponse{Error: err.Error()}, nil
	}
	return ast.ParseResponse{ParsedFile: pf}, nil
}

func main() {
	plugin.Serve(&plugin.ServeConfig{
		HandshakeConfig: ast.HandshakeConfig,
		Plugins: map[string]plugin.Plugin{
			"parser": &ast.ParserPlugin{Impl: &ParserImpl{}},
		},
	})
}

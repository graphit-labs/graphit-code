package ast

import (
	"net/rpc"

	"github.com/hashicorp/go-plugin"
)

type ParseRequest struct {
	Path       string
	Content    []byte
	Grammar    string
	ParserType string // "tree-sitter" or "antlr4"
	Language   string
	IsDepend   bool
	Queries    []ExternalQueryDef
	LangConfig *ExternalQueryFile
}

type ParseResponse struct {
	ParsedFile *ParsedFile
	Error      string
}

type Parser interface {
	Parse(req ParseRequest) (ParseResponse, error)
}

type RPCClient struct {
	client *rpc.Client
}

func (c *RPCClient) Parse(req ParseRequest) (ParseResponse, error) {
	var resp ParseResponse
	err := c.client.Call("Plugin.Parse", req, &resp)
	return resp, err
}

type RPCServer struct {
	Impl Parser
}

func (s *RPCServer) Parse(req ParseRequest, resp *ParseResponse) error {
	var err error
	*resp, err = s.Impl.Parse(req)
	return err
}

type ParserPlugin struct {
	Impl Parser
}

func (p *ParserPlugin) Server(*plugin.MuxBroker) (interface{}, error) {
	return &RPCServer{Impl: p.Impl}, nil
}

func (p *ParserPlugin) Client(b *plugin.MuxBroker, c *rpc.Client) (interface{}, error) {
	return &RPCClient{client: c}, nil
}

var HandshakeConfig = plugin.HandshakeConfig{
	ProtocolVersion:  1,
	MagicCookieKey:   "GRAPHIT_PARSER_PLUGIN",
	MagicCookieValue: "hello",
}

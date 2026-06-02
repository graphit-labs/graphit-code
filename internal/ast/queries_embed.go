package ast

import "embed"

//go:embed queries/*.yaml
var embeddedQueryFS embed.FS

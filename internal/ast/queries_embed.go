package ast

import "embed"

//go:embed queries/*.yaml
var embeddedQueryFS embed.FS

//go:embed frameworks/*.yaml
var embeddedFrameworkFS embed.FS

//go:embed ecosystems.yaml
var embeddedEcosystemFS embed.FS

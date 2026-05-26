package ast

import (
	"github.com/graphit-labs/graphit-code/internal/ignorer"
)

const AstIgnoreFile = ".astignore"

func NewAstIgnoreChecker(rootPath string) *ignorer.IgnoreChecker {
	return ignorer.New(rootPath, rootPath, AstIgnoreFile, nil)
}

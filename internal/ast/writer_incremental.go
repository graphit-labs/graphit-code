package ast

import (
	"context"
)

func (w *GraphWriter) WriteFileIncremental(ctx context.Context, pf *ParsedFile, repoPath string) error {
	return w.WriteChunkIncremental(ctx, []*ParsedFile{pf}, repoPath)
}


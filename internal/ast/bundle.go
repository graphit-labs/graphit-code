package ast

import (
	"archive/zip"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

type BundleNode struct {
	Label      string         `json:"label"`
	Properties map[string]any `json:"properties"`
}

type BundleEdge struct {
	Type        string         `json:"type"`
	SourceLabel string         `json:"source_label"`
	SourceMatch map[string]any `json:"source_match"`
	TargetLabel string         `json:"target_label"`
	TargetMatch map[string]any `json:"target_match"`
	Properties  map[string]any `json:"properties,omitempty"`
}

type BundleManifest struct {
	Version   string `json:"version"`
	RepoPath  string `json:"repo_path"`
	NodeCount int    `json:"node_count"`
	EdgeCount int    `json:"edge_count"`
	// SourceCount is how many files the bundle carries text for, so a consumer can
	// tell a structure-only bundle from a truncated one instead of guessing.
	SourceCount int `json:"source_count"`
}

// BundleOptions says where the file text comes from and whether to carry it.
//
// Text is not a graph property — the search index owns it — so a bundle can only
// include it when it knows which store to read. Without StorePath the bundle is
// structure-only, and says so in its manifest.
type BundleOptions struct {
	StorePath string
	NoSources bool
}

const bundleSourceDir = "sources/"

func ExportBundle(ctx context.Context, db GraphDB, repoPath, outputPath string,
	opts BundleOptions, logger *slog.Logger) error {
	log := slogutil.Resolve(logger)
	abs, _ := filepath.Abs(repoPath)
	prefix := abs + "/"

	var nodes []BundleNode
	var edges []BundleEdge

	allLabels := make(map[string]bool)
	for _, l := range ActiveNodeLabels(ctx, db) {
		allLabels[l] = true
	}

	for label := range allLabels {
		res, err := db.Execute(ctx, fmt.Sprintf(
			`MATCH (n:%s) WHERE n.path STARTS WITH $prefix OR n.path = $repo RETURN n`, label),
			map[string]any{"prefix": prefix, "repo": abs})
		if err != nil {
			continue
		}
		for _, rec := range res.Records {
			if props, ok := rec["n"].(map[string]any); ok {
				nodes = append(nodes, BundleNode{Label: label, Properties: props})
			}
		}
	}

	res, err := db.Execute(ctx, `MATCH (m:Module) RETURN m`, nil)
	if err == nil {
		for _, rec := range res.Records {
			if props, ok := rec["m"].(map[string]any); ok {
				nodes = append(nodes, BundleNode{Label: "Module", Properties: props})
			}
		}
	}

	edgeTypes := []struct{ relType, srcLabel, tgtLabel string }{
		{"CONTAINS", "", ""},
		{"CALLS", "", ""},
		{"INHERITS", "", ""},
		{"IMPLEMENTS", "", ""},
		{"IMPORTS", "", ""},
		{"HAS_PARAMETER", "", ""},
	}

	for _, et := range edgeTypes {
		q := fmt.Sprintf(
			`MATCH (a)-[r:%s]->(b) WHERE a.path STARTS WITH $prefix OR a.path = $repo
			 RETURN labels(a)[0] AS sl, a.name AS sn, a.path AS sp,
			        labels(b)[0] AS tl, b.name AS tn, b.path AS tp,
			        type(r) AS rt, properties(r) AS rp`,
			et.relType)
		res, err := db.Execute(ctx, q, map[string]any{"prefix": prefix, "repo": abs})
		if err != nil {
			continue
		}
		for _, rec := range res.Records {
			edge := BundleEdge{
				Type:        fmt.Sprint(rec["rt"]),
				SourceLabel: fmt.Sprint(rec["sl"]),
				SourceMatch: map[string]any{"name": rec["sn"], "path": rec["sp"]},
				TargetLabel: fmt.Sprint(rec["tl"]),
				TargetMatch: map[string]any{"name": rec["tn"], "path": rec["tp"]},
			}
			if rp, ok := rec["rp"].(map[string]any); ok {
				edge.Properties = rp
			}
			edges = append(edges, edge)
		}
	}

	f, err := os.Create(outputPath)
	if err != nil {
		return fmt.Errorf("create bundle: %w", err)
	}
	defer func() { _ = f.Close() }()

	zw := zip.NewWriter(f)
	defer func() { _ = zw.Close() }()

	sourceCount := 0
	if !opts.NoSources && opts.StorePath != "" {
		n, err := writeBundleSources(zw, opts.StorePath)
		if err != nil {
			return err
		}
		sourceCount = n
	}

	manifest := BundleManifest{
		Version:     "1.0",
		RepoPath:    abs,
		NodeCount:   len(nodes),
		EdgeCount:   len(edges),
		SourceCount: sourceCount,
	}
	if err := writeJSONToZip(zw, "manifest.json", manifest); err != nil {
		return err
	}
	if err := writeJSONToZip(zw, "nodes.json", nodes); err != nil {
		return err
	}
	if err := writeJSONToZip(zw, "edges.json", edges); err != nil {
		return err
	}

	log.Info("bundle exported", "nodes", len(nodes), "edges", len(edges),
		"sources", sourceCount, "path", outputPath)
	return nil
}

func writeBundleSources(zw *zip.Writer, indexPath string) (int, error) {
	count := 0
	err := EachFileSource(context.Background(), indexPath, func(relPath, src string) error {
		clean := filepath.Clean(relPath)
		if filepath.IsAbs(clean) || clean == ".." ||
			strings.HasPrefix(clean, ".."+string(filepath.Separator)) {
			return nil
		}
		w, err := zw.Create(bundleSourceDir + filepath.ToSlash(clean))
		if err != nil {
			return fmt.Errorf("bundle source entry %s: %w", clean, err)
		}
		if _, err := io.WriteString(w, src); err != nil {
			return fmt.Errorf("bundle source write %s: %w", clean, err)
		}
		count++
		return nil
	})
	if err != nil {
		return 0, fmt.Errorf("bundle sources: %w", err)
	}
	return count, nil
}

func writeJSONToZip(zw *zip.Writer, name string, data any) error {
	w, err := zw.Create(name)
	if err != nil {
		return err
	}
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	return enc.Encode(data)
}

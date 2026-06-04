package ast

import (
	"archive/zip"
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"

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
}

func ExportBundle(ctx context.Context, db GraphDB, repoPath, outputPath string, logger *slog.Logger) error {
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

	manifest := BundleManifest{
		Version:   "1.0",
		RepoPath:  abs,
		NodeCount: len(nodes),
		EdgeCount: len(edges),
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

	log.Info("bundle exported", "nodes", len(nodes), "edges", len(edges), "path", outputPath)
	return nil
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

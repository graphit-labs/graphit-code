package ast

import (
	"archive/zip"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
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

func ExportBundle(ctx context.Context, db GraphDB, repoPath, outputPath string) error {
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
	defer f.Close()

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

	fmt.Fprintf(os.Stderr, "[BUNDLE] Exported %d nodes, %d edges to %s\n", len(nodes), len(edges), outputPath)
	return nil
}

func ImportBundle(ctx context.Context, db GraphDB, bundlePath string) error {
	zr, err := zip.OpenReader(bundlePath)
	if err != nil {
		return fmt.Errorf("open bundle: %w", err)
	}
	defer func() { _ = zr.Close() }()

	var nodes []BundleNode
	var edges []BundleEdge

	for _, f := range zr.File {
		switch f.Name {
		case "nodes.json":
			if err := readJSONFromZip(f, &nodes); err != nil {
				return fmt.Errorf("read nodes: %w", err)
			}
		case "edges.json":
			if err := readJSONFromZip(f, &edges); err != nil {
				return fmt.Errorf("read edges: %w", err)
			}
		}
	}

	for _, node := range nodes {
		props := node.Properties
		propsJSON, _ := json.Marshal(props)
		var propsMap map[string]any
		_ = json.Unmarshal(propsJSON, &propsMap)

		sets := make([]string, 0, len(propsMap))
		params := make(map[string]any)
		i := 0
		for k, v := range propsMap {
			paramName := fmt.Sprintf("p%d", i)
			sets = append(sets, fmt.Sprintf("n.%s = $%s", k, paramName))
			params[paramName] = v
			i++
		}

		if name, ok := propsMap["name"]; ok {
			params["name"] = name
			q := fmt.Sprintf(`MERGE (n:%s {name: $name}) SET %s`, node.Label, joinStrings(sets, ", "))
			db.Execute(ctx, q, params)
		}
	}

	for _, edge := range edges {
		params := map[string]any{
			"sn": edge.SourceMatch["name"],
			"sp": edge.SourceMatch["path"],
			"tn": edge.TargetMatch["name"],
			"tp": edge.TargetMatch["path"],
		}
		q := fmt.Sprintf(
			`MATCH (a:%s {name: $sn}) MATCH (b:%s {name: $tn}) MERGE (a)-[:%s]->(b)`,
			edge.SourceLabel, edge.TargetLabel, edge.Type)
		db.Execute(ctx, q, params)
	}

	fmt.Fprintf(os.Stderr, "[BUNDLE] Imported %d nodes, %d edges from %s\n", len(nodes), len(edges), bundlePath)
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

func readJSONFromZip(f *zip.File, target any) error {
	rc, err := f.Open()
	if err != nil {
		return err
	}
	defer rc.Close()
	data, err := io.ReadAll(rc)
	if err != nil {
		return err
	}
	return json.Unmarshal(data, target)
}

func joinStrings(strs []string, sep string) string {
	result := ""
	for i, s := range strs {
		if i > 0 {
			result += sep
		}
		result += s
	}
	return result
}

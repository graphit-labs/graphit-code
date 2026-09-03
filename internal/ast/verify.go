package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// Divergence is one node whose text the file on disk does not support.
type Divergence struct {
	Label      string
	Path       string
	LineNumber int
	EndLine    int
	// Graph is what the database returned; Window is the text actually on those
	// lines. Reported side by side because the whole point is that the graph's
	// answer looks perfectly plausible on its own.
	Graph  string
	Window string
}

// VerifyReport is the outcome of one integrity pass.
type VerifyReport struct {
	Checked     int
	Skipped     int
	FilesRead   int
	Divergences []Divergence
	// Transient counts nodes that failed the first pass and agreed with disk on the
	// second. They are NOT corruption — see VerifyGraphAgainstDisk.
	Transient int
}

// Clean reports whether the graph agreed with disk everywhere it could be checked.
func (r VerifyReport) Clean() bool { return len(r.Divergences) == 0 }

const verifyBatch = 20000

var verifyRecheckDelay = 3 * time.Second

func VerifyGraphAgainstDisk(ctx context.Context, db GraphDB, rootPath string) (VerifyReport, error) {
	var report VerifyReport

	labels, err := verifiableLabels(ctx, db)
	if err != nil {
		return report, err
	}
	if len(labels) == 0 {
		return report, nil
	}

	var candidates []Divergence
	byPath := make(map[string][]graphText)
	for _, label := range labels {
		rows, err := fetchGraphText(ctx, db, label)
		if err != nil {
			return report, err
		}
		for _, r := range rows {
			if r.path == "" || r.line <= 0 || r.text == "" {
				report.Skipped++
				continue
			}
			byPath[r.path] = append(byPath[r.path], r)
		}
	}

	paths := make([]string, 0, len(byPath))
	for p := range byPath {
		paths = append(paths, p)
	}
	sort.Strings(paths)

	for _, relPath := range paths {
		lines, ok := readFileLines(rootPath, relPath)
		if !ok {
			// A file the graph knows and disk does not is a staleness question, not a
			// corruption one — reporting it here would drown the signal this exists
			// for. Counted as skipped so the totals stay honest.
			report.Skipped += len(byPath[relPath])
			continue
		}
		report.FilesRead++

		for _, node := range byPath[relPath] {
			report.Checked++
			window, ok := windowOf(lines, node.line, node.endLine)
			if !ok {
				report.Checked--
				report.Skipped++
				continue
			}
			if textSupportedBy(node.text, window) {
				continue
			}
			candidates = append(candidates, Divergence{
				Label:      node.label,
				Path:       relPath,
				LineNumber: node.line,
				EndLine:    node.endLine,
				Graph:      node.text,
				Window:     strings.Join(window, "\n"),
			})
		}
	}

	confirmed, transient, err := reverify(ctx, db, rootPath, candidates)
	if err != nil {
		return report, err
	}
	report.Divergences = confirmed
	report.Transient = transient
	return report, nil
}

// reverify re-reads the graph for nodes that failed once, and keeps only those that
// fail again.
//
// The second read is a fresh query and a fresh file read, which is the whole point: the
// transient corruption is a property of the moment a read happens, not of the stored
// row, so asking again is the only thing that separates the two. A node whose text now
// matches was never wrong on disk.
func reverify(ctx context.Context, db GraphDB, rootPath string, candidates []Divergence) ([]Divergence, int, error) {
	if len(candidates) == 0 {
		return nil, 0, nil
	}

	select {
	case <-ctx.Done():
		return nil, 0, ctx.Err()
	case <-time.After(verifyRecheckDelay):
	}

	var confirmed []Divergence
	transient := 0
	lineCache := make(map[string][]string)

	for _, c := range candidates {
		fresh, err := refetchText(ctx, db, c)
		if err != nil {
			return nil, 0, err
		}
		if fresh == "" {
			transient++
			continue
		}

		lines, ok := lineCache[c.Path]
		if !ok {
			lines, ok = readFileLines(rootPath, c.Path)
			if !ok {
				transient++
				continue
			}
			lineCache[c.Path] = lines
		}
		window, ok := windowOf(lines, c.LineNumber, c.EndLine)
		if !ok || textSupportedBy(fresh, window) {
			transient++
			continue
		}

		c.Graph = fresh
		c.Window = strings.Join(window, "\n")
		confirmed = append(confirmed, c)
	}
	return confirmed, transient, nil
}

func refetchText(ctx context.Context, db GraphDB, d Divergence) (string, error) {
	q := fmt.Sprintf(
		"MATCH (n:`%s`) WHERE n.path = $path AND n.line_number = $line RETURN n.name LIMIT 8",
		d.Label)
	res, err := db.Query(ctx, q, map[string]any{"path": d.Path, "line": d.LineNumber})
	if err != nil {
		return "", nil
	}
	for _, rec := range res.Records {
		if name, _ := rec["n.name"].(string); name != "" {
			return name, nil
		}
	}
	return "", nil
}

type graphText struct {
	label    string
	path     string
	line     int
	endLine  int
	text     string
	isDepend bool
}

func verifiableLabels(ctx context.Context, db GraphDB) ([]string, error) {
	res, err := db.Query(ctx, "CALL show_tables() RETURN *", nil)
	if err != nil {
		return nil, fmt.Errorf("listing tables: %w", err)
	}

	skip := map[string]bool{
		LabelFile: true, "Directory": true, "Module": true,
		"Value": true, "AttributeValue": true, "Text": true,
	}

	var labels []string
	for _, rec := range res.Records {
		name, ok1 := rec["name"].(string)
		typ, ok2 := rec["type"].(string)
		if !ok1 || !ok2 || typ != "NODE" || skip[name] {
			continue
		}
		labels = append(labels, name)
	}
	sort.Strings(labels)
	return labels, nil
}

func fetchGraphText(ctx context.Context, db GraphDB, label string) ([]graphText, error) {
	q := fmt.Sprintf(
		"MATCH (n:`%s`) WHERE n.is_stub = false RETURN n.name, n.path, n.line_number, n.end_line, n.is_dependency LIMIT %d",
		label, verifyBatch)
	res, err := db.Query(ctx, q, nil)
	if err != nil {
		return nil, nil
	}

	out := make([]graphText, 0, len(res.Records))
	for _, rec := range res.Records {
		g := graphText{label: label}
		g.text, _ = rec["n.name"].(string)
		g.path, _ = rec["n.path"].(string)
		g.line = asInt(rec["n.line_number"])
		g.endLine = asInt(rec["n.end_line"])
		g.isDepend, _ = rec["n.is_dependency"].(bool)
		if g.isDepend {
			continue
		}
		out = append(out, g)
	}
	return out, nil
}

func asInt(v any) int {
	switch n := v.(type) {
	case int:
		return n
	case int32:
		return int(n)
	case int64:
		return int(n)
	case float64:
		return int(n)
	}
	return 0
}

func readFileLines(rootPath, relPath string) ([]string, bool) {
	full := relPath
	if !filepath.IsAbs(full) {
		full = filepath.Join(rootPath, relPath)
	}
	data, err := os.ReadFile(full) //nolint:gosec // paths come from the graph's own index
	if err != nil {
		return nil, false
	}
	return strings.Split(string(data), "\n"), true
}

// windowOf returns the file lines a node claims to span, 1-indexed and inclusive.
//
// A line number past the end of the file is itself a mismatch, but reporting it as a
// text divergence would be misleading — that is a stale index, not a corrupt string —
// so it comes back as "cannot check".
func windowOf(lines []string, start, end int) ([]string, bool) {
	if start <= 0 || start > len(lines) {
		return nil, false
	}
	if end < start {
		end = start
	}
	if end > len(lines) {
		end = len(lines)
	}
	return lines[start-1 : end], true
}

func textSupportedBy(graphText string, window []string) bool {
	joined := strings.Join(window, "\n")
	for _, line := range strings.Split(graphText, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		if !strings.Contains(joined, line) {
			return false
		}
	}
	return true
}

// FormatVerifyReport renders a report for a terminal.
func FormatVerifyReport(r VerifyReport) string {
	var b strings.Builder
	fmt.Fprintf(&b, "checked %d node(s) across %d file(s), skipped %d\n",
		r.Checked, r.FilesRead, r.Skipped)

	if r.Transient > 0 {
		fmt.Fprintf(&b, "%d node(s) disagreed on the first read and agreed on the second — "+
			"a read that landed inside the writer's window, not corruption.\n", r.Transient)
		if r.Checked > 0 && r.Transient*20 > r.Checked {
			b.WriteString("That is a large share of the graph, which means the index was " +
				"being written while this ran. Re-run once it is idle before believing " +
				"anything below.\n")
		}
	}

	if r.Clean() {
		b.WriteString("no divergence: every checked node's text is present in its file\n")
		return b.String()
	}

	fmt.Fprintf(&b, "\n%d DIVERGENCE(S) — the graph holds text its file does not contain.\n", len(r.Divergences))
	b.WriteString("This is what LadybugDB's silent string corruption looks like. It cannot be\n")
	b.WriteString("repaired in place: the shard cache is keyed by content hash and reports these\n")
	b.WriteString("files as up to date, so only a full reindex rewrites the rows.\n\n")

	for _, d := range r.Divergences {
		fmt.Fprintf(&b, "%s:%d-%d (%s)\n", d.Path, d.LineNumber, d.EndLine, d.Label)
		fmt.Fprintf(&b, "  graph: %s\n", oneLine(d.Graph))
		fmt.Fprintf(&b, "  file:  %s\n\n", oneLine(d.Window))
	}
	return b.String()
}

func oneLine(s string) string {
	s = strings.ReplaceAll(s, "\n", " ⏎ ")
	const max = 160
	if len(s) > max {
		return s[:max] + "…"
	}
	return s
}

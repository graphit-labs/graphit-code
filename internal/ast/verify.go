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

// verifyBatch is how many nodes are pulled per query.
const verifyBatch = 20000

// verifyRecheckDelay is how long the second pass waits before re-reading.
//
// It is not politeness, it is the discriminator. The transient corruption is a property
// of reading while the writer is active, and an immediate re-read is still inside that
// same window — measured here: an in-process recheck with no pause confirmed 1873
// "divergences" over a graph that verified clean once the writer went idle. Asking
// again seconds later is what the failure mode actually responds to.
//
// A variable rather than a constant so tests can drop it to zero; nothing else writes it.
var verifyRecheckDelay = 3 * time.Second

// VerifyGraphAgainstDisk checks that the text the graph holds is the text on disk.
//
// It exists for a failure mode nothing else here can see. LadybugDB's string
// corruption has a SILENT form — a wrong (offset, length) pair that lands on the valid
// text of ANOTHER row — and the value that comes back is well-formed UTF-8, internally
// consistent with the rest of its row, and simply wrong. Nothing raises: not the write,
// not the read, not the FTS build. The one occurrence caught so far was caught by a
// reviewer who happened to know that a comment about Cypher could not live in a React
// component.
//
// The existing check — `MATCH (n:<Label>) RETURN count(toLower(n.name))` per label —
// only finds INVALID UTF-8, so it passes clean over a graph corrupted this way. That is
// not a weaker version of this check; it is a check of something else.
//
// The method is the only one available: compare against the file. Text that came FROM a
// file can be verified against that file without reparsing anything, which is why
// comments are the primary subject — a Comment's name IS the source text — and why
// declarations are the secondary one: a declaration's name must appear on the line the
// graph says it is on.
//
// This DETECTS; it cannot repair. The defect is upstream and open, and `graphit sync`
// will not fix it either: the shard cache is keyed by content hash, so it reports the
// intact file as up to date and never rewrites the row. Only a reindex or a reset does.
//
// TWO PASSES, and the second one is what makes this trustworthy. The same corruption
// has a TRANSIENT form: a read landing inside the daemon's write window comes back with
// another row's text and then agrees with disk seconds later. Measured here — a first
// pass right after a sync reported 1915 divergences over a graph that was intact, and
// the very next pass reported none. A probe that shouted on that would be switched off
// within a week, and the durable case would then have nowhere to appear. So anything
// that fails is re-read and re-compared; only what fails BOTH times is reported as a
// divergence, and what recovers is counted as transient and named as such.
func VerifyGraphAgainstDisk(ctx context.Context, db GraphDB, rootPath string) (VerifyReport, error) {
	var report VerifyReport

	labels, err := verifiableLabels(ctx, db)
	if err != nil {
		return report, err
	}
	if len(labels) == 0 {
		return report, nil
	}

	// One file is read once and checked against every node claiming to come from it.
	// Grouping by path is what keeps this to seconds on a corpus of tens of thousands
	// of nodes: the cost is the file reads, not the comparisons.
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

	// One pause for the whole batch, not one per node: what has to change between the
	// two reads is the writer's state, and that is the same for every candidate.
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
			// The node is gone between the two passes — a reindex swapped the
			// database under us. Not a claim this probe can make either way.
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

// refetchText reads one node's text again, addressed by everything that identifies it.
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

// verifiableLabels are the node labels whose name can be checked against a file.
//
// Derived from the live schema rather than listed here, so a grammar that introduces a
// label gets covered without touching this file. Three groups are excluded, each for a
// reason that would otherwise turn the probe into noise:
//
//   - File, Directory and Module name a path or a dependency, not a span of source.
//   - Value, AttributeValue and Text are named after their own content, and that
//     content goes through dataText, which unquotes and truncates it. A truncated name
//     is not in the file verbatim, and would be reported as a divergence forever.
//     Comment is content-named too but keeps its text intact, so it stays in.
//   - anything the graph holds no text for is filtered per row, not per label.
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
		// Relationship tables answer show_tables() too, and have no name column.
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
	// is_stub is excluded in the query rather than filtered afterwards: a stub has no
	// path and no line by construction, so it is not a thing disk can confirm or deny.
	q := fmt.Sprintf(
		"MATCH (n:`%s`) WHERE n.is_stub = false RETURN n.name, n.path, n.line_number, n.end_line, n.is_dependency LIMIT %d",
		label, verifyBatch)
	res, err := db.Query(ctx, q, nil)
	if err != nil {
		// A label with no such property set is not an error worth aborting the whole
		// pass for — the next label may still be checkable.
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

// textSupportedBy reports whether the file window can account for the graph's text.
//
// Containment per line rather than equality, and deliberately so. A comment's stored
// text has been through cleanDocstring — markers stripped, blank lines dropped, lines
// joined — while a trailing comment shares its line with code, and an entity's name is
// one token on a line of many. Equality would report all of those, every run, and a
// probe that cries wolf is worse than no probe: it gets switched off, and then the one
// real divergence has nowhere to appear.
//
// What it does catch is the failure it exists for. Text borrowed from another row does
// not appear on these lines at all, so every one of its lines fails containment at once.
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

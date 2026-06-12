package wiki

import (
	"context"
	"math"
	"regexp"
	"strings"

	sitter "github.com/smacker/go-tree-sitter"
	markdown "github.com/smacker/go-tree-sitter/markdown/tree-sitter-markdown"
)

// ChunkOpts controls how markdown content is split into semantic chunks.
type ChunkOpts struct {
	MaxTokens int    // Target max tokens per chunk (default 512)
	MinTokens int    // Minimum tokens before merging with parent (default 32)
	DocTitle  string // Title of the parent document
	DocSlug   string // Slug for cross-ref tracking
}

// SemanticChunk represents a single semantic unit extracted from a markdown document.
type SemanticChunk struct {
	Title      string   // Section title (from heading) or parent doc title for intro
	Body       string   // Content of the chunk (markdown text)
	Summary    string   // First meaningful line as summary
	Level      int      // Heading level (1-6), 0 for intro/paragraph chunks
	Breadcrumb string   // Hierarchical path: "Doc Title > Section > Subsection"
	ParentIdx  int      // Index of parent chunk (-1 for root)
	Children   []int    // Indices of child chunks
	NodeType   string   // "section", "code_block", "intro"
	CrossRefs  []string // Wikilinks found in this chunk: [[slug]] → slug
	StartByte  uint32   // Position in original document
	EndByte    uint32
}

// sectionNode is an intermediate representation of a heading-delimited section.
type sectionNode struct {
	title    string
	level    int
	startIdx int // index into flat list of document children
	endIdx   int // exclusive
	children []*sectionNode
	parent   *sectionNode
}

var reWikiLink = regexp.MustCompile(`\[\[([^\]]+)\]\]`)

// docChild represents a direct child node of the markdown document root,
// annotated with heading metadata when applicable.
type docChild struct {
	node     *sitter.Node
	nodeType string
	level    int    // heading level if atx_heading/setext_heading, 0 otherwise
	headText string // heading text if heading node
}

// ChunkMarkdown parses markdown content using tree-sitter and splits it into
// semantic chunks suitable for search indexing and LLM retrieval.
func ChunkMarkdown(content string, opts ChunkOpts) ([]SemanticChunk, error) {
	if opts.MaxTokens <= 0 {
		opts.MaxTokens = 512
	}
	if opts.MinTokens <= 0 {
		opts.MinTokens = 32
	}

	content = skipFrontmatter(content)

	if strings.TrimSpace(content) == "" {
		return nil, nil
	}

	src := []byte(content)

	parser := sitter.NewParser()
	parser.SetLanguage(markdown.GetLanguage())

	tree, err := parser.ParseCtx(context.Background(), nil, src)
	if err != nil {
		return nil, err
	}
	if tree == nil {
		return nil, nil
	}
	defer tree.Close()

	root := tree.RootNode()
	if root == nil {
		return nil, nil
	}

	// Flatten the AST into a linear sequence of content nodes.
	// tree-sitter-markdown wraps content in nested `section` nodes, so we
	// recursively unwrap them to produce a flat list of headings, paragraphs,
	// code blocks, etc. that buildSectionTree can process.
	children := flattenSections(root, src)
	if len(children) == 0 {
		return nil, nil
	}

	// Build a hierarchical section tree from the flat heading sequence.
	sections, introEnd := buildSectionTree(children)

	var chunks []SemanticChunk

	// Create intro chunk from content before the first heading.
	if introEnd > 0 {
		var body strings.Builder
		var startByte, endByte uint32
		first := true
		for i := 0; i < introEnd; i++ {
			c := children[i]
			text := nodeContent(src, c.node.StartByte(), c.node.EndByte())
			if text == "" {
				continue
			}
			if first {
				startByte = c.node.StartByte()
				first = false
			}
			endByte = c.node.EndByte()
			body.WriteString(text)
			body.WriteByte('\n')
		}
		bodyStr := strings.TrimRight(body.String(), "\n")
		if strings.TrimSpace(bodyStr) != "" {
			chunk := SemanticChunk{
				Title:      opts.DocTitle,
				Body:       bodyStr,
				Summary:    extractChunkSummary(bodyStr),
				Level:      0,
				Breadcrumb: opts.DocTitle,
				ParentIdx:  -1,
				NodeType:   "intro",
				CrossRefs:  extractWikiLinks(bodyStr),
				StartByte:  startByte,
				EndByte:    endByte,
			}
			chunks = append(chunks, chunk)
		}
	}

	// If there are no heading-delimited sections and we already have an intro,
	// return early.
	if len(sections) == 0 {
		if len(chunks) == 0 {
			// Entire document had no headings and intro was empty — shouldn't
			// happen since we checked for empty content above, but be safe.
			bodyStr := strings.TrimSpace(content)
			if bodyStr != "" {
				chunks = append(chunks, SemanticChunk{
					Title:      opts.DocTitle,
					Body:       bodyStr,
					Summary:    extractChunkSummary(bodyStr),
					Level:      0,
					Breadcrumb: opts.DocTitle,
					ParentIdx:  -1,
					NodeType:   "intro",
					CrossRefs:  extractWikiLinks(bodyStr),
					StartByte:  0,
					EndByte:    uint32(len(src)),
				})
			}
		}
		return chunks, nil
	}

	// Recursively emit chunks from the section tree.
	var headingStack []string
	if opts.DocTitle != "" {
		headingStack = append(headingStack, opts.DocTitle)
	}

	introIdx := -1
	if len(chunks) > 0 {
		introIdx = 0
	}

	emitSections(sections, children, src, opts, headingStack, introIdx, &chunks)

	// Wire up parent/child relationships from the section tree.
	wireParentChild(&chunks)

	return chunks, nil
}

// emitSections recursively converts sectionNode tree into SemanticChunks.
func emitSections(
	sections []*sectionNode,
	children []docChild,
	src []byte,
	opts ChunkOpts,
	headingStack []string,
	parentChunkIdx int,
	chunks *[]SemanticChunk,
) {
	for _, sec := range sections {
		stack := append(headingStack, sec.title)
		breadcrumb := strings.Join(stack, " > ")

		// Collect body content: all non-heading children in this section's range,
		// excluding children that belong to sub-sections.
		ownBody, startByte, endByte := collectSectionBody(sec, children, src)

		myIdx := len(*chunks)

		chunk := SemanticChunk{
			Title:      sec.title,
			Body:       ownBody,
			Summary:    extractChunkSummary(ownBody),
			Level:      sec.level,
			Breadcrumb: breadcrumb,
			ParentIdx:  parentChunkIdx,
			NodeType:   "section",
			CrossRefs:  extractWikiLinks(ownBody),
			StartByte:  startByte,
			EndByte:    endByte,
		}

		tokens := estimateTokens(ownBody)

		// If the section body is too small and has no children, it will get
		// merged by the post-processing step (wireParentChild handles
		// MinTokens merging). For now, always emit it.
		if tokens > opts.MaxTokens && len(sec.children) == 0 {
			// Split large leaf sections at paragraph/list boundaries.
			subChunks := splitLargeSection(ownBody, sec, breadcrumb, opts, parentChunkIdx, startByte)
			*chunks = append(*chunks, subChunks...)
		} else {
			*chunks = append(*chunks, chunk)

			if len(sec.children) > 0 {
				emitSections(sec.children, children, src, opts, stack, myIdx, chunks)
			}
		}
	}
}

// splitLargeSection splits a leaf section that exceeds MaxTokens into
// sub-chunks at paragraph and list_item boundaries, keeping atomic nodes intact.
func splitLargeSection(
	body string,
	sec *sectionNode,
	breadcrumb string,
	opts ChunkOpts,
	parentChunkIdx int,
	baseStartByte uint32,
) []SemanticChunk {
	// Split on double newlines to approximate paragraph boundaries.
	paragraphs := splitIntoParagraphs(body)

	var chunks []SemanticChunk
	var accum strings.Builder
	accumTokens := 0
	accumStart := baseStartByte

	flush := func() {
		text := strings.TrimSpace(accum.String())
		if text == "" {
			return
		}
		endByte := accumStart + uint32(len(text))
		chunks = append(chunks, SemanticChunk{
			Title:      sec.title,
			Body:       text,
			Summary:    extractChunkSummary(text),
			Level:      sec.level,
			Breadcrumb: breadcrumb,
			ParentIdx:  parentChunkIdx,
			NodeType:   "section",
			CrossRefs:  extractWikiLinks(text),
			StartByte:  accumStart,
			EndByte:    endByte,
		})
		accumStart = endByte
		accum.Reset()
		accumTokens = 0
	}

	for _, para := range paragraphs {
		paraTokens := estimateTokens(para)

		// If this single paragraph is atomic/large on its own, flush what we
		// have and emit it as its own chunk.
		if paraTokens > opts.MaxTokens {
			flush()
			endByte := accumStart + uint32(len(para))
			chunks = append(chunks, SemanticChunk{
				Title:      sec.title,
				Body:       para,
				Summary:    extractChunkSummary(para),
				Level:      sec.level,
				Breadcrumb: breadcrumb,
				ParentIdx:  parentChunkIdx,
				NodeType:   "section",
				CrossRefs:  extractWikiLinks(para),
				StartByte:  accumStart,
				EndByte:    endByte,
			})
			accumStart = endByte
			continue
		}

		if accumTokens+paraTokens > opts.MaxTokens && accumTokens > 0 {
			flush()
		}

		if accum.Len() > 0 {
			accum.WriteString("\n\n")
		}
		accum.WriteString(para)
		accumTokens += paraTokens
	}
	flush()

	return chunks
}

// collectSectionBody gathers the text content of a section's own nodes
// (excluding sub-section content).
func collectSectionBody(
	sec *sectionNode,
	children []docChild,
	src []byte,
) (string, uint32, uint32) {
	// Determine which child indices are owned by sub-sections.
	subRanges := make(map[int]bool)
	for _, child := range sec.children {
		for i := child.startIdx; i < child.endIdx; i++ {
			subRanges[i] = true
		}
	}

	var body strings.Builder
	var startByte, endByte uint32
	first := true

	for i := sec.startIdx; i < sec.endIdx; i++ {
		if subRanges[i] {
			continue
		}

		c := children[i]
		// Skip the heading node itself for this section — its text is in Title.
		if i == sec.startIdx && (c.nodeType == "atx_heading" || c.nodeType == "setext_heading") {
			if first {
				startByte = c.node.StartByte()
				first = false
			}
			endByte = c.node.EndByte()
			continue
		}

		text := nodeContent(src, c.node.StartByte(), c.node.EndByte())
		if text == "" {
			continue
		}
		if first {
			startByte = c.node.StartByte()
			first = false
		}
		endByte = c.node.EndByte()
		body.WriteString(text)
		body.WriteByte('\n')
	}

	return strings.TrimRight(body.String(), "\n"), startByte, endByte
}

// buildSectionTree constructs a hierarchical tree of sections from the flat
// list of document children. Returns the section roots and the index of the
// first heading (i.e., how many children are "intro" content).
func buildSectionTree(children []docChild) ([]*sectionNode, int) {
	// Find index of first heading.
	introEnd := len(children)
	for i, c := range children {
		if c.level > 0 {
			introEnd = i
			break
		}
	}

	if introEnd == len(children) {
		// No headings at all.
		return nil, introEnd
	}

	// First pass: create sectionNode for each heading, determining its range
	// (from this heading to the next same-or-higher level heading).
	var allSections []*sectionNode
	for i := introEnd; i < len(children); i++ {
		c := children[i]
		if c.level == 0 {
			continue
		}
		sec := &sectionNode{
			title:    c.headText,
			level:    c.level,
			startIdx: i,
		}
		allSections = append(allSections, sec)
	}

	// Set endIdx for each section.
	for i, sec := range allSections {
		if i+1 < len(allSections) {
			sec.endIdx = allSections[i+1].startIdx
		} else {
			sec.endIdx = len(children)
		}
	}

	// Build hierarchy: a section's parent is the nearest preceding section
	// with a strictly lower (higher priority) heading level.
	var roots []*sectionNode
	var stack []*sectionNode

	for _, sec := range allSections {
		// Pop stack until we find a section with a strictly lower level.
		for len(stack) > 0 && stack[len(stack)-1].level >= sec.level {
			stack = stack[:len(stack)-1]
		}

		if len(stack) == 0 {
			roots = append(roots, sec)
		} else {
			parent := stack[len(stack)-1]
			parent.children = append(parent.children, sec)
			sec.parent = parent
		}

		stack = append(stack, sec)
	}

	// Adjust endIdx: a parent section's range should encompass its children,
	// but the children's content should not duplicate in the parent body.
	// The endIdx is already correct from the linear scan above.

	return roots, introEnd
}

// wireParentChild sets Children slices and performs MinTokens merging.
func wireParentChild(chunks *[]SemanticChunk) {
	if len(*chunks) == 0 {
		return
	}

	// Build Children from ParentIdx.
	for i := range *chunks {
		(*chunks)[i].Children = nil
	}
	for i, c := range *chunks {
		if c.ParentIdx >= 0 && c.ParentIdx < len(*chunks) {
			parent := &(*chunks)[c.ParentIdx]
			parent.Children = append(parent.Children, i)
		}
	}
}

// estimateTokens roughly estimates token count. Uses words * 1.33 (i.e., ÷ 0.75).
func estimateTokens(text string) int {
	words := len(strings.Fields(text))
	return int(math.Ceil(float64(words) / 0.75))
}

// extractHeadingLevel returns the heading level (1-6) from an atx_heading or
// setext_heading node.
func extractHeadingLevel(node *sitter.Node, src []byte) int {
	if node == nil {
		return 0
	}

	ntype := node.Type()

	if ntype == "atx_heading" {
		// Look for atx_h1_marker through atx_h6_marker among children.
		count := int(node.ChildCount())
		for i := 0; i < count; i++ {
			child := node.Child(i)
			if child == nil {
				continue
			}
			ct := child.Type()
			switch ct {
			case "atx_h1_marker":
				return 1
			case "atx_h2_marker":
				return 2
			case "atx_h3_marker":
				return 3
			case "atx_h4_marker":
				return 4
			case "atx_h5_marker":
				return 5
			case "atx_h6_marker":
				return 6
			}
		}
		// Fallback: count leading # characters.
		text := nodeContent(src, node.StartByte(), node.EndByte())
		level := 0
		for _, ch := range text {
			if ch == '#' {
				level++
			} else {
				break
			}
		}
		if level >= 1 && level <= 6 {
			return level
		}
		return 1
	}

	if ntype == "setext_heading" {
		// Setext headings: = underline → H1, - underline → H2.
		text := nodeContent(src, node.StartByte(), node.EndByte())
		lines := strings.Split(text, "\n")
		for i := len(lines) - 1; i >= 0; i-- {
			trimmed := strings.TrimSpace(lines[i])
			if trimmed == "" {
				continue
			}
			if strings.Count(trimmed, "=") == len(trimmed) && len(trimmed) > 0 {
				return 1
			}
			if strings.Count(trimmed, "-") == len(trimmed) && len(trimmed) > 0 {
				return 2
			}
			break
		}
		return 1
	}

	return 0
}

// extractHeadingText returns the text content of a heading (without the # markers
// or setext underlines).
func extractHeadingText(node *sitter.Node, src []byte) string {
	if node == nil {
		return ""
	}

	ntype := node.Type()

	if ntype == "atx_heading" {
		// Find the inline or heading_content child.
		count := int(node.ChildCount())
		for i := 0; i < count; i++ {
			child := node.Child(i)
			if child == nil {
				continue
			}
			ct := child.Type()
			// Skip marker nodes.
			if strings.HasPrefix(ct, "atx_h") && strings.HasSuffix(ct, "_marker") {
				continue
			}
			// The remaining child is the heading content.
			text := nodeContent(src, child.StartByte(), child.EndByte())
			return strings.TrimSpace(text)
		}
		// Fallback: strip leading # and space.
		text := nodeContent(src, node.StartByte(), node.EndByte())
		text = strings.TrimLeft(text, "#")
		return strings.TrimSpace(text)
	}

	if ntype == "setext_heading" {
		// First line is the heading text; subsequent lines are underlines.
		text := nodeContent(src, node.StartByte(), node.EndByte())
		lines := strings.SplitN(text, "\n", 2)
		return strings.TrimSpace(lines[0])
	}

	return strings.TrimSpace(nodeContent(src, node.StartByte(), node.EndByte()))
}

// nodeContent extracts the text for a range of bytes.
func nodeContent(src []byte, startByte, endByte uint32) string {
	if int(startByte) >= len(src) {
		return ""
	}
	if int(endByte) > len(src) {
		endByte = uint32(len(src))
	}
	return string(src[startByte:endByte])
}

// extractWikiLinks finds all [[slug]] references in text and returns
// deduplicated, resolved slugs.
func extractWikiLinks(text string) []string {
	matches := reWikiLink.FindAllStringSubmatch(text, -1)
	if len(matches) == 0 {
		return nil
	}
	seen := make(map[string]bool, len(matches))
	var result []string
	for _, m := range matches {
		slug := ResolveSlug(m[1])
		if slug != "" && !seen[slug] {
			seen[slug] = true
			result = append(result, slug)
		}
	}
	return result
}

// extractChunkSummary returns the first meaningful line of text — skipping
// empty lines and heading markers.
func extractChunkSummary(body string) string {
	for _, line := range strings.Split(body, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		// Skip heading lines.
		if strings.HasPrefix(line, "#") {
			continue
		}
		// Skip setext underlines.
		trimmed := strings.TrimLeft(line, "=-")
		if trimmed == "" {
			continue
		}
		// Truncate long summary lines.
		if len(line) > 200 {
			return line[:200] + "…"
		}
		return line
	}
	return ""
}

// isAtomicNode returns true for nodes that should never be split.
func isAtomicNode(nodeType string) bool {
	switch nodeType {
	case "fenced_code_block", "html_block", "table", "block_quote":
		return true
	}
	return false
}

// skipFrontmatter removes YAML frontmatter (--- delimited) from the beginning
// of the content. This reuses the same logic as stripYAMLFrontmatter but is
// named to avoid confusion within this file.
func skipFrontmatter(content string) string {
	return stripYAMLFrontmatter(content)
}

// flattenSections recursively walks the tree-sitter AST and produces a flat
// sequence of docChild entries. The tree-sitter-markdown grammar wraps headings
// and their content in nested `section` nodes. This function peels those
// wrappers so that the result is a linear list of heading and content nodes
// suitable for buildSectionTree.
func flattenSections(node *sitter.Node, src []byte) []docChild {
	if node == nil {
		return nil
	}
	var result []docChild
	count := int(node.ChildCount())
	for i := 0; i < count; i++ {
		child := node.Child(i)
		if child == nil {
			continue
		}
		ntype := child.Type()

		if ntype == "section" {
			// Unwrap section — recurse into its children.
			result = append(result, flattenSections(child, src)...)
			continue
		}

		dc := docChild{node: child, nodeType: ntype}
		if ntype == "atx_heading" || ntype == "setext_heading" {
			dc.level = extractHeadingLevel(child, src)
			dc.headText = extractHeadingText(child, src)
		}
		result = append(result, dc)
	}
	return result
}

// splitIntoParagraphs splits body text into paragraph-like segments at double
// newline boundaries, preserving atomic blocks (fenced code blocks, etc.) intact.
func splitIntoParagraphs(body string) []string {
	var segments []string
	lines := strings.Split(body, "\n")

	var current strings.Builder
	inFenced := false

	flush := func() {
		text := strings.TrimSpace(current.String())
		if text != "" {
			segments = append(segments, text)
		}
		current.Reset()
	}

	for i := 0; i < len(lines); i++ {
		line := lines[i]
		trimmed := strings.TrimSpace(line)

		// Track fenced code block boundaries.
		if strings.HasPrefix(trimmed, "```") || strings.HasPrefix(trimmed, "~~~") {
			if !inFenced {
				// Starting a fenced block — flush what we have first.
				flush()
				inFenced = true
				current.WriteString(line)
				current.WriteByte('\n')
				continue
			}
			// Ending a fenced block.
			current.WriteString(line)
			current.WriteByte('\n')
			inFenced = false
			flush()
			continue
		}

		if inFenced {
			current.WriteString(line)
			current.WriteByte('\n')
			continue
		}

		// Outside of fenced blocks, double newline = paragraph boundary.
		if trimmed == "" {
			if current.Len() > 0 {
				flush()
			}
			continue
		}

		current.WriteString(line)
		current.WriteByte('\n')
	}

	flush()
	return segments
}

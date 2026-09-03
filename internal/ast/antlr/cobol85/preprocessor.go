// Package cobol85 — COBOL 85 preprocessor.
//
// This is a faithful Go port of the proleap COBOL preprocessor from
// https://github.com/antlr/grammars-v4/tree/master/cobol85/Java/io/proleap/cobol/preprocessor
//
// The pipeline is:
//
//  1. LineReader — parse raw text into CobolLine structs using the FIXED-format regex
//  2. LineIndicatorProcessor — process indicator column (continuation, comments, debug)
//  3. CommentEntriesMarker — handle multi-line comment entries (AUTHOR., DATE-WRITTEN., etc.)
//  4. LineWriter — serialize processed lines back to a single string
//  5. DocumentParser — use Cobol85Preprocessor.g4 to parse COPY/REPLACE/EXEC statements
//
// Step 5 (DocumentParser) requires copybook file resolution which is not available in
// our AST indexing context. We implement steps 1–4 faithfully and skip COPY expansion
// (which is consistent with parsing standalone files).
package cobol85

import (
	"regexp"
	"strings"
)

const (
	charAsterisk = "*"
	charD        = "D"
	charDLower   = "d"
	charMinus    = "-"
	charSlash    = "/"
	commentTag   = ">*"
	ws           = " "
	newline      = "\n"
)

// CobolSourceFormat represents the COBOL source format.
type CobolSourceFormat int

const (
	// FormatFixed is standard ANSI / IBM fixed format (80 cols):
	//   1-6: sequence area, 7: indicator, 8-12: area A, 13-72: area B, 73-80: comment
	FormatFixed CobolSourceFormat = iota
	// FormatVariable is like FIXED but area B extends past column 72.
	FormatVariable
	// FormatTandem has no sequence area: 1: indicator, 2-5: area A, 6-132: area B.
	FormatTandem
)

const indicatorField = `([ABCdD\t\-/*# ])`

var (
	fixedPattern    = regexp.MustCompile(`(.{6})` + indicatorField + `(.{0,4})(.{0,61})(.*)`)
	variablePattern = regexp.MustCompile(`(.{6})(?:` + indicatorField + `(.{0,4})(.*)()?)?`)
	tandemPattern   = regexp.MustCompile(`()` + indicatorField + `(.{0,4})(.*)()`)
)

func patternForFormat(f CobolSourceFormat) *regexp.Regexp {
	switch f {
	case FormatFixed:
		return fixedPattern
	case FormatVariable:
		return variablePattern
	case FormatTandem:
		return tandemPattern
	default:
		return fixedPattern
	}
}

func isCommentEntryMultiLine(f CobolSourceFormat) bool {
	return f != FormatTandem
}

// CobolLineType represents the type of a COBOL source line.
type CobolLineType int

const (
	LineBlank        CobolLineType = iota
	LineComment      CobolLineType = iota
	LineContinuation CobolLineType = iota
	LineDebug        CobolLineType = iota
	LineNormal       CobolLineType = iota
)

// CobolLine represents a parsed COBOL source line.
type CobolLine struct {
	SequenceArea  string
	IndicatorArea string
	ContentAreaA  string
	ContentAreaB  string
	Comment       string
	Format        CobolSourceFormat
	Number        int
	Type          CobolLineType
}

// GetContentArea returns the concatenation of content areas A and B.
func (l *CobolLine) GetContentArea() string {
	return l.ContentAreaA + l.ContentAreaB
}

// BlankSequenceArea returns a blank sequence area for the given format.
func BlankSequenceArea(format CobolSourceFormat) string {
	if format == FormatTandem {
		return ""
	}
	return strings.Repeat(ws, 6)
}

// WithIndicatorAndContent creates a new CobolLine with the given indicator and content area.
func WithIndicatorAndContent(line *CobolLine, indicatorArea, contentArea string) *CobolLine {
	areaA, areaB := splitContentArea(contentArea)
	return &CobolLine{
		SequenceArea:  line.SequenceArea,
		IndicatorArea: indicatorArea,
		ContentAreaA:  areaA,
		ContentAreaB:  areaB,
		Comment:       line.Comment,
		Format:        line.Format,
		Number:        line.Number,
		Type:          line.Type,
	}
}

// WithContentArea creates a new CobolLine with a new content area but the same indicator.
func WithContentArea(line *CobolLine, contentArea string) *CobolLine {
	areaA, areaB := splitContentArea(contentArea)
	return &CobolLine{
		SequenceArea:  line.SequenceArea,
		IndicatorArea: line.IndicatorArea,
		ContentAreaA:  areaA,
		ContentAreaB:  areaB,
		Comment:       line.Comment,
		Format:        line.Format,
		Number:        line.Number,
		Type:          line.Type,
	}
}

func splitContentArea(contentArea string) (string, string) {
	if len(contentArea) > 4 {
		return contentArea[:4], contentArea[4:]
	}
	return contentArea, ""
}

func determineType(indicatorArea string) CobolLineType {
	switch indicatorArea {
	case charD, charDLower:
		return LineDebug
	case charMinus:
		return LineContinuation
	case charAsterisk, charSlash:
		return LineComment
	default:
		return LineNormal
	}
}

func parseLine(line string, lineNumber int, format CobolSourceFormat) *CobolLine {
	if strings.TrimSpace(line) == "" {
		return &CobolLine{
			SequenceArea:  BlankSequenceArea(format),
			IndicatorArea: ws,
			ContentAreaA:  "",
			ContentAreaB:  "",
			Comment:       "",
			Format:        format,
			Number:        lineNumber,
			Type:          LineBlank,
		}
	}

	pattern := patternForFormat(format)
	matches := pattern.FindStringSubmatch(line)

	if matches == nil {
		return &CobolLine{
			SequenceArea:  BlankSequenceArea(format),
			IndicatorArea: ws,
			ContentAreaA:  "",
			ContentAreaB:  line,
			Comment:       "",
			Format:        format,
			Number:        lineNumber,
			Type:          LineNormal,
		}
	}

	sequenceArea := safeGroup(matches, 1)
	indicatorArea := safeGroup(matches, 2)
	contentAreaA := safeGroup(matches, 3)
	contentAreaB := safeGroup(matches, 4)
	commentArea := safeGroup(matches, 5)

	if indicatorArea == "" {
		indicatorArea = ws
	}

	lineType := determineType(indicatorArea)

	return &CobolLine{
		SequenceArea:  sequenceArea,
		IndicatorArea: indicatorArea,
		ContentAreaA:  contentAreaA,
		ContentAreaB:  contentAreaB,
		Comment:       commentArea,
		Format:        format,
		Number:        lineNumber,
		Type:          lineType,
	}
}

func safeGroup(matches []string, idx int) string {
	if idx < len(matches) {
		return matches[idx]
	}
	return ""
}

func readLines(cobolCode string, format CobolSourceFormat) []*CobolLine {
	scanner := strings.Split(cobolCode, "\n")
	result := make([]*CobolLine, 0, len(scanner))

	for i, line := range scanner {
		line = strings.TrimRight(line, "\r")
		parsed := parseLine(line, i, format)
		result = append(result, parsed)
	}

	return result
}

var trailingWhitespace = regexp.MustCompile(`\s+$`)

func handleTrailingComma(contentArea string) string {
	if contentArea == "" {
		return contentArea
	}
	last := contentArea[len(contentArea)-1]
	if last == ',' || last == ';' {
		return contentArea + ws
	}
	return contentArea
}

func processLineIndicator(line *CobolLine) *CobolLine {
	trimmedTrailWs := trailingWhitespace.ReplaceAllString(line.GetContentArea(), "")
	handled := handleTrailingComma(trimmedTrailWs)

	switch line.Type {
	case LineDebug:
		return WithIndicatorAndContent(line, ws, handled)

	case LineContinuation:
		trimmed := strings.TrimSpace(handled)
		if len(trimmed) > 0 {
			first := trimmed[0]
			if first == '"' || first == '\'' {
				return WithIndicatorAndContent(line, ws, trimmed[1:])
			}
		}
		return WithIndicatorAndContent(line, ws, trimmed)

	case LineComment:
		return WithIndicatorAndContent(line, commentTag+ws, handled)

	default:
		return WithIndicatorAndContent(line, ws, handled)
	}
}

func processLineIndicators(lines []*CobolLine) []*CobolLine {
	result := make([]*CobolLine, len(lines))
	for i, line := range lines {
		result[i] = processLineIndicator(line)
	}
	return result
}

var (
	commentEntryTriggersStart = []string{
		"AUTHOR.", "INSTALLATION.", "DATE-WRITTEN.",
		"DATE-COMPILED.", "SECURITY.", "REMARKS.",
	}
	commentEntryTriggersEnd = []string{
		"PROGRAM-ID.", "AUTHOR.", "INSTALLATION.", "DATE-WRITTEN.",
		"DATE-COMPILED.", "SECURITY.", "ENVIRONMENT", "DATA.", "PROCEDURE.",
	}
	commentEntryTriggerPattern *regexp.Regexp
)

const commentEntryTag = ">*CE"

func init() {
	escaped := make([]string, len(commentEntryTriggersStart))
	for i, t := range commentEntryTriggersStart {
		escaped[i] = regexp.QuoteMeta(t)
	}
	commentEntryTriggerPattern = regexp.MustCompile(
		"(?i)(" + strings.Join(escaped, "|") + ")(.+)")
}

func startsWithTrigger(line *CobolLine, triggers []string) bool {
	content := strings.ToUpper(line.GetContentArea())
	for _, trigger := range triggers {
		if strings.HasPrefix(content, trigger) {
			return true
		}
	}
	return false
}

func escapeCommentEntry(line *CobolLine) *CobolLine {
	matcher := commentEntryTriggerPattern.FindStringSubmatch(line.GetContentArea())
	if matcher != nil {
		trigger := matcher[1]
		commentEntry := matcher[2]
		newContentArea := trigger + ws + commentEntryTag + commentEntry
		return WithContentArea(line, newContentArea)
	}
	return line
}

func markCommentEntries(lines []*CobolLine, format CobolSourceFormat) []*CobolLine {
	result := make([]*CobolLine, len(lines))

	foundTriggerInPrevious := false
	isInCommentEntry := false

	for i, line := range lines {
		if isCommentEntryMultiLine(format) {
			foundTriggerInCurrent := startsWithTrigger(line, commentEntryTriggersStart)

			if foundTriggerInCurrent {
				result[i] = escapeCommentEntry(line)
			} else if foundTriggerInPrevious || isInCommentEntry {
				isContentAreaAEmpty := strings.TrimSpace(line.ContentAreaA) == ""
				isInCommentEntry = (line.Type == LineComment || isContentAreaAEmpty) &&
					!startsWithTrigger(line, commentEntryTriggersEnd)

				if isInCommentEntry {
					result[i] = &CobolLine{
						SequenceArea:  line.SequenceArea,
						IndicatorArea: commentEntryTag + ws,
						ContentAreaA:  line.ContentAreaA,
						ContentAreaB:  line.ContentAreaB,
						Comment:       line.Comment,
						Format:        line.Format,
						Number:        line.Number,
						Type:          line.Type,
					}
				} else {
					result[i] = line
				}
			} else {
				result[i] = line
			}

			foundTriggerInPrevious = foundTriggerInCurrent
		} else {
			foundTriggerInCurrent := startsWithTrigger(line, commentEntryTriggersStart)
			if foundTriggerInCurrent {
				result[i] = escapeCommentEntry(line)
			} else {
				result[i] = line
			}
		}
	}

	return result
}

func serializeLines(lines []*CobolLine) string {
	var sb strings.Builder
	sb.Grow(len(lines) * 80)

	for _, line := range lines {
		notContinuation := line.Type != LineContinuation

		if notContinuation {
			if line.Number > 0 {
				sb.WriteString(newline)
			}
			sb.WriteString(BlankSequenceArea(line.Format))
			sb.WriteString(line.IndicatorArea)
		}

		sb.WriteString(line.GetContentArea())
	}

	return sb.String()
}

// Preprocess normalizes COBOL source code for ANTLR parsing.
//
// This is a faithful Go port of the proleap preprocessor pipeline from
// the antlr/grammars-v4 repository. The pipeline:
//  1. Reads raw lines into CobolLine structs (LineReader)
//  2. Processes indicator column — continuations, comments, debug (LineIndicatorProcessor)
//  3. Marks multi-line comment entries — AUTHOR., DATE-WRITTEN., etc. (CommentEntriesMarker)
//  4. Serializes processed lines back to text (LineWriter)
//
// COPY/REPLACE expansion (Stage 5 in the Java reference) is skipped because
// we don't have access to copybook files in our AST indexing context.
func Preprocess(raw string) string {
	if len(raw) == 0 {
		return raw
	}

	format := detectFormat(raw)

	lines := readLines(raw, format)

	processed := processLineIndicators(lines)

	marked := markCommentEntries(processed, format)

	return serializeLines(marked)
}

func detectFormat(raw string) CobolSourceFormat {
	lines := strings.SplitN(raw, "\n", 51)

	numericCount := 0
	checkedCount := 0

	for _, line := range lines {
		line = strings.TrimRight(line, "\r")
		if len(line) < 7 {
			continue
		}
		checkedCount++
		if checkedCount > 50 {
			break
		}

		allDigitsOrSpaces := true
		hasDigit := false
		for _, r := range line[:6] {
			if r >= '0' && r <= '9' {
				hasDigit = true
			} else if r != ' ' {
				allDigitsOrSpaces = false
				break
			}
		}
		if allDigitsOrSpaces && hasDigit {
			numericCount++
		}
	}

	if checkedCount == 0 {
		return FormatFixed
	}

	if float64(numericCount)/float64(checkedCount) > 0.3 {
		return FormatFixed
	}

	return FormatVariable
}

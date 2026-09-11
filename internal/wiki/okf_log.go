package wiki

import (
	"fmt"
	"os"
	"regexp"
	"strings"
)

// LogEntryKind is the leading bold word of a log entry.
//
// §9 calls it a convention rather than a requirement, and lists these three.
const (
	LogCreation    = "Creation"
	LogUpdate      = "Update"
	LogDeprecation = "Deprecation"
)

// LogEntry is one line of a §9 log file.
type LogEntry struct {
	Kind string
	Text string
}

var reOKFLogDate = regexp.MustCompile(`(?m)^## (\d{4}-\d{2}-\d{2})\s*$`)

// AppendOKFLogEntries prepends entries to a §9 log file, grouped under their date.
//
// §9 fixes three things: the headings are ISO 8601 dates, the newest date comes first, and
// the entries under it are prose. It says nothing about frontmatter, and §11 exempts the
// reserved filenames from the frontmatter requirement, so a log carries none — the block the
// pre-OKF writer put there made log.md the one file in the bundle claiming to be a concept.
//
// Several syncs on the same day land under the same heading rather than creating a heading
// per run, which is the difference between a log a person can read a month of and one that
// is a heading per compile.
func AppendOKFLogEntries(logPath, header string, date string, entries []LogEntry) error {
	if len(entries) == 0 {
		return nil
	}

	var block strings.Builder
	for _, e := range entries {
		kind := e.Kind
		if kind == "" {
			kind = LogUpdate
		}
		_, _ = fmt.Fprintf(&block, "* **%s**: %s\n", kind, strings.TrimSpace(e.Text))
	}

	existing, _ := os.ReadFile(logPath)
	content := string(existing)
	if strings.TrimSpace(content) == "" {
		content = "# " + header + "\n\n"
		content += "## " + date + "\n\n" + block.String() + "\n"
		return os.WriteFile(logPath, []byte(content), 0o644)
	}

	if loc := reOKFLogDate.FindStringSubmatchIndex(content); loc != nil {
		firstDate := content[loc[2]:loc[3]]
		if firstDate == date {
			insertAt := loc[1]
			for insertAt < len(content) && content[insertAt] == '\n' {
				insertAt++
			}
			return os.WriteFile(logPath, []byte(content[:insertAt]+block.String()+content[insertAt:]), 0o644)
		}
		return os.WriteFile(logPath,
			[]byte(content[:loc[0]]+"## "+date+"\n\n"+block.String()+"\n"+content[loc[0]:]), 0o644)
	}

	insertAt := len(content)
	if h1 := strings.Index(content, "\n#"); strings.HasPrefix(content, "# ") || h1 >= 0 {
		if i := strings.Index(content, "\n## "); i >= 0 {
			insertAt = i + 1
		} else if i := strings.Index(content, "\n\n"); i >= 0 {
			insertAt = i + 2
		}
	}
	head, tail := content[:insertAt], content[insertAt:]
	if !strings.HasSuffix(head, "\n\n") {
		head = strings.TrimRight(head, "\n") + "\n\n"
	}
	return os.WriteFile(logPath, []byte(head+"## "+date+"\n\n"+block.String()+"\n"+tail), 0o644)
}

package ide

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"

	gitblk "github.com/graphit-labs/graphit-code/internal/git"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

var legacyHTMLBlocks = []string{
	"MEMORY", "AST_EXPLORER", "HUB_DISCOVERY",
	"KNOWLEDGE", "IMPROVEMENTS", "SYSTEM_MANDATE",
}

var MandateBlockName = strings.ToUpper(brand.Brand) + "_SYSTEM_MANDATE"

var SysReminder = "\n\n_SYS_REMINDER: Mandate active: <" + brand.Brand + "> required. MCP-first: never read index.md directly. No grep for structure — use AST. If external, use Hub. If code changed, update docs/tasks/ & " + brand.Brand + "_sync."

func mandatePreamble() string {
	tag := brand.Brand
	return "You are the " + brand.Capitalize(brand.Brand) + " autonomous agent.\n" +
		"Before ANY action, tool use, or code generation, you MUST produce <" + tag + "> evaluating all module rules below.\n" +
		"Format: <" + tag + ">MEM:0|AST:0|HUB:0|DOC:0|IMP:0 — [action]</" + tag + ">\n" +
		"Set a module flag to 1 when that module's rule applies to the current action.\n"
}

func mandateTag() string {
	return MandateBlockName
}

func readMandateContent(targetPath string) string {
	data, err := os.ReadFile(targetPath)
	if err != nil {
		return ""
	}
	content := string(data)

	startTag := "<" + mandateTag() + ">"
	endTag := "</" + mandateTag() + ">"
	si := strings.Index(content, startTag)
	ei := strings.Index(content, endTag)
	if si < 0 || ei < 0 || ei <= si {
		return ""
	}
	return content[si+len(startTag) : ei]
}

// UpsertMandateTrigger adds or updates a module's trigger within the mandate.
// triggerTag is the XML tag name (e.g. "mem_rule"), triggerContent is the raw
// inner content (without XML tags). The function wraps it in <triggerTag>...</triggerTag>.
func UpsertMandateTrigger(projectDir, ideName, triggerTag, triggerContent string) error {
	globalFile := GlobalRulesFile(ideName)
	targetPath := filepath.Join(projectDir, globalFile)

	if err := os.MkdirAll(filepath.Dir(targetPath), 0o755); err != nil {
		return err
	}

	cleanupLegacy(targetPath)

	inner := readMandateContent(targetPath)

	// Idempotency: if the trigger tag already contains exactly the same content,
	// skip all destructive operations (RemoveBlockStyled + WriteFile).
	reCurrent := regexp.MustCompile(`(?s)<` + regexp.QuoteMeta(triggerTag) + `>(.*?)</` + regexp.QuoteMeta(triggerTag) + `>`)
	if m := reCurrent.FindStringSubmatch(inner); m != nil && m[1] == triggerContent {
		return nil
	}

	// Remove old trigger if present.
	re := regexp.MustCompile(`(?s)<` + regexp.QuoteMeta(triggerTag) + `>.*?</` + regexp.QuoteMeta(triggerTag) + `>\n?`)
	inner = re.ReplaceAllString(inner, "")
	inner = strings.TrimSpace(inner)

	// Append the new trigger wrapped in XML tags.
	wrapped := "<" + triggerTag + ">" + triggerContent + "</" + triggerTag + ">"
	if inner == "" {
		inner = mandatePreamble() + "\n\n" + wrapped
	} else {
		inner = inner + "\n" + wrapped
	}

	// Remove existing mandate block, then prepend.
	_, _ = gitblk.RemoveBlockStyled(targetPath, mandateTag(), false, gitblk.XMLBlockStyle)

	block := "<" + mandateTag() + ">\n" + strings.TrimSpace(inner) + "\n</" + mandateTag() + ">"

	rest := ""
	if data, err := os.ReadFile(targetPath); err == nil {
		rest = strings.TrimSpace(string(data))
	}

	var out string
	if rest != "" {
		out = block + "\n\n" + rest + "\n"
	} else {
		out = block + "\n"
	}

	return os.WriteFile(targetPath, []byte(out), 0o644)
}

// RemoveMandateTrigger removes a module's trigger from the mandate.
// If no triggers remain, the entire mandate block is removed.
func RemoveMandateTrigger(projectDir, ideName, triggerTag string) error {
	globalFile := GlobalRulesFile(ideName)
	targetPath := filepath.Join(projectDir, globalFile)

	inner := readMandateContent(targetPath)
	if inner == "" {
		return nil
	}

	re := regexp.MustCompile(`(?s)<` + regexp.QuoteMeta(triggerTag) + `>.*?</` + regexp.QuoteMeta(triggerTag) + `>\n?`)
	cleaned := re.ReplaceAllString(inner, "")
	cleaned = strings.TrimSpace(cleaned)

	// Check if any triggers remain (look for any <*_rule> tags).
	hasRules := regexp.MustCompile(`<\w+_rule>`).MatchString(cleaned)

	if !hasRules {
		return RemoveMandate(projectDir, ideName)
	}

	// Rebuild mandate with remaining triggers.
	_, _ = gitblk.RemoveBlockStyled(targetPath, mandateTag(), false, gitblk.XMLBlockStyle)

	block := "<" + mandateTag() + ">\n" + cleaned + "\n</" + mandateTag() + ">"

	rest := ""
	if data, err := os.ReadFile(targetPath); err == nil {
		rest = strings.TrimSpace(string(data))
	}

	var out string
	if rest != "" {
		out = block + "\n\n" + rest + "\n"
	} else {
		out = block + "\n"
	}

	return os.WriteFile(targetPath, []byte(out), 0o644)
}

func RemoveMandate(projectDir, ideName string) error {
	globalFile := GlobalRulesFile(ideName)
	targetPath := filepath.Join(projectDir, globalFile)
	_, err := gitblk.RemoveBlockStyled(targetPath, mandateTag(), false, gitblk.XMLBlockStyle)
	return err
}

func cleanupLegacy(targetPath string) {
	for _, old := range legacyHTMLBlocks {
		marker := blockMarkerForName(old)
		_, _ = gitblk.RemoveBlockStyled(targetPath, marker, false, gitblk.HTMLBlockStyle)
	}
}

func blockMarkerForName(name string) string {
	return strings.ToUpper(brand.Brand) + " " + strings.ToUpper(name) + " BLOCK"
}

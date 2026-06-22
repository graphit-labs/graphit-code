package ide

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	gitblk "github.com/graphit-labs/graphit-code/internal/git"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// mandateHashCacheFile is a JSON sidecar file that stores per-trigger SHA256
// hashes to enable fast-path skipping in UpsertMandateTrigger.
const mandateHashCacheFile = ".mandate.hash"

// mandateHashCache is the JSON structure stored in mandateHashCacheFile.
type mandateHashCache struct {
	Hashes map[string]string `json:"hashes"` // triggerTag → SHA256 of triggerContent
	Mtimes map[string]int64  `json:"mtimes"` // triggerTag → AGENTS.md mtime (UnixNano) at last write
}

func loadMandateHashCache(rulesDir string) *mandateHashCache {
	c := &mandateHashCache{
		Hashes: make(map[string]string),
		Mtimes: make(map[string]int64),
	}
	data, err := os.ReadFile(filepath.Join(rulesDir, mandateHashCacheFile))
	if err != nil {
		return c
	}
	_ = json.Unmarshal(data, c)
	if c.Hashes == nil {
		c.Hashes = make(map[string]string)
	}
	if c.Mtimes == nil {
		c.Mtimes = make(map[string]int64)
	}
	return c
}

func saveMandateHashCache(rulesDir string, c *mandateHashCache) {
	data, err := json.Marshal(c)
	if err != nil {
		return
	}
	_ = os.WriteFile(filepath.Join(rulesDir, mandateHashCacheFile), data, 0o644)
}

var legacyHTMLBlocks = []string{
	"MEMORY", "AST_EXPLORER", "HUB_DISCOVERY",
	"KNOWLEDGE", "IMPROVEMENTS", "SYSTEM_MANDATE",
}

// canonicalTriggerOrder defines the fixed position of every known module
// trigger inside the mandate block. UpsertMandateTrigger always rewrites
// the full inner content in this order so the file never changes position
// between syncs, keeping diffs minimal.
var canonicalTriggerOrder = []string{
	"mem_rule",
	"ast_rule",
	"hub_rule",
	"doc_rule",
	"imp_rule",
}

// parseTriggers extracts all <tag>...</tag> blocks from inner into a map.
// Go's regexp package does not support backreferences, so we first collect
// all tag names and then extract each one individually.
func parseTriggers(inner string) map[string]string {
	// Find all tag names present in the inner content.
	tagRe := regexp.MustCompile(`<(\w+)>`)
	tagMatches := tagRe.FindAllStringSubmatch(inner, -1)
	seen := make(map[string]bool, len(tagMatches))
	for _, m := range tagMatches {
		seen[m[1]] = true
	}

	out := make(map[string]string, len(seen))
	for tag := range seen {
		re := regexp.MustCompile(`(?s)<` + regexp.QuoteMeta(tag) + `>(.*?)</` + regexp.QuoteMeta(tag) + `>`)
		if m := re.FindStringSubmatch(inner); m != nil {
			out[tag] = m[1]
		}
	}
	return out
}

// assembleTriggers rebuilds the mandate inner content from a trigger map,
// emitting known triggers in canonicalTriggerOrder and then any unknown
// ones (from hub-installed artifacts, etc.) in sorted order after.
func assembleTriggers(triggers map[string]string) string {
	var parts []string
	seen := make(map[string]bool, len(triggers))
	for _, tag := range canonicalTriggerOrder {
		if content, ok := triggers[tag]; ok {
			parts = append(parts, "<"+tag+">"+content+"</"+tag+">")
			seen[tag] = true
		}
	}
	extra := make([]string, 0, len(triggers))
	for tag := range triggers {
		if !seen[tag] {
			extra = append(extra, tag)
		}
	}
	sort.Strings(extra)
	for _, tag := range extra {
		parts = append(parts, "<"+tag+">"+triggers[tag]+"</"+tag+">")
	}
	return strings.Join(parts, "\n")
}

func triggersInCanonicalOrder(fileContent string) bool {
	lastPos := -1
	for _, tag := range canonicalTriggerOrder {
		pos := strings.Index(fileContent, "<"+tag+">")
		if pos < 0 {
			continue
		}
		if pos <= lastPos {
			return false
		}
		lastPos = pos
	}
	return true
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
	return readMandateContentFromString(string(data))
}

func readMandateContentFromString(content string) string {
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

	hashCacheDir := filepath.Join(projectDir, ".graphit")
	_ = os.MkdirAll(hashCacheDir, 0o755)
	hashCache := loadMandateHashCache(hashCacheDir)
	newHash := fmt.Sprintf("%x", sha256.Sum256([]byte(triggerContent)))
	if storedHash, ok := hashCache.Hashes[triggerTag]; ok && storedHash == newHash {
		if fi, err := os.Stat(targetPath); err == nil {
			cachedMtime := hashCache.Mtimes[triggerTag]
			if fi.ModTime().UnixNano() == cachedMtime {
				return nil
			}
			if data, readErr := os.ReadFile(targetPath); readErr == nil {
				inner := readMandateContentFromString(string(data))
				triggers := parseTriggers(inner)
				if currentContent, ok := triggers[triggerTag]; ok {
					if fmt.Sprintf("%x", sha256.Sum256([]byte(currentContent))) == newHash &&
						triggersInCanonicalOrder(string(data)) {
						hashCache.Mtimes[triggerTag] = fi.ModTime().UnixNano()
						saveMandateHashCache(hashCacheDir, hashCache)
						return nil
					}
				}
			}
		}
	}

	fileData, _ := os.ReadFile(targetPath)
	fileContent := string(fileData)

	hasLegacy := false
	for _, old := range legacyHTMLBlocks {
		marker := blockMarkerForName(old)
		if strings.Contains(fileContent, "<!-- "+marker+" -->") || strings.Contains(fileContent, "<!-- END "+marker+" -->") {
			hasLegacy = true
			break
		}
	}

	inner := readMandateContentFromString(fileContent)

	reCurrent := regexp.MustCompile(`(?s)<` + regexp.QuoteMeta(triggerTag) + `>(.*?)</` + regexp.QuoteMeta(triggerTag) + `>`)
	if !hasLegacy {
		if m := reCurrent.FindStringSubmatch(inner); m != nil && m[1] == triggerContent &&
			triggersInCanonicalOrder(fileContent) {
			hashCache.Hashes[triggerTag] = newHash
			saveMandateHashCache(hashCacheDir, hashCache)
			return nil
		}
	}

	if hasLegacy {
		cleanupLegacy(targetPath)
	}

	inner = readMandateContent(targetPath)

	// Parse all existing triggers, update the target one, then rewrite in
	// canonical order so the mandate block is always position-stable.
	triggers := parseTriggers(inner)
	triggers[triggerTag] = triggerContent

	body := assembleTriggers(triggers)
	var assembledInner string
	if body == "" {
		assembledInner = mandatePreamble() + "\n\n" + "<" + triggerTag + ">" + triggerContent + "</" + triggerTag + ">"
	} else {
		assembledInner = mandatePreamble() + "\n\n" + body
	}

	_, _ = gitblk.RemoveBlockStyled(targetPath, mandateTag(), false, gitblk.XMLBlockStyle)

	block := "<" + mandateTag() + ">\n" + strings.TrimSpace(assembledInner) + "\n</" + mandateTag() + ">"

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

	if err := os.WriteFile(targetPath, []byte(out), 0o644); err != nil {
		return err
	}

	// Update hash + mtime cache after a successful write.
	hashCache.Hashes[triggerTag] = newHash
	if fi, err := os.Stat(targetPath); err == nil {
		hashCache.Mtimes[triggerTag] = fi.ModTime().UnixNano()
	}
	saveMandateHashCache(hashCacheDir, hashCache)
	return nil
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

	// Check if any triggers remain.
	hasRules := regexp.MustCompile(`<\w+_rule>`).MatchString(cleaned)

	if !hasRules {
		return RemoveMandate(projectDir, ideName)
	}

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

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
// hashes to enable fast-path skipping in UpsertMandateTrigger. It lives in the
// project's runtime directory, which is the only part of the brand directory the
// generated .gitignore covers.
const mandateHashCacheFile = "mandate.hash"

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

// ModuleMandateTrigger builds a standardized mandate trigger block for a
// module. The block enforces MCP-tool priority over the model's native tools,
// forbids CLI usage, and requires the agent to read the module skill before
// acting. alwaysClause, when non-empty, adds an unconditional "always use this
// skill" directive (used for the memory and AST modules).
//
// IMPORTANT: the returned content is embedded inside <triggerTag>...</triggerTag>
// in AGENTS.md and is parsed by parseTriggers, which treats any <word> as a tag.
// Therefore this text MUST NOT contain angle-bracket pseudo-tags.
// ModuleMandateTrigger renders one module's block of the system mandate.
//
// triggers are the concrete situations that must make the agent open the skill.
// They exist because an abstract domain name does not fire reliably: an agent
// asked to "find who calls saveUser" does not necessarily classify that as
// "structural analysis", and so reaches for grep. Listing the shapes a request
// actually arrives in — the words a person types, the intent behind them — is
// what turns the mandate from a policy into a trigger.
//
// tools are the MCP tools the module owns. Naming them here means the agent
// knows a tool exists before it has read the skill, which is the moment the
// decision between MCP and a native tool is actually made.
func ModuleMandateTrigger(heading, skillName, domain, alwaysClause string, triggers, tools []string) string {
	var b strings.Builder
	b.WriteString("\n# ")
	b.WriteString(heading)
	b.WriteString("\n")
	// The precedence rule, the CLI ban and the integrity clause are stated once in
	// mandatePreamble and apply here in full — repeating them per module cost five
	// copies of the same paragraph at the top of every session. What this line carries
	// is what actually differs: the domain and the skill that covers it.
	b.WriteString("MCP-FIRST for ")
	b.WriteString(domain)
	b.WriteString(": read the `")
	b.WriteString(skillName)
	b.WriteString("` skill BEFORE any ")
	b.WriteString(domain)
	b.WriteString(" operation, and use exactly the MCP tools it prescribes.\n")

	if len(triggers) > 0 {
		b.WriteString("\nOPEN THE `")
		b.WriteString(skillName)
		b.WriteString("` SKILL WHEN ANY OF THESE IS TRUE — this is the trigger list, not a summary:\n")
		for _, t := range triggers {
			b.WriteString("- ")
			b.WriteString(t)
			b.WriteString("\n")
		}
	}

	if len(tools) > 0 {
		b.WriteString("\nMCP tools this module owns: ")
		for i, t := range tools {
			if i > 0 {
				b.WriteString(", ")
			}
			b.WriteString("`")
			b.WriteString(brand.MCPToolName(t))
			b.WriteString("`")
		}
		b.WriteString(". The skill says when and how to call each.\n")
	}

	if alwaysClause != "" {
		b.WriteString("\n")
		b.WriteString(alwaysClause)
		b.WriteString("\n")
	}
	return b.String()
}

var MandateBlockName = strings.ToUpper(brand.Brand) + "_SYSTEM_MANDATE"

var SysReminder = ""

// mandatePreamble states the policy that is identical for every module, ONCE.
//
// It used to be repeated inside each module block: five copies of the precedence rule,
// the CLI ban, the native-tool list, the "if unsure, it applies" line and the integrity
// clause. That is a large, fixed cost paid at the top of every session before any work
// begins, and the fifth copy teaches nothing the first did not. What genuinely varies
// per module — its domain, its skill, its triggers, its tools — stays in the block.
func mandatePreamble() string {
	return "You are the " + brand.Capitalize(brand.Brand) + " autonomous agent.\n" +
		"Whenever you are about to perform any action, you MUST first read and use the corresponding skill. Always read the corresponding skill before proceeding.\n" +
		"\n" +
		"## MCP-FIRST — NON-NEGOTIABLE (applies to EVERY module below, in full)\n" +
		"For any task a module below covers, the " + brand.Brand + " MCP tools take ABSOLUTE PRECEDENCE over your built-in/native tools. " +
		"Use them via MCP ONLY — NEVER via the CLI, and NEVER substitute them with your own native tooling (grep, ripgrep, file search, native memory/recall, web search, code symbols) when an MCP tool exists for the job.\n" +
		"Read the module's skill BEFORE performing any operation in its domain, and use exactly the MCP tools it prescribes; never invent arguments for them.\n" +
		"Each module lists the situations that must make you open its skill. If you are unsure whether one applies, it applies — reading the skill costs one tool call; guessing costs a wrong answer. " +
		"Those lists re-apply to every request in this conversation, not only the first: the tenth edit the user asks for needs the same check as the first, especially once you are mid-task and already holding assumptions from earlier turns.\n" +
		"Bypassing, skipping, or short-circuiting these tools — or falling back to native tools without meeting a skill's explicit fallback conditions — is a framework integrity violation.\n" +
		"\n" +
		"## AN INTERRUPTION IS NOT AN EXEMPTION (applies to every resume)\n" +
		"Being interrupted, corrected, redirected, or asked to change, fix or redo work does not suspend anything above — it re-applies all of it. Before you touch the work again: re-open the skill for the domain you are re-entering, re-run its lookups, and keep the " + brand.Brand + " MCP tools ahead of your native ones exactly as on the first turn.\n" +
		"This is where the rule is most often dropped, and not out of confusion: a correction feels like continuation and it arrives with urgency, so the native tool is the one that comes to hand. It is also the moment your assumptions are least reliable — the user just changed a premise the earlier work rested on, so what you were about to do next is a guess until the tools confirm it again. Resuming from memory of what you believed before the interruption is the same violation as never having read the skill.\n" +
		"\n" +
		"## AUTOMATIC INDEXING LAGS THE CHANGE — SYNC IS HOW YOU GET CERTAINTY\n" +
		"The daemon reindexes on its own, but it does so AFTER the write and with a short delay. A tool called inside that window answers from an index that does not yet hold what was just written, and it answers with exactly the confidence it would have if it did: from where you are standing, a stale result and a current one look the same.\n" +
		"So whenever you need to be CERTAIN that what these tools return reflects the current state — before deciding anything on the basis of a result, before reporting work as done, and after any change that did not come from your own edits (a pull, a checkout, a rebase, a restore) — call " + brand.MCPToolRef("sync") + " for the project and let it finish. That single call brings the knowledge index, the memory index and the AST index into step; when only one of the three is in doubt, the module skills name the narrower tool.\n" +
		"What this does NOT mean is a sync after every edit: mid-session the watcher already covers that, and each skill says when the targeted tool is the better call.\n"
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

	hashCacheDir := brand.ProjectRuntimePath(projectDir)
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

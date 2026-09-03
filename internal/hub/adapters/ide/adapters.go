package ide

import (
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"unicode/utf8"

	"gopkg.in/yaml.v3"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func GetAdapter(ide string) Adapter {
	switch strings.ToLower(ide) {
	case "antigravity":
		return NewAntigravityAdapter()
	case "cursor":
		return NewCursorAdapter()
	case "claude", "claude-code":
		return NewClaudeAdapter()
	case "kiro":
		return NewKiroAdapter()
	case "codex":
		return NewCodexAdapter()
	case "opencode":
		return NewOpenCodeAdapter()
	case "gemini", "gemini-code":
		return NewGeminiAdapter()
	default:
		return nil
	}
}

func SupportedIDEs() []string {
	return []string{"antigravity", "cursor", "claude", "kiro", "codex", "opencode", "gemini"}
}

// SkillFrontmatter builds the YAML frontmatter block that opens a managed
// SKILL.md: the `name`, which must match the skill's directory, and the
// `description`, which is what an IDE matches a request against.
//
// The block is produced by the YAML marshaller, never by string concatenation.
// Every module writes prose into the description — "Use when: ...",
// "MANDATORY: ..." — and a plain YAML scalar may not contain ": ": a strict
// parser reads the colon as a nested mapping and rejects the whole frontmatter.
// The skill is then not degraded, it is invisible: the IDE discovers no valid
// metadata and the agent is never offered the skill, with nothing logged to say
// why. Kiro (strict) skipped all five skills this way while Claude Code
// (lenient) loaded the very same files. Deciding which values need quoting, and
// how, is the marshaller's job — the next description with a character we did
// not anticipate has to keep working.
// Both fields are validated against the limits every IDE enforces on a skill,
// because a value outside them is dropped in the same silent way: the skill
// stops being offered and nothing says so. Failing here surfaces it as a sync
// error instead.
func SkillFrontmatter(name, description string) (string, error) {
	if err := validateSkillName(name); err != nil {
		return "", err
	}
	if err := validateSkillDescription(name, description); err != nil {
		return "", err
	}
	body, err := yaml.Marshal(skillFrontmatter{Name: name, Description: description})
	if err != nil {
		return "", fmt.Errorf("marshaling skill frontmatter for %s: %w", name, err)
	}
	return "---\n" + string(body) + "---\n\n", nil
}

type skillFrontmatter struct {
	Name        string `yaml:"name"`
	Description string `yaml:"description"`
}

const (
	MaxSkillNameLength        = 64
	MaxSkillDescriptionLength = 1024
)

var skillNamePattern = regexp.MustCompile(`^[a-z0-9]+(-[a-z0-9]+)*$`)

func validateSkillName(name string) error {
	if name == "" {
		return fmt.Errorf("skill name is empty")
	}
	if utf8.RuneCountInString(name) > MaxSkillNameLength {
		return fmt.Errorf("skill name %q is %d characters, over the %d-character limit", name, utf8.RuneCountInString(name), MaxSkillNameLength)
	}
	if !skillNamePattern.MatchString(name) {
		return fmt.Errorf("skill name %q must be lowercase letters, digits and single separating hyphens, and must match the skill's directory name", name)
	}
	return nil
}

func validateSkillDescription(name, description string) error {
	if strings.TrimSpace(description) == "" {
		return fmt.Errorf("skill %s: description is empty, and it is what the IDE matches a request against", name)
	}
	if n := utf8.RuneCountInString(description); n > MaxSkillDescriptionLength {
		return fmt.Errorf("skill %s: description is %d characters, over the %d-character limit", name, n, MaxSkillDescriptionLength)
	}
	return nil
}

func InstallManagedSkill(projectDir, ideName, skillName, content string) error {
	adapter := GetAdapter(ideName)
	if adapter == nil {
		return fmt.Errorf("unknown IDE: %s", ideName)
	}
	return installSkillForAdapter(adapter, projectDir, skillName, content)
}

func RemoveManagedSkill(projectDir, ideName, skillName string) error {
	adapter := GetAdapter(ideName)
	if adapter == nil {
		return fmt.Errorf("unknown IDE: %s", ideName)
	}
	return removeSkillForAdapter(adapter, projectDir, skillName)
}

func skillHashCachePath(projectDir, rootDirName, skillName string) string {
	adapterKey := strings.TrimPrefix(rootDirName, ".")
	return brand.ProjectRuntimePath(projectDir, "cache", "skills", adapterKey, skillName)
}

type folderBackedAdapter interface {
	folderBasedAdapter() *FolderBasedAdapter
}

func folderBase(adapter Adapter) (*FolderBasedAdapter, bool) {
	current, ok := adapter.(folderBackedAdapter)
	if !ok {
		return nil, false
	}
	return current.folderBasedAdapter(), true
}

// GetSkillDir returns the directory where a skill is installed for the given
// adapter and project. Returns empty string if the adapter type is unsupported.
func GetSkillDir(adapter Adapter, projectDir, skillName string) string {
	base, ok := folderBase(adapter)
	if !ok {
		return ""
	}
	rootDir, skillsDir := base.cfg.RootDirName, base.cfg.SkillsDir
	if skillsDir == "" {
		skillsDir = "skills"
	}
	return filepath.Join(projectDir, rootDir, skillsDir, skillName)
}

func installSkillForAdapter(adapter Adapter, projectDir, skillName, content string) error {
	base, ok := folderBase(adapter)
	if !ok {
		return fmt.Errorf("unsupported adapter type for skill installation")
	}
	rootDir, skillsDir := base.cfg.RootDirName, base.cfg.SkillsDir
	if skillsDir == "" {
		skillsDir = "skills"
	}

	skillDir := filepath.Join(projectDir, rootDir, skillsDir, skillName)
	if err := os.MkdirAll(skillDir, 0o755); err != nil {
		return fmt.Errorf("creating skill dir: %w", err)
	}

	skillFile := filepath.Join(skillDir, "SKILL.md")
	hashFile := skillHashCachePath(projectDir, rootDir, skillName)
	newHash := fmt.Sprintf("%x", sha256.Sum256([]byte(content)))
	if cached, err := os.ReadFile(hashFile); err == nil {
		parts := strings.SplitN(strings.TrimSpace(string(cached)), ":", 2)
		if len(parts) == 2 && parts[0] == newHash {
			if fi, statErr := os.Stat(skillFile); statErr == nil {
				mtimeMatch := false
				if cachedMtime, parseErr := strconv.ParseInt(parts[1], 10, 64); parseErr == nil {
					mtimeMatch = fi.ModTime().UnixNano() == cachedMtime
				}
				if mtimeMatch {
					return nil
				}
				if currentContent, readErr := os.ReadFile(skillFile); readErr == nil {
					if fmt.Sprintf("%x", sha256.Sum256(currentContent)) == newHash {
						if mkErr := os.MkdirAll(filepath.Dir(hashFile), 0o755); mkErr == nil {
							entry := fmt.Sprintf("%s:%d", newHash, fi.ModTime().UnixNano())
							_ = os.WriteFile(hashFile, []byte(entry), 0o644)
						}
						return nil
					}
				}
			}
		}
	}

	if err := os.WriteFile(skillFile, []byte(content), 0o644); err != nil {
		return err
	}
	if fi, err := os.Stat(skillFile); err == nil {
		if mkErr := os.MkdirAll(filepath.Dir(hashFile), 0o755); mkErr == nil {
			entry := fmt.Sprintf("%s:%d", newHash, fi.ModTime().UnixNano())
			_ = os.WriteFile(hashFile, []byte(entry), 0o644)
		}
	}
	return nil
}

func removeSkillForAdapter(adapter Adapter, projectDir, skillName string) error {
	base, ok := folderBase(adapter)
	if !ok {
		return nil
	}
	rootDir, skillsDir := base.cfg.RootDirName, base.cfg.SkillsDir
	if skillsDir == "" {
		skillsDir = "skills"
	}

	skillDir := filepath.Join(projectDir, rootDir, skillsDir, skillName)
	_ = os.Remove(skillHashCachePath(projectDir, rootDir, skillName))
	return os.RemoveAll(skillDir)
}

func ArtifactTypePath(projectDir, ideName, artifactType, artifactName string) (string, error) {
	if artifactType == "rule" {
		return "", fmt.Errorf("rule artifacts are delivered dynamically by lifecycle hooks and have no IDE path")
	}
	adapter := GetAdapter(ideName)
	if adapter == nil {
		return "", fmt.Errorf("unknown IDE: %s", ideName)
	}

	baseAdapter, ok := folderBase(adapter)
	if !ok {
		return "", fmt.Errorf("unsupported adapter type for IDE %s", ideName)
	}
	rootDir := baseAdapter.cfg.RootDirName
	typeDir := baseAdapter.getTypeDir(artifactType)
	fm := baseAdapter.getFileMode(artifactType)
	if typeDir == "" {
		return "", fmt.Errorf("unknown artifact type %q for IDE %s", artifactType, ideName)
	}

	base := filepath.Join(projectDir, rootDir, typeDir)
	if fm.Mode == "folder" {
		return filepath.Join(base, artifactName), nil
	}
	return filepath.Join(base, artifactName+"."+fm.Ext), nil
}

func GetFileMode(ideName, artifactType string) string {
	adapter := GetAdapter(ideName)
	if adapter == nil {
		return "file"
	}

	base, ok := folderBase(adapter)
	if !ok {
		return "file"
	}
	return base.getFileMode(artifactType).Mode
}

package prep

import (
	"context"
	"crypto/sha256"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
	"github.com/graphit-labs/graphit-code/internal/store"
	"gopkg.in/yaml.v3"
)

var localAliasCharacters = regexp.MustCompile(`[^a-z0-9]+`)

func projectArtifactAlias(item ProjectArtifact) string {
	identity := fmt.Sprintf("%x", sha256.Sum256([]byte(item.ProjectID+"/"+item.InstanceID+"/"+item.ProjectKind+"/"+item.Type+"/"+item.ID)))[:12]
	name := strings.Trim(localAliasCharacters.ReplaceAllString(strings.ToLower(item.ID), "-"), "-")
	if len(name) > 51 {
		name = strings.TrimRight(name[:51], "-")
	}
	if name == "" {
		name = "artifact"
	}
	return name + "-" + identity
}

func prepareProjectArtifact(ctx context.Context, ws, agentName string, ref livesearch.Artifact, progress func(string)) error {
	item, err := resolveProjectArtifact(agentName, ref)
	if err != nil {
		return err
	}
	alias := projectArtifactAlias(item)
	if item.ProjectKind == "own_context" || item.ProjectKind == "installed_context" {
		rec := item.context
		if item.ProjectKind == "own_context" {
			rec = store.ContextRecord{Name: alias, Origin: "link", SourcePath: item.projectDir}
		}
		if rec.IsHub() {
			installer, err := newInstaller(ctx)
			if err != nil {
				return err
			}
			_, err = installer.Install(ctx, rec.ArtifactID+"@"+rec.Version, "", agentName, hub.ArtifactType(item.Type), "", ws)
			return err
		}
		source := store.ASTContextDirIn(item.projectDir, item.ID)
		if item.Type == store.KindKnowledge {
			source = store.KnowledgeContextDirIn(item.projectDir, item.ID)
		}
		if item.ProjectKind == "own_context" {
			source = store.ASTProjectDir(item.projectDir)
			if item.Type == store.KindKnowledge {
				source = store.KnowledgeProjectDir(item.projectDir)
			}
		}
		if info, err := os.Stat(source); err != nil || !info.IsDir() {
			return fmt.Errorf("existing %s index for %s is unavailable", item.Type, item.ID)
		}
		// A local import without a source is a globally named imported store. Keep its
		// name; a sibling link instead receives a unique investigation alias.
		if rec.SourcePath != "" {
			rec.Name = alias
		} else {
			rec.Name = item.ID
		}
		if err := store.AddContext(ws, item.Type, rec); err != nil {
			return err
		}
		progress(fmt.Sprintf("linked %s from %s as %s", item.Type, item.ProjectName, rec.Name))
		return nil
	}
	packageDir := filepath.Join(ws, ".graphit", "live-artifacts", item.Type, alias)
	if err := copyLocalPackage(ctx, item.path, packageDir, item.Type, item.isFile); err != nil {
		return fmt.Errorf("copy %s: %w", item.ID, err)
	}
	if item.Type == "skill" {
		if err := renameLocalSkill(packageDir, alias); err != nil {
			return err
		}
	}
	lockPath := filepath.Join(ws, brand.LockFileName())
	lf, err := hub.LoadLockfile(lockPath)
	if err != nil {
		return err
	}
	if lf == nil {
		return fmt.Errorf("investigation lock is missing")
	}
	typ := hub.ArtifactType(item.Type)
	if lf.Artifacts == nil {
		lf.Artifacts = map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta{}
	}
	if lf.Artifacts[typ] == nil {
		lf.Artifacts[typ] = map[string]*hub.LockfileArtifactMeta{}
	}
	lf.Artifacts[typ][alias] = &hub.LockfileArtifactMeta{Origin: "link", Version: "local", LinkSource: packageDir}
	if err := hub.SaveLockfile(lockPath, lf); err != nil {
		return err
	}
	if err := hub.SyncAgentAdapter(agentName, ws, lf); err != nil {
		return err
	}
	progress(fmt.Sprintf("prepared %s %s from %s", item.Type, item.ID, item.ProjectName))
	return nil
}

func renameLocalSkill(dir, name string) error {
	path := filepath.Join(dir, "SKILL.md")
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	text := strings.ReplaceAll(string(data), "\r\n", "\n")
	if !strings.HasPrefix(text, "---\n") {
		return fmt.Errorf("skill %s is missing YAML frontmatter", name)
	}
	end := strings.Index(text[4:], "\n---")
	if end < 0 {
		return fmt.Errorf("skill %s has invalid frontmatter", name)
	}
	end += 4
	meta := map[string]any{}
	if err := yaml.Unmarshal([]byte(text[4:end]), &meta); err != nil {
		return err
	}
	description, _ := meta["description"].(string)
	if _, err := agent.SkillFrontmatter(name, description); err != nil {
		return err
	}
	meta["name"] = name
	front, err := yaml.Marshal(meta)
	if err != nil {
		return err
	}
	return os.WriteFile(path, []byte("---\n"+string(front)+"---"+text[end+4:]), 0644)
}

// One Hub artifact/version is addressable per type and id in a workspace lock.
// Refuse a conflicting boundary before any install rather than silently replacing it.
func validateSelectedVersions(agentName string, refs []livesearch.Artifact) error {
	versions := map[string]string{}
	for _, ref := range refs {
		if err := validateArtifactSource(ref); err != nil {
			return err
		}
		id, typ, version := ref.ID, ref.Type, ref.Version
		if ref.Source == "project" {
			item, err := resolveProjectArtifact(agentName, ref)
			if err != nil {
				return err
			}
			if item.ProjectKind != "installed_context" || !item.context.IsHub() {
				continue
			}
			id, version = item.context.ArtifactID, item.context.Version
		}
		if base, pinned, ok := strings.Cut(id, "@"); ok {
			id = base
			if version == "" {
				version = pinned
			}
		}
		key := typ + "/" + id
		if previous, exists := versions[key]; exists && previous != version {
			return fmt.Errorf("choose one version of %s for this investigation (selected %q and %q)", key, previous, version)
		}
		versions[key] = version
	}
	return nil
}

func copyLocalPackage(ctx context.Context, source, dest, typ string, isFile bool) error {
	root, err := filepath.EvalSymlinks(source)
	if err != nil {
		return err
	}
	if isFile {
		name := map[string]string{"skill": "SKILL.md", "agent": "AGENT.md", "rule": "RULE.md", "command": "COMMAND.md", "workflow": "WORKFLOW.md"}[typ]
		if name == "" {
			return fmt.Errorf("unsupported local type %s", typ)
		}
		return copyLocalFile(ctx, root, filepath.Join(dest, name))
	}
	return filepath.WalkDir(root, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if err := ctx.Err(); err != nil {
			return err
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		target := filepath.Join(dest, rel)
		if entry.Type()&os.ModeSymlink != 0 {
			return fmt.Errorf("package symlink %s must be materialized before selecting this artifact", rel)
		}
		if entry.IsDir() {
			return os.MkdirAll(target, 0755)
		}
		if !entry.Type().IsRegular() {
			return fmt.Errorf("unsupported package file %s", rel)
		}
		return copyLocalFile(ctx, path, target)
	})
}
func copyLocalFile(ctx context.Context, source, dest string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(dest), 0755); err != nil {
		return err
	}
	input, err := os.Open(source)
	if err != nil {
		return err
	}
	defer input.Close()
	info, err := input.Stat()
	if err != nil {
		return err
	}
	if !info.Mode().IsRegular() {
		return fmt.Errorf("source is not a regular file")
	}
	output, err := os.OpenFile(dest, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, info.Mode().Perm()&0777)
	if err != nil {
		return err
	}
	_, copyErr := io.Copy(output, input)
	closeErr := output.Close()
	if copyErr != nil {
		return copyErr
	}
	return closeErr
}

func validateArtifactSource(ref livesearch.Artifact) error {
	switch ref.Source {
	case "", "hub":
		return nil
	case "project":
		if ref.ProjectID != "" && ref.InstanceID != "" && strings.TrimSpace(ref.ProjectKind) != "" {
			return nil
		}
	}
	return fmt.Errorf("invalid artifact source for %q", ref.ID)
}

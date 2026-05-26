package paths

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	gitmod "github.com/graphit-labs/graphit-code/internal/git"
)

type ProjectPaths struct {
	IDE              string
	ActiveProjectDir string
	TargetDir        string
	FrameworksDir    string
	ResourcesDir     string
	ModulesDir       string
	GitignorePath    string
	RepoHooksDir     string
	GlobalAgentsMD   string
	LocalAgentsMD    string
	LocalAgentsMDRel string
	LocalRulesFile   string
	LockFilePath     string
}

func GetPaths(ide string, global bool) *ProjectPaths {
	projectDir, _ := gitTopLevel()
	if projectDir == "" {
		projectDir, _ = os.Getwd()
	}

	p := buildPaths(ide, projectDir)

	if global {
		p.TargetDir = p.FrameworksDir
		p.LockFilePath = filepath.Join(p.FrameworksDir, brand.LockFileName())
	}
	return p
}

func GetPathsForProject(ide, projectDir string) *ProjectPaths {
	if projectDir == "" {
		return GetPaths(ide, false)
	}
	return buildPaths(ide, projectDir)
}

func buildPaths(ide, projectDir string) *ProjectPaths {
	if ide == "" {
		ide = "antigravity"
	}

	home, _ := os.UserHomeDir()
	frameworksDir := filepath.Join(home, brand.DotDir(), "frameworks")
	resourcesDir := filepath.Join(home, brand.DotDir(), "artifacts")

	repoHooksDir := filepath.Join(resolveGitDir(projectDir), "hooks")
	if hooksPath := gitConfig(projectDir, "core.hooksPath"); hooksPath != "" {
		if filepath.IsAbs(hooksPath) {
			repoHooksDir = hooksPath
		} else {
			repoHooksDir = filepath.Join(projectDir, hooksPath)
		}
	}

	localAgentsMD := filepath.Join(projectDir, brand.DotDir(), "AGENTS.md")
	localAgentsMDRel, _ := filepath.Rel(projectDir, localAgentsMD)
	localAgentsMDRel = filepath.ToSlash(localAgentsMDRel)

	return &ProjectPaths{
		IDE:              ide,
		ActiveProjectDir: projectDir,
		TargetDir:        projectDir,
		FrameworksDir:    frameworksDir,
		ResourcesDir:     resourcesDir,
		ModulesDir:       filepath.Join(resourcesDir, "modules"),
		GitignorePath:    filepath.Join(projectDir, ".gitignore"),
		RepoHooksDir:     repoHooksDir,
		GlobalAgentsMD:   filepath.Join(home, brand.DotDir(), "AGENTS.md"),
		LocalAgentsMD:    localAgentsMD,
		LocalAgentsMDRel: localAgentsMDRel,
		LocalRulesFile:   filepath.Join(projectDir, brand.DotDir(), "rules", "session.md"),
		LockFilePath:     filepath.Join(projectDir, brand.LockFileName()),
	}
}

func gitTopLevel() (string, error) {
	return gitmod.Default().RunGlobalOutput("rev-parse", "--show-toplevel")
}

func gitConfig(dir, key string) string {
	return gitmod.Default().RunSilent(dir, "config", key)
}

func resolveGitDir(projectDir string) string {
	dotGit := filepath.Join(projectDir, ".git")
	info, err := os.Lstat(dotGit)
	if err != nil {
		return dotGit
	}
	if info.IsDir() {
		return dotGit
	}
	data, err := os.ReadFile(dotGit)
	if err != nil {
		return dotGit
	}
	line := strings.TrimSpace(string(data))
	if !strings.HasPrefix(line, "gitdir: ") {
		return dotGit
	}
	gitdir := strings.TrimPrefix(line, "gitdir: ")
	if !filepath.IsAbs(gitdir) {
		gitdir = filepath.Join(projectDir, gitdir)
	}
	return filepath.Clean(gitdir)
}

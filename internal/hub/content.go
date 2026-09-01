package hub

import (
	"context"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"unicode/utf8"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// An agent that has no checkout cannot read what a rule or a skill SAYS.
//
// Every other artifact family is reachable through a query tool: an ast context through
// the graph, a knowledge context through the wiki. The instruction-carrying types —
// rule, skill, command, agent — are plain files, and until now the only copy an agent
// could open was the one materialised into an IDE directory of a project. No project, no
// copy, and no way to ask what the artifact contains.
//
// The clone in the shared cache is the artifact, though, and it is already global and
// version-keyed. So the content is served from there, and the project is optional: it
// only ever narrows WHICH version, by way of the project's claim.
//
// One artifact is often several files — a skill is exactly that, a SKILL.md plus
// whatever it references — so the answer is keyed by path rather than concatenated. A
// concatenation would force the caller to guess where one file ends, and would lose the
// names that the files refer to each other by.

// contentTypes are the artifact types this serves.
//
// ast and knowledge are deliberately absent and are refused by name rather than
// silently returning nothing: they are MOUNTED, not downloaded, so there is no file
// tree to hand back — their content comes from the graph and the wiki.
var contentTypes = map[ArtifactType]bool{
	TypeRule:    true,
	TypeSkill:   true,
	TypeCommand: true,
	TypeAgent:   true,
}

// ContentTypeNames lists the servable types, sorted, for error messages.
func ContentTypeNames() []string {
	names := make([]string, 0, len(contentTypes))
	for t := range contentTypes {
		names = append(names, string(t))
	}
	sort.Strings(names)
	return names
}

// nonTextMarker stands in for a file whose bytes are not text.
//
// An artifact may legitimately carry an image or an archive beside its markdown. Its
// bytes are useless to a language model and expensive to transport, but its PRESENCE is
// information — a skill that references an asset is incomplete without it — so the path
// is reported and the content is not.
func nonTextMarker(size int64) string {
	return fmt.Sprintf("<not text: %d bytes — this file is part of the artifact but its content is not returned>", size)
}

// ArtifactContent is the answer to "what does this artifact contain".
type ArtifactContent struct {
	ID      string       `json:"id"`
	Type    ArtifactType `json:"type"`
	Version string       `json:"version"`
	// Files maps each artifact-relative path to that file's text, in slash form so the
	// keys read the same on every platform.
	Files map[string]string `json:"files"`
	// Canonical is the artifact's entry-point file when it has one — SKILL.md,
	// RULE.md, COMMAND.md, AGENT.md — so a caller knows which key to read first.
	Canonical string `json:"canonical,omitempty"`
	// Notice carries a sentence the caller needs before reading, such as which version
	// was chosen when the reference did not name one.
	Notice string `json:"notice,omitempty"`
}

// ArtifactContentFor reads the files of an installed rule, skill, command or agent.
//
// projectDir is optional. With one, the project's own claim decides the version, which
// is what makes the answer agree with what that project actually has installed. Without
// one, the global installs are consulted and the reference may carry an @version to pick
// among them.
//
// Nothing is downloaded here. An artifact that is not installed is reported as not
// installed, naming the step that is missing — fetching it silently would turn a
// read-only question into a network write against a Hub the caller may not even have
// configured.
func (s *HubService) ArtifactContentFor(ctx context.Context, projectDir, ref string, artType ArtifactType, only string) (*ArtifactContent, error) {
	id, version := store.SplitQualified(ref)

	if artType != "" && !contentTypes[artType] {
		return nil, unservableTypeError(artType)
	}

	dir, resolvedType, resolvedVersion, notice, err := s.artifactContentDir(projectDir, id, artType, version)
	if err != nil {
		return nil, err
	}

	files, canonical, err := readArtifactFiles(dir, resolvedType, only)
	if err != nil {
		return nil, err
	}

	return &ArtifactContent{
		ID:        id,
		Type:      resolvedType,
		Version:   resolvedVersion,
		Files:     files,
		Canonical: canonical,
		Notice:    notice,
	}, nil
}

func unservableTypeError(artType ArtifactType) error {
	switch artType {
	case TypeAST:
		return fmt.Errorf("an ast artifact is mounted, not downloaded, so it has no files to return — " +
			"read it through the code graph instead: ast schema, ast query, ast source")
	case TypeKnowledge:
		return fmt.Errorf("a knowledge artifact is mounted, not downloaded, so it has no files to return — " +
			"read it through the wiki instead: knowledge search, then wiki source")
	default:
		return fmt.Errorf("artifact type %q has no readable content here — this serves %s",
			artType, strings.Join(ContentTypeNames(), ", "))
	}
}

// artifactContentDir locates the clone directory of one installed artifact.
func (s *HubService) artifactContentDir(
	projectDir, id string, artType ArtifactType, version string,
) (dir string, resolvedType ArtifactType, resolvedVersion, notice string, err error) {
	if projectDir != "" {
		return s.projectArtifactDir(projectDir, id, artType, version)
	}

	art, gErr := s.findGlobalInstall(id, artType, version)
	if gErr != nil {
		return "", "", "", "", gErr
	}
	if !contentTypes[art.Type] {
		return "", "", "", "", unservableTypeError(art.Type)
	}
	if version == "" {
		notice = fmt.Sprintf("no version was given, so the globally installed %s@%s was read", art.ID, art.Version)
	}

	dir = art.CachePath
	if dir == "" {
		dir = s.cacheDirFor(art.Type, art.ID, art.Version, art.ProjectID)
	}
	if dir == "" {
		return "", "", "", "", fmt.Errorf("%s %s@%s is recorded as installed but its local directory is unknown — "+
			"reinstall it to repair the record", art.Type, art.ID, art.Version)
	}
	return dir, art.Type, art.Version, notice, nil
}

// projectArtifactDir locates the clone of an artifact a project has claimed.
//
// The project's lockfile pins the version, which is the point of asking with a project:
// the answer is what THIS project has, not whatever is newest on the machine.
func (s *HubService) projectArtifactDir(
	projectDir, id string, artType ArtifactType, version string,
) (string, ArtifactType, string, string, error) {
	lf, err := LoadLockfile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil || lf == nil {
		return "", "", "", "", fmt.Errorf("no project at %s — omit project_dir to read a globally installed artifact", projectDir)
	}

	var meta *LockfileArtifactMeta
	resolvedType := artType
	if artType != "" {
		meta = lf.Artifacts[artType][id]
	} else {
		for t, typeMap := range lf.Artifacts {
			if m, ok := typeMap[id]; ok && contentTypes[t] {
				resolvedType, meta = t, m
				break
			}
		}
	}
	if meta == nil {
		return "", "", "", "", fmt.Errorf("%q is not installed in %s — install it there, or omit project_dir "+
			"to read a globally installed copy", id, projectDir)
	}
	if !contentTypes[resolvedType] {
		return "", "", "", "", unservableTypeError(resolvedType)
	}

	resolvedVersion := meta.Version
	if version != "" && version != resolvedVersion {
		return "", "", "", "", fmt.Errorf("%s has %s@%s installed, not @%s — omit project_dir to read another version",
			projectDir, id, resolvedVersion, version)
	}

	// A linked artifact is developed in place, so its source directory IS its content
	// and reading the clone cache would answer with whatever was published before the
	// link. resolveArtifactPath honours LinkSource for the same reason.
	if meta.Origin == projectlock.OriginLink && meta.SourcePath != "" {
		return projectlock.SourceDir(projectDir, meta.SourcePath), resolvedType, resolvedVersion, "", nil
	}

	dir := s.cacheDirFor(resolvedType, id, resolvedVersion, meta.ProjectID)
	if dir == "" {
		return "", "", "", "", fmt.Errorf("the hub is not configured, so the local directory of %s@%s cannot be resolved",
			id, resolvedVersion)
	}
	return dir, resolvedType, resolvedVersion, "", nil
}

func (s *HubService) cacheDirFor(artType ArtifactType, id, version, publisherID string) string {
	if s.registry == nil {
		return ""
	}
	st := s.registry.Store()
	if st == nil {
		return ""
	}
	return st.ArtifactCacheDir(artType, id, version, publisherID)
}

// readArtifactFiles walks an artifact directory into a path-keyed map.
func readArtifactFiles(dir string, artType ArtifactType, only string) (map[string]string, string, error) {
	info, err := os.Stat(dir)
	if err != nil || !info.IsDir() {
		return nil, "", fmt.Errorf("the artifact's local directory %s is missing — reinstall it", dir)
	}

	if only != "" {
		content, rErr := readOneArtifactFile(dir, only)
		if rErr != nil {
			return nil, "", rErr
		}
		return map[string]string{filepath.ToSlash(filepath.Clean(only)): content}, "", nil
	}

	files := map[string]string{}
	walkErr := filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			// A clone may carry the publisher's version control and per-directory
			// caches. They are not the artifact and returning them would bury it.
			if name := d.Name(); path != dir && (name == ".git" || name == ".DS_Store") {
				return fs.SkipDir
			}
			return nil
		}
		rel, rErr := filepath.Rel(dir, path)
		if rErr != nil {
			return rErr
		}
		files[filepath.ToSlash(rel)] = fileTextOrMarker(path, d)
		return nil
	})
	if walkErr != nil {
		return nil, "", fmt.Errorf("reading the artifact at %s: %w", dir, walkErr)
	}
	if len(files) == 0 {
		return nil, "", fmt.Errorf("the artifact's local directory %s is empty — reinstall it", dir)
	}

	canonical := ""
	if name := filepath.Base(findCanonicalFile(string(artType), dir)); name != "." && name != "" {
		if _, ok := files[name]; ok {
			canonical = name
		}
	}
	return files, canonical, nil
}

func readOneArtifactFile(dir, rel string) (string, error) {
	clean := filepath.Clean(filepath.FromSlash(rel))
	if filepath.IsAbs(clean) || clean == ".." || strings.HasPrefix(clean, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("path %q escapes the artifact — pass a path relative to the artifact root", rel)
	}
	full := filepath.Join(dir, clean)

	// Belt and braces after the lexical check: a symlink inside the clone could still
	// point outside it, and this tool must not become a way to read arbitrary files off
	// the machine that runs the server.
	resolvedDir, err := filepath.EvalSymlinks(dir)
	if err != nil {
		resolvedDir = dir
	}
	resolvedFull, err := filepath.EvalSymlinks(full)
	if err != nil {
		return "", fmt.Errorf("%s is not part of this artifact", rel)
	}
	if rp, err := filepath.Rel(resolvedDir, resolvedFull); err != nil ||
		rp == ".." || strings.HasPrefix(rp, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("%s is not part of this artifact", rel)
	}

	data, err := os.ReadFile(resolvedFull)
	if err != nil {
		return "", fmt.Errorf("reading %s: %w", rel, err)
	}
	if !utf8.Valid(data) {
		return nonTextMarker(int64(len(data))), nil
	}
	return string(data), nil
}

func fileTextOrMarker(path string, d fs.DirEntry) string {
	if info, err := d.Info(); err == nil && info.Size() > maxArtifactFileBytes {
		return fmt.Sprintf("<too large: %d bytes — read it on its own with the 'path' parameter>", info.Size())
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Sprintf("<unreadable: %v>", err)
	}
	if !utf8.Valid(data) {
		return nonTextMarker(int64(len(data)))
	}
	return string(data)
}

// maxArtifactFileBytes caps what a whole-artifact read returns per file.
//
// The cap exists because this tool returns EVERY file at once, and one oversized file
// would otherwise crowd out the ones the caller asked about. It is per-file and it is
// escapable: the `path` parameter reads a single file with no cap.
const maxArtifactFileBytes = 512 * 1024

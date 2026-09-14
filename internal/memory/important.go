package memory

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

func MemoryFileName(id string) string {
	return id + ".md"
}

func MemoryIDFromFileName(name string) string {
	if name == "" {
		return ""
	}
	return strings.TrimSuffix(filepath.Base(name), ".md")
}

func IsImportantContent(content string) bool {
	return ParseMemoryFrontmatter(content).Important
}

func IsMandatoryContent(content string) bool {
	return ParseMemoryFrontmatter(content).Mandatory
}

type ImportantEntry struct {
	ID      string
	Title   string
	Content string
	Path    string
	created string
}

// MandatoryEntry contains the complete memory because mandatory recall is an unconditional read,
// not a title-only search followed by selective page loading.
type MandatoryEntry = ImportantEntry

// ListImportantMemories is the scope's promoted memories, read from its store.
//
// `Path` is the path form a memory would have had — `<id>.md`. It is no longer a location, and it is
// kept because it is what identifies a memory to callers that display or link one; the store has no
// files to point at.
func ListImportantMemories(scope string) ([]ImportantEntry, error) {
	return listMemoriesByRelevance(scope, false)
}

// ListMandatoryMemories reads the authoritative table directly and returns every live mandatory
// memory in canonical priority/recency order.
func ListMandatoryMemories(scope string) ([]MandatoryEntry, error) {
	wd, _ := os.Getwd()
	return ListMandatoryMemoriesForProject(wd, scope)
}

// ListMandatoryMemoriesForProject is the explicit-project form used by Agent
// hooks. Hook processes are not required to inherit the workspace as their
// working directory.
func ListMandatoryMemoriesForProject(projectDir, scope string) ([]MandatoryEntry, error) {
	return listMemoriesByRelevanceIn(projectDir, scope, true)
}

// ListMandatoryMemoriesForProjectContext preserves the verified caller identity
// when mandatory memories are read through remote MCP.
func ListMandatoryMemoriesForProjectContext(ctx context.Context, projectDir, scope string) ([]MandatoryEntry, error) {
	return listMemoriesByRelevanceInContext(ctx, projectDir, scope, true)
}

func listMemoriesByRelevance(scope string, mandatory bool) ([]ImportantEntry, error) {
	wd, _ := os.Getwd()
	return listMemoriesByRelevanceIn(wd, scope, mandatory)
}

func listMemoriesByRelevanceIn(projectDir, scope string, mandatory bool) ([]ImportantEntry, error) {
	return listMemoriesByRelevanceInContext(context.Background(), projectDir, scope, mandatory)
}

func listMemoriesByRelevanceInContext(ctx context.Context, projectDir, scope string, mandatory bool) ([]ImportantEntry, error) {
	scopeID := resolveScopeIDInContext(ctx, projectDir, scope)
	if scopeID == "" {
		return nil, nil
	}
	uri := MemoryTableURIWithContext(ctx, "memory/"+scope+"/"+scopeID, TableDirFor(scope, scopeID))
	if uri == "" {
		return nil, nil
	}
	if cfg := memoryS3Config(ctx, []string{"memory", scope, scopeID}); strings.HasPrefix(uri, "s3://") {
		objects, err := s3store.New(ctx, cfg)
		if err != nil {
			return nil, err
		}
		switch scope {
		case "project":
			snapshot, err := auth.ResolveActive(ctx)
			if err != nil {
				return nil, err
			}
			if auth.UsesScopedS3(snapshot.Provider) {
				registry, err := hub.NewS3Store(ctx, nil, nil)
				if err != nil {
					return nil, err
				}
				if err := registry.AuthorizeProject(ctx, scopeID); err != nil {
					return nil, err
				}
			} else if err := hubaccess.AuthorizeProject(ctx, objects, scopeID); err != nil {
				return nil, err
			}
		case "user":
			subject, err := hubaccess.TrustedSubject(ctx)
			if err != nil || subject.UserID != scopeID {
				return nil, fmt.Errorf("%w: user memory %s", hubaccess.ErrDenied, scopeID)
			}
		}
	}
	tbl, err := OpenMemoryTable(ctx, uri)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tbl.Close() }()

	var records []MemoryRecord
	if mandatory {
		records, err = tbl.Mandatory(ctx)
	} else {
		records, err = tbl.Live(ctx)
	}
	if err != nil {
		return nil, err
	}
	entries := make([]ImportantEntry, 0)
	for _, rec := range records {
		if !mandatory && !rec.Important {
			continue
		}
		entries = append(entries, ImportantEntry{
			ID:      rec.ID,
			Title:   rec.Title,
			Content: strings.TrimSpace(rec.Body),
			Path:    MemoryFileName(rec.ID),
			created: rec.CreatedAt,
		})
	}
	return entries, nil
}

func extractBodyAfterFrontmatter(content string) string {
	lines := strings.Split(content, "\n")
	inFrontmatter := false
	passedFrontmatter := false
	passedH1 := false
	var body []string

	for i, line := range lines {
		trimmed := strings.TrimSpace(line)

		if i == 0 && trimmed == "---" {
			inFrontmatter = true
			continue
		}
		if inFrontmatter {
			if trimmed == "---" {
				inFrontmatter = false
				passedFrontmatter = true
			}
			continue
		}

		if passedFrontmatter && !passedH1 {
			if trimmed == "" {
				continue
			}
			if strings.HasPrefix(trimmed, "# ") {
				passedH1 = true
				continue
			}
		}

		body = append(body, line)
	}

	return strings.TrimSpace(strings.Join(body, "\n"))
}

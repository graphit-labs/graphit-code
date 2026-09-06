package prep

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

func prepareIndexes(ctx context.Context, ws string, progress func(string)) error {
	reportKnowledge(ws, progress)
	if err := ctx.Err(); err != nil {
		return err
	}

	reportGraphs(ws, progress)

	prepareUserMemory(ws, progress)
	return ctx.Err()
}

func reportKnowledge(ws string, progress func(string)) {
	names := knowledge.InstalledContextsIn(ws)
	if len(names) == 0 {
		return
	}
	sort.Strings(names)
	progress(fmt.Sprintf("%s ready to search: %s",
		countNoun(len(names), "documentation set"), strings.Join(names, ", ")))
}

func reportGraphs(ws string, progress func(string)) {
	contexts := ast.ListImportedContextsIn(ws)
	if len(contexts) == 0 {
		return
	}
	names := make([]string, 0, len(contexts))
	for id := range contexts {
		names = append(names, id)
	}
	sort.Strings(names)
	progress(fmt.Sprintf("%s ready to query: %s",
		countNoun(len(names), "code graph"), strings.Join(names, ", ")))
}

func prepareUserMemory(ws string, progress func(string)) {
	userID, err := memory.UserScopeID()
	if err != nil {
		progress("no user memory available: " + err.Error())
		return
	}
	store, err := memory.NewMemoryStore()
	if err != nil {
		progress("no user memory available: " + err.Error())
		return
	}

	svc := memory.NewMemoryService(memory.MemoryScopeUser, userID, store)
	if err := svc.EnsureInitialised(); err != nil {
		progress("the user memory could not be opened: " + err.Error())
		return
	}

	source := svc.WikiDir()
	if source == "" {
		progress("no user memory available: the memory wiki has not been built")
		return
	}
	if entries, err := os.ReadDir(source); err != nil || len(entries) == 0 {
		progress("the user memory is empty, so there is nothing of yours to recall here")
		return
	}
	progress("your memory is available to this search")
}

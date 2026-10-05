package lancestore

import (
	"context"
	"errors"
	"os"
	"sort"
)

// TableBranches lists the native Lance branches of one table at cfg's URI, opening
// the store read-only. A store or table that does not exist yet has none.
func TableBranches(ctx context.Context, cfg Config, table string) ([]string, error) {
	cfg.Writable = false
	cfg.Branch = ""
	cfg.NoFork = false
	if !cfg.IsRemote() {
		// Listing must not create the local store it inspects.
		if _, err := os.Stat(cfg.URI); err != nil {
			return nil, nil
		}
	}
	st, err := Open(ctx, cfg)
	if err != nil {
		return nil, err
	}
	defer func() { _ = st.Close() }()
	names, err := st.TableNames(ctx)
	if err != nil {
		return nil, err
	}
	found := false
	for _, name := range names {
		if name == table {
			found = true
			break
		}
	}
	if !found {
		return nil, nil
	}
	branches, err := st.Branches(ctx, table)
	if errors.Is(err, ErrNoBranches) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	out := make([]string, 0, len(branches))
	for name := range branches {
		out = append(out, name)
	}
	sort.Strings(out)
	return out, nil
}

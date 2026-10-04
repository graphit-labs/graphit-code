package memory

import "context"

// MaintainTable folds write fragments of one authoritative memory table into its
// indexes, compacts it and prunes old physical versions under the retention policy.
// An empty table needs no maintenance.
func MaintainTable(ctx context.Context, uri string) error {
	table, err := OpenMemoryTable(ctx, uri)
	if err != nil {
		return err
	}
	defer func() { _ = table.Close() }()
	count, err := table.Count(ctx)
	if err != nil || count == 0 {
		return err
	}
	if err := table.EnsureIndexes(ctx); err != nil {
		return err
	}
	return table.Maintain(ctx)
}

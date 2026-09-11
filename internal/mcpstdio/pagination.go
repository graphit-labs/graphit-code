package mcpstdio

import (
	"encoding/json"
	"fmt"

	page "github.com/graphit-labs/graphit-code/internal/pagination"
)

func paginationTOON(body, nextCursor string) string {
	if nextCursor == "" {
		return body + "\nnext_cursor: null"
	}
	b, _ := json.Marshal(nextCursor)
	return fmt.Sprintf("%s\nnext_cursor: %s", body, b)
}

func openPage(pageSize int, cursor string, total, defaultPageSize int, bind any) (page.Window, error) {
	return page.Open(page.Spec{
		PageSize:        pageSize,
		Cursor:          cursor,
		Total:           total,
		DefaultPageSize: defaultPageSize,
		Bind:            bind,
	})
}

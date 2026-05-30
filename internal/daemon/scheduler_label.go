//go:build darwin || windows

package daemon

import "github.com/graphit-labs/graphit-code/internal/brand"

func schedulerLabel() string {
	return brand.Brand + ".daemon"
}

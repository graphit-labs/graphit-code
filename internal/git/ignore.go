package git

import "github.com/graphit-labs/graphit-code/internal/brand"

var IgnoreMarker = brand.IgnoreMarker()

func InjectGitignore(targetPath, content string) error {
	return InjectBlock(targetPath, content, IgnoreMarker, "")
}

func RemoveGitignore(targetPath string) (bool, error) {
	return RemoveBlock(targetPath, IgnoreMarker, false)
}

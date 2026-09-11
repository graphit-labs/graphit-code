package hubaccess

import "testing"

func TestProjectPathsRejectMissingOrNonULIDIdentity(t *testing.T) {
	for _, projectID := range []string{"", "project", "../project"} {
		if got := ProjectMetadataKey(projectID); got != "" {
			t.Errorf("ProjectMetadataKey(%q) = %q", projectID, got)
		}
		if got := ProjectArtifactPrefix(projectID, "skills", "lint", "1.0.0"); got != "" {
			t.Errorf("ProjectArtifactPrefix(%q) = %q", projectID, got)
		}
	}
}

func TestProjectPathsRejectUnsafeSegments(t *testing.T) {
	const projectID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	if got := ProjectRegistryKey(projectID, "skills", "../lint"); got != "" {
		t.Errorf("unsafe registry key = %q", got)
	}
	if got := ProjectArtifactPrefix(projectID, "skills", "lint", "branch/main"); got != "" {
		t.Errorf("unsafe artifact prefix = %q", got)
	}
	if got := ProjectTaskPrefix(projectID, "tasks/../shared"); got != "" {
		t.Errorf("unsafe task prefix = %q", got)
	}
}

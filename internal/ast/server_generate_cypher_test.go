package ast

import "testing"

func TestGenerationRepoPathPrefersRequestedProject(t *testing.T) {
	t.Parallel()

	if got := generationRepoPath("/projects/linux", "/projects/graphit-code"); got != "/projects/linux" {
		t.Fatalf("generationRepoPath() = %q, want requested project", got)
	}
	if got := generationRepoPath("", "/projects/graphit-code"); got != "/projects/graphit-code" {
		t.Fatalf("generationRepoPath() fallback = %q, want server repository", got)
	}
}

package dream

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSlugify(t *testing.T) {
	tests := []struct {
		title string
		want  string
	}{
		{"Hello World!", "hello-world"},
		{"Título com Acentuação", "titulo-com-acentuacao"}, // slugify has simple unicode normalization/accents strip
		{"---Special---Characters---", "special-characters"},
		{strings.Repeat("a", 100), strings.Repeat("a", 60)},
	}

	for _, tc := range tests {
		got := slugify(tc.title)
		if got != tc.want {
			t.Errorf("slugify(%q) = %q; want %q", tc.title, got, tc.want)
		}
	}
}

func TestDreamSubjects(t *testing.T) {
	tempProj, err := os.MkdirTemp("", "graphit-dream-test-*")
	if err != nil {
		t.Fatalf("failed to create temp project: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempProj) }()

	// 1. Add Subject
	sub, err := AddSubject(tempProj, "My Dream Subject", "Instructions to dream about.")
	if err != nil {
		t.Fatalf("AddSubject failed: %v", err)
	}
	if sub.Slug != "my-dream-subject" {
		t.Errorf("expected slug 'my-dream-subject', got %q", sub.Slug)
	}

	// Try adding duplicate
	_, err = AddSubject(tempProj, "My Dream Subject", "Instructions.")
	if err == nil {
		t.Error("expected error when adding duplicate subject")
	}

	// 2. List and Pending
	list, err := ListSubjects(tempProj)
	if err != nil {
		t.Fatalf("ListSubjects failed: %v", err)
	}
	if len(list) != 1 || list[0].Slug != "my-dream-subject" {
		t.Errorf("expected 1 subject in list, got %v", list)
	}

	pending, err := PendingSubjects(tempProj)
	if err != nil || len(pending) != 1 {
		t.Errorf("expected 1 pending subject, got %v, error: %v", pending, err)
	}

	// 3. Pick Subject
	picked, err := PickSubject(tempProj)
	if err != nil || picked == nil || picked.Slug != "my-dream-subject" {
		t.Errorf("unexpected picked subject: %v, error: %v", picked, err)
	}

	// Mark done by writing done file
	donePath := filepath.Join(SubjectsDir(tempProj), "my-dream-subject"+resultExt)
	_ = os.WriteFile(donePath, []byte("Done content"), 0644)

	listDone, _ := ListSubjects(tempProj)
	if len(listDone) != 1 || !listDone[0].Done {
		t.Error("expected subject to be marked done")
	}

	pendingEmpty, _ := PendingSubjects(tempProj)
	if len(pendingEmpty) != 0 {
		t.Errorf("expected 0 pending subjects after done, got %v", pendingEmpty)
	}

	// 4. Remove Subject
	err = RemoveSubject(tempProj, "my-dream-subject")
	if err != nil {
		t.Fatalf("RemoveSubject failed: %v", err)
	}

	listEmpty, _ := ListSubjects(tempProj)
	if len(listEmpty) != 0 {
		t.Errorf("expected empty list after removal, got %v", listEmpty)
	}
}

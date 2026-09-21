package wiki

import (
	"context"
	"fmt"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"gopkg.in/yaml.v3"
)

// FrontmatterReferences reads explicit author metadata, never guesses from prose.
// Nil means absent; [] is an intentional empty list. Invalid metadata fails the
// write before the current corpus is changed.
func FrontmatterReferences(body string) (*[]relations.Ref, error) {
	block, ok := FrontmatterBlock(body)
	if !ok {
		return nil, nil
	}
	var doc yaml.Node
	if err := yaml.Unmarshal([]byte(block), &doc); err != nil {
		return nil, fmt.Errorf("references frontmatter: %w", err)
	}
	if len(doc.Content) == 0 || doc.Content[0].Kind != yaml.MappingNode {
		return nil, nil
	}
	mapping := doc.Content[0]
	for i := 0; i+1 < len(mapping.Content); i += 2 {
		if mapping.Content[i].Value != "references" {
			continue
		}
		node := mapping.Content[i+1]
		if node.Kind != yaml.SequenceNode {
			return nil, fmt.Errorf("references must be a list; use [] to clear")
		}
		refs := []relations.Ref{}
		if err := node.Decode(&refs); err != nil {
			return nil, fmt.Errorf("references: %w", err)
		}
		if err := relations.Validate(&refs); err != nil {
			return nil, err
		}
		return &refs, nil
	}
	return nil, nil
}

func chunkReferences(c WikiChunk, xrefs map[string][]string, previous ...relations.Edge) ([]relations.Ref, error) {
	explicit, err := FrontmatterReferences(c.Body)
	if err != nil {
		return nil, fmt.Errorf("page %s: %w", c.Slug, err)
	}
	refs := []relations.Ref{}
	if explicit != nil {
		refs = append(refs, (*explicit)...)
	} else {
		for _, edge := range previous {
			if edge.Source.ID == c.Slug && edge.Field != "wiki-links" {
				refs = append(refs, relations.Ref{Target: edge.Target, Relation: edge.Relation, Field: edge.Field})
			}
		}
	}
	for _, target := range xrefs[c.Slug] {
		refs = append(refs, relations.Ref{Target: relations.Entity{Type: "knowledge", ID: target}, Relation: "wiki-link", Field: "wiki-links"})
	}
	return refs, nil
}

// EnsureReferences repairs an older projection only during an authorized index
// operation. Read APIs never infer or write relationships.
func EnsureReferences(ctx context.Context, dir string) error {
	db, err := OpenWikiDB(ctx, dir)
	if err != nil {
		return err
	}
	defer db.Close()
	_, complete, err := db.ReferenceEdges(ctx)
	if err != nil {
		return err
	}
	if complete {
		return nil
	}
	return db.ReconcileReferences(ctx)
}

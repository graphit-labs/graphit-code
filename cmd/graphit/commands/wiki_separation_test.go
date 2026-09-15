package commands

import (
	"testing"

	"github.com/spf13/cobra"
)

func TestSingleWikiCommandsDoNotOfferMemoryAsAScope(t *testing.T) {
	commands := []struct {
		name string
		cmd  *cobra.Command
	}{
		{"source", newWikiSourceCmd()},
		{"browse", newWikiBrowseCmd()},
		{"log", newWikiLogCmd()},
		{"xrefs", newWikiXRefsCmd()},
		{"embed", newWikiEmbedCmd()},
	}
	for _, item := range commands {
		if flag := item.cmd.Flags().Lookup("wiki"); flag != nil {
			t.Errorf("wiki %s still exposes a --wiki scope selector: %s", item.name, flag.Usage)
		}
	}
}

func TestWikiDoesNotRegisterExport(t *testing.T) {
	cmd := newWikiCmd()
	for _, child := range cmd.Commands() {
		if child.Name() == "export" {
			t.Fatal("wiki export must be owned by the knowledge command")
		}
	}
}

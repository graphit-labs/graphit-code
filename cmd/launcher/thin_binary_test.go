package main

import (
	"reflect"
	"testing"
)

func TestThinBinaryRoutesInvocationsThatDoNotNeedTheCore(t *testing.T) {
	for _, tc := range []struct {
		args []string
		name string
		rest []string
		ok   bool
	}{
		{[]string{"mcp", "--stdio"}, "mcp", nil, true},
		{[]string{"model", "serve", "rerank"}, "model", []string{"serve", "rerank"}, true},
		{[]string{"_session-hook", "--format", "stop", "--sync"}, "hook", []string{"--format", "stop", "--sync"}, true},
		{[]string{"_git-hook", "pre-push", "origin"}, "githook", []string{"pre-push", "origin"}, true},
		{[]string{"tray"}, "tray", nil, true},
		{[]string{"tray", "login", "status"}, "", nil, false},
		{[]string{"mcp"}, "", nil, false},
		{[]string{"sync"}, "", nil, false},
		{nil, "", nil, false},
	} {
		name, rest, ok := thinBinary(tc.args)
		if name != tc.name || ok != tc.ok || !reflect.DeepEqual(rest, tc.rest) {
			t.Errorf("thinBinary(%v) = %q %v %v, want %q %v %v", tc.args, name, rest, ok, tc.name, tc.rest, tc.ok)
		}
	}
}

// Daemon-scheduled store maintenance runs in graphit-store; the same command typed by a
// user stays in the core, which also starts the daemon and tray.
func TestStoreWorkRoutesOnlyForManagedChildren(t *testing.T) {
	old := managedChild
	t.Cleanup(func() { managedChild = old })

	served := [][]string{
		{"task", "maintain"},
		{"memory", "maintain"}, {"memory", "maintain", "--user"},
		{"memory", "embed"}, {"memory", "embed", "--user"},
		{"memory", "index"}, {"memory", "index", "--user"}, {"memory", "index", "--user", "--reset"},
	}
	notServed := [][]string{
		{"task", "maintain", "--json"}, {"task", "list"},
		{"memory", "embed", "--reset"}, {"memory", "maintain", "--user", "--user"},
		{"memory", "search", "x"}, {"memory"},
	}
	managedChild = func() bool { return true }
	for _, args := range served {
		if name, rest, ok := thinBinary(args); !ok || name != "store" || !reflect.DeepEqual(rest, args) {
			t.Errorf("managed %v = %q %v %v, want store with the full arguments", args, name, rest, ok)
		}
	}
	for _, args := range notServed {
		if name, _, ok := thinBinary(args); ok {
			t.Errorf("managed %v routed to %q, want the core", args, name)
		}
	}
	managedChild = func() bool { return false }
	for _, args := range served {
		if name, _, ok := thinBinary(args); ok {
			t.Errorf("interactive %v routed to %q, want the core", args, name)
		}
	}
}

package commands

import (
	"context"
	"errors"
	"testing"
)

func TestRequestedUIReusesLiveDaemon(t *testing.T) {
	old := nonInteractive
	nonInteractive = false
	defer func() { nonInteractive = old }()
	requested := false
	err := runRequestedUI(context.Background(), func() bool { return true }, func() (bool, error) { t.Fatal("live daemon was handed to service"); return false, nil }, func(context.Context) (string, error) { requested = true; return "http://127.0.0.1:8080/", nil }, func(string) {
		if !requested {
			t.Fatal("browser opened before readiness")
		}
	})
	if err != nil {
		t.Fatal(err)
	}
	if startsBackgroundServices(newUICmd()) {
		t.Fatal("UI root pre-run must not restart a live foreground daemon")
	}
}

func TestRequestedUIStartsMissingDaemonAndHonorsNonInteractive(t *testing.T) {
	old := nonInteractive
	nonInteractive = true
	defer func() { nonInteractive = old }()
	started := false
	err := runRequestedUI(context.Background(), func() bool { return false }, func() (bool, error) { started = true; return true, nil }, func(context.Context) (string, error) {
		if !started {
			t.Fatal("request preceded daemon startup")
		}
		return "http://127.0.0.1:8080/", nil
	}, func(string) { t.Fatal("non-interactive browser") })
	if err != nil {
		t.Fatal(err)
	}
}

func TestRequestedUIFailureDoesNotOpenBrowser(t *testing.T) {
	want := errors.New("not ready")
	err := runRequestedUI(context.Background(), func() bool { return true }, func() (bool, error) { return false, nil }, func(context.Context) (string, error) { return "", want }, func(string) { t.Fatal("failure opened browser") })
	if !errors.Is(err, want) {
		t.Fatal(err)
	}
}

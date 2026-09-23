//go:build windows

package daemonservice

import (
	"encoding/xml"
	"strings"
	"testing"
)

func TestWindowsTasksUseLoginAndFailureRecovery(t *testing.T) {
	exe := `C:\Program Files\Graphit & Co\graphit.exe`
	main := taskXML(exe, "daemon --managed", `DOMAIN\tester`, false, true)
	login := taskXML(exe, "daemon service start", `DOMAIN\tester`, true, false)
	for _, document := range [][]byte{main, login} {
		var root struct{ XMLName xml.Name }
		if err := xml.Unmarshal(document, &root); err != nil {
			t.Fatalf("invalid task XML: %v", err)
		}
		if !strings.Contains(string(document), `Graphit &amp; Co`) {
			t.Fatal("path was not XML escaped")
		}
		if strings.Contains(string(document), `<Repetition>`) {
			t.Fatal("periodic watchdog remains")
		}
	}
	if !strings.Contains(string(main), `<RestartOnFailure><Interval>PT1M</Interval><Count>255</Count></RestartOnFailure>`) {
		t.Fatal("no failure restart")
	}
	if !strings.Contains(string(login), `<LogonTrigger><Enabled>true</Enabled><UserId>DOMAIN\tester</UserId></LogonTrigger>`) {
		t.Fatal("no login trigger")
	}
}

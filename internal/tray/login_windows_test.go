//go:build windows

package tray

import (
	"encoding/xml"
	"strings"
	"testing"
)

func TestTrayLoginTaskIsForCurrentUser(t *testing.T) {
	doc := taskXML(`C:\Program Files\Graphit & Co\graphit.exe`, `DOMAIN\test`)
	var root struct{ XMLName xml.Name }
	if err := xml.Unmarshal(doc, &root); err != nil {
		t.Fatal(err)
	}
	text := string(doc)
	for _, want := range []string{`<LogonTrigger>`, `<UserId>DOMAIN\test</UserId>`, `InteractiveToken`, `Graphit &amp; Co`, `<Arguments>tray</Arguments>`, `<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>`, `<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>`} {
		if !strings.Contains(text, want) {
			t.Errorf("missing %q", want)
		}
	}
}

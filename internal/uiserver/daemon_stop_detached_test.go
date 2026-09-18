package uiserver

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func stubDaemonStop(t *testing.T, stop func() error, pid func() (int, bool)) {
	t.Helper()
	previousStop, previousPID := stopDaemonDetached, livingDaemonPID
	t.Cleanup(func() { stopDaemonDetached, livingDaemonPID = previousStop, previousPID })
	stopDaemonDetached, livingDaemonPID = stop, pid
}

func postDaemonStop(t *testing.T) *httptest.ResponseRecorder {
	t.Helper()
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, httptest.NewRequest(http.MethodPost, "/api/daemon/stop", nil))
	return w
}

func decodeStopBody(t *testing.T, w *httptest.ResponseRecorder) map[string]any {
	t.Helper()
	var body map[string]any
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decoding the reply the UI would receive: %v", err)
	}
	return body
}

func TestStoppingAnswersTheBrowserBeforeTheDaemonGoesDown(t *testing.T) {
	stops := 0
	stubDaemonStop(t,
		func() error { stops++; return nil },
		func() (int, bool) { return 7777, true },
	)

	w := postDaemonStop(t)
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}
	// The whole point: the reply exists while the outgoing process is still up, instead of being
	// written after a teardown that kills the connection carrying it.
	if pid, running := livingDaemonPID(); !running || pid != 7777 {
		t.Fatal("the handler waited for the daemon to die before replying")
	}
	if stops != 1 {
		t.Errorf("stop was handed over %d time(s), want exactly 1", stops)
	}
	body := decodeStopBody(t, w)
	if body["success"] != true {
		t.Errorf("success = %v, want true", body["success"])
	}
	if message, _ := body["message"].(string); !strings.Contains(message, "7777") {
		t.Errorf("message = %q, want it to name PID 7777", message)
	}
}

func TestStoppingWithNoDaemonKeepsTheOldReply(t *testing.T) {
	stubDaemonStop(t,
		func() error { t.Fatal("nothing should be stopped when no daemon is running"); return nil },
		func() (int, bool) { return 0, false },
	)

	w := postDaemonStop(t)
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}
	body := decodeStopBody(t, w)
	if body["success"] != true {
		t.Errorf("success = %v, want true", body["success"])
	}
	if message, _ := body["message"].(string); !strings.Contains(message, "No daemon running") {
		t.Errorf("message = %q, want the unchanged no-daemon wording", message)
	}
}

func TestAStopThatCannotStartIsAnErrorNotASuccess(t *testing.T) {
	stubDaemonStop(t,
		func() error { return errors.New("no executable") },
		func() (int, bool) { return 4242, true },
	)

	w := postDaemonStop(t)
	if w.Code == http.StatusOK {
		t.Fatalf("status = %d, want an error status", w.Code)
	}
	if strings.Contains(w.Body.String(), `"success":true`) {
		t.Errorf("body = %q, want no success payload", w.Body.String())
	}
}

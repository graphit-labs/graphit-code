package ladybugstore

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestHTTPFSInstallLayout records what INSTALL httpfs actually puts on disk, because the
// documentation does not publish the extension server's URL scheme or the file name it
// writes. Both are needed to pre-download the extension per platform in the Makefile and
// carry it in the launcher payload, which is how remote graph queries stay offline.
//
// It needs the network once, and it writes to the real ~/.lbug — it is a discovery probe,
// not a test of ours.
//
//	GRAPHIT_HTTPFS_PROBE=1 go test -run TestHTTPFSInstallLayout ./internal/ladybugstore/ -v
func TestHTTPFSInstallLayout(t *testing.T) {
	if os.Getenv("GRAPHIT_HTTPFS_PROBE") == "" {
		t.Skip("set GRAPHIT_HTTPFS_PROBE=1")
	}

	st, err := Open(filepath.Join(t.TempDir(), "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer st.Close()

	if err := st.Exec("INSTALL httpfs", nil); err != nil {
		t.Logf("INSTALL httpfs failed: %v", err)
	} else {
		t.Log("INSTALL httpfs succeeded")
	}

	home, err := os.UserHomeDir()
	if err != nil {
		t.Fatalf("home: %v", err)
	}
	root := filepath.Join(home, ".lbug")
	var found []string
	_ = filepath.Walk(root, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil || info == nil || info.IsDir() {
			return nil
		}
		found = append(found, strings.TrimPrefix(path, root)+" ("+byteSize(info.Size())+")")
		return nil
	})
	if len(found) == 0 {
		t.Logf("nothing under %s", root)
	}
	t.Logf("under %s:\n%s", root, strings.Join(found, "\n"))

	// Does LOAD by name work at all after INSTALL? If it does, the extension is somewhere
	// and the question is only where.
	st3, err3 := Open(filepath.Join(t.TempDir(), "db3"))
	if err3 != nil {
		t.Fatalf("third open: %v", err3)
	}
	if err := st3.Exec("INSTALL httpfs", nil); err != nil {
		t.Logf("INSTALL (second store): %v", err)
	}
	if err := st3.Exec("LOAD EXTENSION httpfs", nil); err != nil {
		t.Logf("LOAD EXTENSION httpfs by NAME failed: %v", err)
	} else {
		t.Log("LOAD EXTENSION httpfs by NAME works")
		if err := st3.Exec("CALL s3_credential(key_id='k', secret='s', region='us-east-1')", nil); err != nil {
			t.Logf("s3_credential after load: %v", err)
		} else {
			t.Log("s3_credential accepted — the extension is genuinely loaded")
		}
	}
	st3.Close()

	for _, dir := range []string{
		filepath.Join(home, ".lbug"),
		filepath.Join(home, ".ladybug"),
		filepath.Join(home, ".kuzu"),
		os.Getenv("LBUG_EXTENSION_DIR"),
	} {
		if dir == "" {
			continue
		}
		if _, statErr := os.Stat(dir); statErr == nil {
			t.Logf("directory exists: %s", dir)
		}
	}

	// INSTALL and LOAD both reported success while s3_credential does not exist, which means
	// one of them is a silent no-op. These four probes separate "the extension is absent" from
	// "the function has another name".
	st4, err4 := Open(filepath.Join(t.TempDir(), "db4"))
	if err4 != nil {
		t.Fatalf("fourth open: %v", err4)
	}
	defer st4.Close()

	for _, q := range []string{
		"CALL show_official_extensions() RETURN *",
		"CALL show_loaded_extensions() RETURN *",
		"CALL show_functions() RETURN name",
	} {
		rows, qErr := st4.Query(q, nil)
		if qErr != nil {
			t.Logf("%s -> error: %v", q, qErr)
			continue
		}
		var hits []string
		for _, r := range rows {
			for _, v := range r {
				sv := strings.ToLower(Str(v))
				if strings.Contains(sv, "http") || strings.Contains(sv, "s3") || strings.Contains(sv, "fs") {
					hits = append(hits, Str(v))
				}
			}
		}
		t.Logf("%s -> %d rows; http/s3/fs mentions: %v", q, len(rows), hits)
	}

	// The definitive test: can it read a remote file at all? A public HTTPS parquet needs
	// httpfs exactly as s3:// does.
	if err := st4.Exec("LOAD EXTENSION httpfs", nil); err != nil {
		t.Logf("load before remote read: %v", err)
	}
	remote := "https://raw.githubusercontent.com/LadybugDB/ladybug/master/README.md"
	if _, qErr := st4.Query("LOAD FROM '"+remote+"' RETURN *", nil); qErr != nil {
		t.Logf("remote LOAD FROM -> %v", qErr)
	} else {
		t.Log("remote LOAD FROM worked — httpfs is functional")
	}

	// The decisive question: the extension server has no build for the running liblbug
	// (0.18.2 is a 404; 0.18.1 is the newest published). Does the previous version's binary
	// load anyway, or does the engine reject it on a version check?
	if explicit := os.Getenv("GRAPHIT_HTTPFS_PATH"); explicit != "" {
		st5, err5 := Open(filepath.Join(t.TempDir(), "db5"))
		if err5 != nil {
			t.Fatalf("fifth open: %v", err5)
		}
		defer st5.Close()

		if loadErr := st5.Exec("LOAD EXTENSION '"+EscapeLiteral(explicit)+"'", nil); loadErr != nil {
			t.Logf("LOAD EXTENSION by explicit path -> %v", loadErr)
		} else {
			t.Log("LOAD EXTENSION by explicit path reported success")
		}

		rows, _ := st5.Query("CALL show_loaded_extensions() RETURN *", nil)
		t.Logf("show_loaded_extensions after explicit load: %d rows -> %v", len(rows), rows)

		if credErr := st5.Exec("CALL s3_credential(key_id='k', secret='s', region='us-east-1')", nil); credErr != nil {
			t.Logf("s3_credential -> %v", credErr)
		} else {
			t.Log("s3_credential EXISTS — httpfs is genuinely loaded from the explicit path")
		}

		// The documented CALL s3_credential(...) does not bind as a function. This engine
		// configures extension options with CALL <option>=<value>, the same shape as the
		// documented CALL HTTP_CACHE_FILE=TRUE, so probe that form.
		for _, stmt := range []string{
			"CALL s3_access_key_id='AKIA_TEST'",
			"CALL s3_secret_access_key='secret_test'",
			"CALL s3_region='us-east-1'",
			"CALL s3_endpoint='localhost:9000'",
			"CALL s3_url_style='path'",
			"CALL http_cache_file=true",
		} {
			if optErr := st5.Exec(stmt, nil); optErr != nil {
				t.Logf("%-45s -> %v", stmt, optErr)
			} else {
				t.Logf("%-45s -> OK", stmt)
			}
		}

	}

	// The whole point of the offline route: loading by explicit path must work, not just
	// LOAD <name> against the managed directory.
	for _, rel := range found {
		name := strings.TrimSpace(strings.Split(rel, " (")[0])
		if !strings.HasSuffix(name, ".lbug_extension") {
			continue
		}
		abs := filepath.Join(root, name)
		st2, err2 := Open(filepath.Join(t.TempDir(), "db2"))
		if err2 != nil {
			t.Fatalf("second open: %v", err2)
		}
		loadErr := st2.Exec("LOAD EXTENSION '"+EscapeLiteral(abs)+"'", nil)
		st2.Close()
		if loadErr != nil {
			t.Errorf("LOAD EXTENSION %q failed: %v", abs, loadErr)
			continue
		}
		t.Logf("LOAD EXTENSION by path works: %s", abs)
	}
}

func byteSize(n int64) string {
	switch {
	case n > 1<<20:
		return itoa(n/(1<<20)) + " MiB"
	case n > 1<<10:
		return itoa(n/(1<<10)) + " KiB"
	default:
		return itoa(n) + " B"
	}
}

func itoa(n int64) string {
	if n == 0 {
		return "0"
	}
	var digits []byte
	for n > 0 {
		digits = append([]byte{byte('0' + n%10)}, digits...)
		n /= 10
	}
	return string(digits)
}

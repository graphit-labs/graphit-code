//go:build windows

package daemonservice

import (
	"bytes"
	"encoding/xml"
	"os"
	"os/user"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

var schtasksRun commandRunner = run

func taskName() string      { return strings.ReplaceAll(serviceName(), "-", "_") }
func loginTaskName() string { return taskName() + "_login" }
func legacyTaskName() string {
	return strings.ReplaceAll(brand.Brand+".daemon", ".", "_") + "_watchdog"
}

func escapeXML(value string) string {
	var b bytes.Buffer
	_ = xml.EscapeText(&b, []byte(value))
	return b.String()
}

func taskXML(exe, args, username string, login, restart bool) []byte {
	trigger := `<RegistrationTrigger><Enabled>false</Enabled></RegistrationTrigger>`
	if login {
		trigger = `<LogonTrigger><Enabled>true</Enabled><UserId>` + escapeXML(username) + `</UserId></LogonTrigger>`
	}
	restartXML := ""
	if restart {
		restartXML = `<RestartOnFailure><Interval>PT1M</Interval><Count>255</Count></RestartOnFailure>`
	}
	return []byte(`<?xml version="1.0" encoding="UTF-8"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
<RegistrationInfo><Description>Graphit daemon</Description></RegistrationInfo>
<Triggers>` + trigger + `</Triggers>
<Principals><Principal id="Author"><UserId>` + escapeXML(username) + `</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
<Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><AllowStartOnDemand>true</AllowStartOnDemand>` + restartXML + `</Settings>
<Actions Context="Author"><Exec><Command>` + escapeXML(exe) + `</Command><Arguments>` + escapeXML(args) + `</Arguments></Exec></Actions>
</Task>`)
}

func registerTask(name string, document []byte) error {
	if err := os.MkdirAll(serviceDir(), 0o700); err != nil {
		return err
	}
	file, err := os.CreateTemp(serviceDir(), "task-*.xml")
	if err != nil {
		return err
	}
	defer os.Remove(file.Name())
	if _, err := file.Write(document); err != nil {
		file.Close()
		return err
	}
	if err := file.Close(); err != nil {
		return err
	}
	return schtasksRun("schtasks", "/create", "/tn", name, "/xml", file.Name(), "/f")
}

func taskExists(name string) bool {
	return schtasksRun("schtasks", "/query", "/tn", name) == nil
}

func install(autoStart bool) error {
	exe, err := ResolveExecutable()
	if err != nil {
		return err
	}
	u, err := user.Current()
	if err != nil {
		return err
	}
	if err := registerTask(taskName(), taskXML(exe, "daemon --managed", u.Username, false, true)); err != nil {
		return err
	}
	if autoStart {
		if err := registerTask(loginTaskName(), taskXML(exe, "daemon service start", u.Username, true, false)); err != nil {
			return err
		}
	} else if taskExists(loginTaskName()) {
		if err := schtasksRun("schtasks", "/delete", "/tn", loginTaskName(), "/f"); err != nil {
			return err
		}
	}
	// Delete only the legacy Graphit task after the managed tasks are registered.
	if taskExists(legacyTaskName()) {
		if err := schtasksRun("schtasks", "/delete", "/tn", legacyTaskName(), "/f"); err != nil {
			return err
		}
	}
	return nil
}

func Start() error {
	if !taskExists(taskName()) {
		return ErrNotInstalled
	}
	if err := schtasksRun("schtasks", "/change", "/tn", taskName(), "/enable"); err != nil {
		return err
	}
	if running, err := IsRunning(); err != nil {
		return err
	} else if running {
		return nil
	}
	return schtasksRun("schtasks", "/run", "/tn", taskName())
}

func setLogin(enabled bool) error {
	if !taskExists(taskName()) {
		return ErrNotInstalled
	}
	if enabled {
		exe, err := ResolveExecutable()
		if err != nil {
			return err
		}
		u, err := user.Current()
		if err != nil {
			return err
		}
		return registerTask(loginTaskName(), taskXML(exe, "daemon service start", u.Username, true, false))
	}
	if taskExists(loginTaskName()) {
		return schtasksRun("schtasks", "/delete", "/tn", loginTaskName(), "/f")
	}
	return nil
}

func Stop() error {
	if !taskExists(taskName()) {
		return ErrNotInstalled
	}
	// Disabling blocks RestartOnFailure. The separate login task enables this
	// task again at the next login when auto-start was selected.
	if err := schtasksRun("schtasks", "/change", "/tn", taskName(), "/disable"); err != nil {
		return err
	}
	if running, err := IsRunning(); err != nil {
		return err
	} else if !running {
		return nil
	}
	// A foreground daemon can hold the same PID lock while the task itself is
	// already stopped. daemonctl.Stop handles that remaining process afterward.
	_ = schtasksRun("schtasks", "/end", "/tn", taskName())
	return nil
}

func Restart() error {
	if err := Stop(); err != nil {
		return err
	}
	return Start()
}

func Remove() error {
	if taskExists(loginTaskName()) {
		if err := schtasksRun("schtasks", "/delete", "/tn", loginTaskName(), "/f"); err != nil {
			return err
		}
	}
	if !taskExists(taskName()) {
		return nil
	}
	if err := Stop(); err != nil {
		return err
	}
	return schtasksRun("schtasks", "/delete", "/tn", taskName(), "/f")
}

func GetStatus() (Status, error) {
	s := Status{Manager: "Windows Task Scheduler"}
	s.Installed = taskExists(taskName())
	if !s.Installed {
		return s, nil
	}
	s.AutoStart = taskExists(loginTaskName())
	var err error
	s.Active, err = IsRunning()
	return s, err
}

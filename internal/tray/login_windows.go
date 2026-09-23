//go:build windows

package tray

import (
	"bytes"
	"encoding/xml"
	"fmt"
	"os"
	"os/exec"
	"os/user"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
)

func trayTaskName() string { return brand.Brand + "_tray" }

func escapeXML(s string) string {
	var b bytes.Buffer
	_ = xml.EscapeText(&b, []byte(s))
	return b.String()
}

func taskXML(exe, username string) []byte {
	return []byte(`<?xml version="1.0" encoding="UTF-8"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
<RegistrationInfo><Description>Graphit tray at user login</Description></RegistrationInfo>
<Triggers><LogonTrigger><Enabled>true</Enabled><UserId>` + escapeXML(username) + `</UserId></LogonTrigger></Triggers>
<Principals><Principal id="Author"><UserId>` + escapeXML(username) + `</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
<Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><AllowStartOnDemand>true</AllowStartOnDemand></Settings>
<Actions Context="Author"><Exec><Command>` + escapeXML(exe) + `</Command><Arguments>tray</Arguments></Exec></Actions>
</Task>`)
}

func taskRun(args ...string) error {
	cmd := exec.Command("schtasks", args...)
	if output, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("schtasks %v: %w: %s", args, err, output)
	}
	return nil
}

func InstallLogin() error {
	exe, err := daemonservice.ResolveExecutable()
	if err != nil {
		return err
	}
	u, err := user.Current()
	if err != nil {
		return err
	}
	f, err := os.CreateTemp("", "graphit-tray-*.xml")
	if err != nil {
		return err
	}
	defer os.Remove(f.Name())
	if _, err := f.Write(taskXML(exe, u.Username)); err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return taskRun("/create", "/tn", trayTaskName(), "/xml", f.Name(), "/f")
}

func RemoveLogin() error {
	enabled, err := LoginEnabled()
	if err != nil || !enabled {
		return err
	}
	return taskRun("/delete", "/tn", trayTaskName(), "/f")
}

func LoginEnabled() (bool, error) {
	return taskRun("/query", "/tn", trayTaskName()) == nil, nil
}

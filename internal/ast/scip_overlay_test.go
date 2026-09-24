package ast

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	scip "github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
)

func TestSCIPDockerCancellationRemovesOverlay(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 to run Docker integration")
	}
	useLocalSCIPDevImage(t)
	root, global := t.TempDir(), t.TempDir()
	writeSCIPJavaFixture(t, filepath.Join(root, "pom.xml"), `<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>x</groupId><artifactId>cancel</artifactId><version>1</version><properties><maven.compiler.source>17</maven.compiler.source><maven.compiler.target>17</maven.compiler.target></properties></project>`)
	writeSCIPJavaFixture(t, filepath.Join(root, "src/main/java/Hello.java"), "public class Hello {}\n")
	before := snapshotSCIPJavaFixture(t, root)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	name := scipContainerName(root, "java")
	t.Cleanup(func() { _ = exec.Command("docker", "container", "rm", "-f", "-v", name).Run() })
	result := make(chan error, 1)
	go func() { _, err := runSCIPImage(ctx, root, global, "java"); result <- err }()
	var volumeName string
	timer := time.NewTimer(20 * time.Second)
	defer timer.Stop()
	ticker := time.NewTicker(50 * time.Millisecond)
	defer ticker.Stop()
observe:
	for {
		select {
		case err := <-result:
			t.Fatalf("indexer finished before cancellation: %v", err)
		case <-timer.C:
			cancel()
			<-result
			t.Fatal("container did not start before cancellation deadline")
		case <-ticker.C:
			inspection, err := exec.Command("docker", "container", "inspect", name).Output()
			if err != nil {
				continue
			}
			var containers []struct {
				State struct {
					Running bool `json:"Running"`
				} `json:"State"`
				Mounts []struct {
					Name        string `json:"Name"`
					Destination string `json:"Destination"`
				} `json:"Mounts"`
			}
			if err := json.Unmarshal(inspection, &containers); err != nil || len(containers) != 1 || !containers[0].State.Running {
				continue
			}
			for _, mount := range containers[0].Mounts {
				if mount.Destination == "/overlay" {
					volumeName = mount.Name
				}
			}
			if volumeName != "" {
				break observe
			}
		}
	}
	cancel()
	err := <-result
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("expected cancellation during indexing, got %v", err)
	}
	if _, err := exec.Command("docker", "container", "inspect", name).Output(); err == nil {
		t.Fatal("cancelled parse left its container and overlay volume")
	}
	if _, err := exec.Command("docker", "volume", "inspect", volumeName).Output(); err == nil {
		t.Fatal("cancelled parse left its anonymous overlay volume")
	}
	if after := snapshotSCIPJavaFixture(t, root); !reflect.DeepEqual(before, after) {
		t.Fatalf("cancelled parse changed source: before=%v after=%v", before, after)
	}
}

func TestSCIPJavaDockerIntegration(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 to run Docker integration")
	}
	useLocalSCIPDevImage(t)
	for _, nested := range []bool{false, true} {
		name := "single"
		if nested {
			name = "module"
		}
		t.Run(name, func(t *testing.T) {
			root, global := t.TempDir(), t.TempDir()
			if !nested {
				root = filepath.Join(root, "java,project")
				global = filepath.Join(global, "ast,global")
				if err := os.MkdirAll(root, 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.MkdirAll(global, 0o755); err != nil {
					t.Fatal(err)
				}
			}
			module := root
			if nested {
				module = filepath.Join(root, "module")
				writeSCIPJavaFixture(t, filepath.Join(root, "pom.xml"), `<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>x</groupId><artifactId>parent</artifactId><version>1</version><packaging>pom</packaging><modules><module>module</module></modules></project>`)
			}
			writeSCIPJavaFixture(t, filepath.Join(module, "pom.xml"), `<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>x</groupId><artifactId>x</artifactId><version>1</version><properties><maven.compiler.source>17</maven.compiler.source><maven.compiler.target>17</maven.compiler.target></properties></project>`)
			writeSCIPJavaFixture(t, filepath.Join(module, "src/main/java/Hello.java"), "public class Hello {}\n")
			before := snapshotSCIPJavaFixture(t, root)
			t.Cleanup(func() { _ = exec.Command("docker", "container", "rm", "-f", scipContainerName(root, "java")).Run() })
			data, err := runSCIPImage(context.Background(), root, global, "java")
			if err != nil {
				t.Fatal(err)
			}
			if len(data) == 0 {
				t.Fatal("empty SCIP index")
			}
			if after := snapshotSCIPJavaFixture(t, root); !reflect.DeepEqual(after, before) {
				t.Fatalf("source changed: before=%v after=%v", before, after)
			}
			if _, err := exec.Command("docker", "container", "inspect", scipContainerName(root, "java")).Output(); err == nil {
				t.Fatal("Java container and overlay volume survived the parse")
			}
			if _, err := os.Stat(filepath.Join(scipOutputDir(root, global, "java"), "index.scip")); err != nil {
				t.Fatalf("SCIP output absent from global AST cache: %v", err)
			}
		})
	}
}

func writeSCIPJavaFixture(t *testing.T, name, data string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(name), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(name, []byte(data), 0o644); err != nil {
		t.Fatal(err)
	}
}

func snapshotSCIPJavaFixture(t *testing.T, root string) map[string]string {
	t.Helper()
	result := make(map[string]string)
	if err := filepath.WalkDir(root, func(name string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(root, name)
		if err != nil {
			return err
		}
		if entry.IsDir() {
			result[rel] = "dir"
			return nil
		}
		data, err := os.ReadFile(name)
		if err != nil {
			return err
		}
		sum := sha256.Sum256(data)
		result[rel] = hex.EncodeToString(sum[:])
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	return result
}

func TestSCIPDotnetDockerIntegration(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 to run Docker integration")
	}
	useLocalSCIPDevImage(t)
	root, global := t.TempDir(), t.TempDir()
	writeSCIPJavaFixture(t, filepath.Join(root, "demo.csproj"), `<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>`)
	writeSCIPJavaFixture(t, filepath.Join(root, "Keep.cs"), "public class Keep { public int Value => new Drop().Value; }\n")
	writeSCIPJavaFixture(t, filepath.Join(root, "Drop.cs"), "public class Drop { public int Value => 1; }\n")
	before := snapshotSCIPJavaFixture(t, root)
	t.Cleanup(func() { _ = exec.Command("docker", "container", "rm", "-f", scipContainerName(root, "dotnet")).Run() })
	data, err := runSCIPImage(context.Background(), root, global, "dotnet")
	if err != nil {
		t.Fatal(err)
	}
	index := &scip.Index{}
	if err := proto.Unmarshal(data, index); err != nil {
		t.Fatal(err)
	}
	seen := map[string]bool{}
	resolvedDrop := false
	for _, doc := range index.Documents {
		seen[doc.RelativePath] = true
		if doc.RelativePath == "Keep.cs" {
			for _, occurrence := range doc.Occurrences {
				if strings.Contains(occurrence.Symbol, "Drop#") {
					resolvedDrop = true
				}
			}
		}
	}
	if !seen["Keep.cs"] || !seen["Drop.cs"] || !resolvedDrop {
		t.Fatalf("missing .NET documents or cross-file Drop reference: docs=%v resolved=%v", seen, resolvedDrop)
	}
	if after := snapshotSCIPJavaFixture(t, root); !reflect.DeepEqual(before, after) {
		t.Fatalf("source tree changed: before=%v after=%v", before, after)
	}
	if _, err := exec.Command("docker", "container", "inspect", scipContainerName(root, "dotnet")).Output(); err == nil {
		t.Fatal(".NET container and overlay volume survived the parse")
	}
	if _, err := os.Stat(filepath.Join(scipOutputDir(root, global, "dotnet"), "index.scip")); err != nil {
		t.Fatalf("SCIP output absent from global AST cache: %v", err)
	}
	if _, err := runSCIPImage(context.Background(), root, global, "dotnet"); err != nil {
		t.Fatalf("incremental index failed: %v", err)
	}
	if after := snapshotSCIPJavaFixture(t, root); !reflect.DeepEqual(before, after) {
		t.Fatalf("source changed after second index: before=%v after=%v", before, after)
	}
}

func TestSCIPDotnetDockerWithExistingAssets(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 to run Docker integration")
	}
	useLocalSCIPDevImage(t)
	root, global := t.TempDir(), t.TempDir()
	writeSCIPJavaFixture(t, filepath.Join(root, "demo.csproj"), `<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup><ItemGroup><PackageReference Include="Newtonsoft.Json" Version="13.0.3" /></ItemGroup></Project>`)
	writeSCIPJavaFixture(t, filepath.Join(root, "Keep.cs"), "public class Keep { public string Value => Newtonsoft.Json.JsonConvert.SerializeObject(1); }\n")
	cache := scipCacheDir(root, global, "dotnet")
	if err := os.MkdirAll(cache, 0o755); err != nil {
		t.Fatal(err)
	}
	uid, gid := scipHostIdentity(context.Background())
	restore := exec.Command("docker", "run", "--rm", "--workdir", "/workspace", "--entrypoint", "dotnet",
		"--user", fmt.Sprintf("%d:%d", uid, gid),
		"--mount", scipBindMount(root, "/workspace", false),
		"--mount", scipBindMount(cache, "/cache", false),
		"-e", "DOTNET_CLI_HOME=/cache/dotnet", "-e", "NUGET_PACKAGES=/cache/nuget",
		"graphit-scip-dotnet:dev", "restore", "demo.csproj", "--ignore-failed-sources")
	if out, err := restore.CombinedOutput(); err != nil {
		t.Fatalf("prepare project.assets.json: %v: %s", err, out)
	}
	if _, err := os.Stat(filepath.Join(root, "obj", "project.assets.json")); err != nil {
		t.Fatal(err)
	}
	before := snapshotSCIPJavaFixture(t, root)
	t.Cleanup(func() { _ = exec.Command("docker", "container", "rm", "-f", scipContainerName(root, "dotnet")).Run() })
	data, err := runSCIPImage(context.Background(), root, global, "dotnet")
	if err != nil {
		t.Fatal(err)
	}
	index := &scip.Index{}
	if err := proto.Unmarshal(data, index); err != nil {
		t.Fatal(err)
	}
	var keep *scip.Document
	for _, doc := range index.Documents {
		if doc.RelativePath == "Keep.cs" {
			keep = doc
			break
		}
	}
	if keep == nil {
		t.Fatalf("Keep.cs missing among %d documents", len(index.Documents))
	}
	resolved := false
	for _, occurrence := range keep.Occurrences {
		if strings.Contains(occurrence.Symbol, "Newtonsoft.Json") && strings.Contains(occurrence.Symbol, "JsonConvert") {
			resolved = true
		}
	}
	if !resolved {
		t.Fatalf("package symbol Newtonsoft.Json.JsonConvert was not resolved; occurrences=%v", keep.Occurrences)
	}
	if after := snapshotSCIPJavaFixture(t, root); !reflect.DeepEqual(before, after) {
		t.Fatalf("source tree changed: before=%v after=%v", before, after)
	}
	if _, err := exec.Command("docker", "container", "inspect", scipContainerName(root, "dotnet")).Output(); err == nil {
		t.Fatal(".NET container and overlay volume survived the parse")
	}
}

func TestSCIPDotnetDockerRestoreFailure(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_SCIP_DOCKER") != "1" {
		t.Skip("set GRAPHIT_TEST_SCIP_DOCKER=1 to run Docker integration")
	}
	useLocalSCIPDevImage(t)
	root, global := t.TempDir(), t.TempDir()
	writeSCIPJavaFixture(t, filepath.Join(root, "demo.csproj"), `<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup><ItemGroup><PackageReference Include="Graphit.Missing.Package" Version="9999.0.0" /></ItemGroup></Project>`)
	writeSCIPJavaFixture(t, filepath.Join(root, "NuGet.Config"), `<configuration><packageSources><clear /><add key="offline" value="/cache/empty-feed" /></packageSources></configuration>`)
	writeSCIPJavaFixture(t, filepath.Join(root, "Keep.cs"), "public class Keep {}\n")
	before := snapshotSCIPJavaFixture(t, root)
	t.Cleanup(func() { _ = exec.Command("docker", "container", "rm", "-f", scipContainerName(root, "dotnet")).Run() })
	if _, err := runSCIPImage(context.Background(), root, global, "dotnet"); err == nil {
		t.Fatal("unrestorable package produced a potentially degraded SCIP index")
	}
	if _, err := exec.Command("docker", "container", "inspect", scipContainerName(root, "dotnet")).Output(); err == nil {
		t.Fatal("failed restore left its container and overlay volume")
	}
	if after := snapshotSCIPJavaFixture(t, root); !reflect.DeepEqual(before, after) {
		t.Fatalf("source changed on restore failure: before=%v after=%v", before, after)
	}
	if _, err := os.Stat(filepath.Join(scipOutputDir(root, global, "dotnet"), "index.scip")); !os.IsNotExist(err) {
		t.Fatalf("failed restore left a SCIP output: %v", err)
	}
}

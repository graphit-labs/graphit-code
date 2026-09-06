package hubaccess

import (
	"fmt"
	"regexp"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/oklog/ulid/v2"
)

const VersionPrefix = "v2"

var (
	projectNamePattern = regexp.MustCompile(`^[a-z0-9](?:[a-z0-9._-]{0,62}[a-z0-9])?$`)
	namePrefixPattern  = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,63}$`)
	subjectIDPattern   = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._@+-]{0,127}$`)
)

func ValidateProjectID(projectID string) error {
	parsed, err := ulid.ParseStrict(strings.TrimSpace(projectID))
	if err != nil || parsed.String() != projectID {
		return fmt.Errorf("invalid project ULID %q", projectID)
	}
	return nil
}

func NormalizeProjectName(name string) (string, error) {
	name = strings.ToLower(strings.TrimSpace(name))
	if !projectNamePattern.MatchString(name) {
		return "", fmt.Errorf("invalid project name %q", name)
	}
	return name, nil
}

func NormalizeNamePrefix(prefix string) (string, error) {
	prefix = strings.ToLower(strings.TrimSpace(prefix))
	if !namePrefixPattern.MatchString(prefix) {
		return "", fmt.Errorf("invalid project name prefix %q", prefix)
	}
	return prefix, nil
}

func ValidateSubjectID(kind, id string) error {
	if !subjectIDPattern.MatchString(id) {
		return fmt.Errorf("invalid %s ID %q", kind, id)
	}
	return nil
}

func ProjectRoot(projectID string) string {
	if ValidateProjectID(projectID) != nil {
		return ""
	}
	return s3store.JoinKey(VersionPrefix, "projects", projectID)
}

func ProjectMetadataKey(projectID string) string {
	return joinProjectKey(projectID, "project.json")
}

func ProjectRegistryPrefix(projectID string) string {
	return joinProjectKey(projectID, "registry")
}

func ProjectRegistryKey(projectID, artifactType, artifactID string) string {
	if !validPathSegment(artifactType) || !validPathSegment(artifactID) {
		return ""
	}
	return joinProjectKey(projectID, "registry", artifactType, artifactID+".json")
}

func ProjectArtifactPrefix(projectID, folder, artifactID, version string) string {
	if !validPathSegment(folder) || !validPathSegment(artifactID) || !validPathSegment(version) {
		return ""
	}
	return joinProjectKey(projectID, "artifacts", folder, artifactID, version)
}

func ProjectEventsPrefix(projectID string) string {
	return joinProjectKey(projectID, "events")
}

func ProjectMemoryPrefix(projectID string) string {
	return joinProjectKey(projectID, "memory")
}

func ProjectTaskPrefix(projectID, taskPrefix string) string {
	taskPrefix = strings.Trim(strings.TrimSpace(taskPrefix), "/")
	if taskPrefix == "" {
		return ""
	}
	for _, segment := range strings.Split(taskPrefix, "/") {
		if !validPathSegment(segment) {
			return ""
		}
	}
	return joinProjectKey(projectID, taskPrefix)
}

func NameDirectoryPrefix() string {
	return s3store.JoinKey(VersionPrefix, "registry", "names") + "/"
}

func NameRecordKey(normalizedName string) string {
	name, err := NormalizeProjectName(normalizedName)
	if err != nil || name != normalizedName {
		return ""
	}
	return NameDirectoryPrefix() + normalizedName + ".json"
}

func GlobalProjectsKey() string {
	return s3store.JoinKey(VersionPrefix, "global", "projects.json")
}

func UserProjectsKey(userID string) string {
	if ValidateSubjectID("user", userID) != nil {
		return ""
	}
	return s3store.JoinKey(VersionPrefix, "users", userID, "projects.json")
}

func TeamProjectsKey(teamID string) string {
	if ValidateSubjectID("team", teamID) != nil {
		return ""
	}
	return s3store.JoinKey(VersionPrefix, "teams", teamID, "projects.json")
}

func GlobalRulesPrefix() string {
	return s3store.JoinKey(VersionPrefix, "global", "rules")
}

func BaselinesKey() string {
	return s3store.JoinKey(VersionPrefix, "registry", "baselines.json")
}

func UserMemoryPrefix(userID string) string {
	if ValidateSubjectID("user", userID) != nil {
		return ""
	}
	return s3store.JoinKey(VersionPrefix, "users", userID, "memory")
}

func joinProjectKey(projectID string, parts ...string) string {
	root := ProjectRoot(projectID)
	if root == "" {
		return ""
	}
	return s3store.JoinKey(append([]string{root}, parts...)...)
}

func validPathSegment(value string) bool {
	value = strings.TrimSpace(value)
	return value != "" && value != "." && value != ".." && !strings.ContainsAny(value, `/\\`)
}

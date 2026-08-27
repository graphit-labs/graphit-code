package s3store

import "strings"

// JoinKey builds an object key from parts, collapsing the empty ones so a caller does not
// have to know whether the configured prefix was set.
func JoinKey(parts ...string) string {
	kept := make([]string, 0, len(parts))
	for _, p := range parts {
		p = strings.Trim(strings.TrimSpace(p), "/")
		if p != "" {
			kept = append(kept, p)
		}
	}
	return strings.Join(kept, "/")
}

// URI renders the s3:// form that LadybugDB accepts as a table's `storage` and that LanceDB
// accepts as a connection target. Both engines want the bucket as the authority, so this is
// deliberately not an HTTPS endpoint URL.
func URI(bucket, key string) string {
	if bucket == "" {
		return ""
	}
	if key == "" {
		return "s3://" + bucket
	}
	return "s3://" + bucket + "/" + key
}

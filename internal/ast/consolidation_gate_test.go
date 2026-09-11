package ast

type gateEntity struct {
	uid, name, docstring, entityType, path string
}

func gateCorpus() []gateEntity {
	return []gateEntity{
		{"u1", "parseConfig", "Parses the configuration file into a Config struct.", "Function", "config.go"},
		{"u2", "Config", "Configuration for the parser.", "Struct", "config.go"},
		{"u3", "loadUserConfig", "Loads user level configuration overrides.", "Function", "user.go"},
		{"u4", "validateSchema", "Validates the database schema before migration.", "Function", "schema.go"},
		{"u5", "SchemaValidator", "Validates schemas.", "Class", "schema.go"},
		{"u6", "connectDatabase", "Opens a connection to the database.", "Function", "db.go"},
		{"u7", "closeDatabase", "Closes the database connection.", "Function", "db.go"},
		{"u8", "retryPolicy", "Retry policy with exponential backoff for network calls.", "Struct", "retry.go"},
		{"u9", "computeChecksum", "Computes a checksum of the payload.", "Function", "hash.go"},
		{"u10", "parseSQL", "Parses a SQL statement into an AST.", "Function", "sql.go"},
	}
}

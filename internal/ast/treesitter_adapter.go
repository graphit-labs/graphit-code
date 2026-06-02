package ast

import (
	"context"
	"fmt"
	"strings"

	sitter "github.com/smacker/go-tree-sitter"
	"github.com/smacker/go-tree-sitter/c"
	"github.com/smacker/go-tree-sitter/cpp"
	"github.com/smacker/go-tree-sitter/csharp"
	"github.com/smacker/go-tree-sitter/golang"
	"github.com/smacker/go-tree-sitter/java"
	"github.com/smacker/go-tree-sitter/javascript"
	"github.com/smacker/go-tree-sitter/kotlin"
	"github.com/smacker/go-tree-sitter/php"
	"github.com/smacker/go-tree-sitter/python"
	"github.com/smacker/go-tree-sitter/ruby"
	"github.com/smacker/go-tree-sitter/rust"
	"github.com/smacker/go-tree-sitter/sql"
	"github.com/smacker/go-tree-sitter/swift"
	"github.com/smacker/go-tree-sitter/typescript/tsx"
	"github.com/smacker/go-tree-sitter/typescript/typescript"

	dart "github.com/graphit-labs/graphit-code/internal/ast/lang_dart"
)

type tsLangConfig struct {
	Language   string
	Extensions []string
	TSLang     *sitter.Language
	Queries    []tsQueryDef
}

type tsQueryDef struct {
	DataKey     string
	GraphLabel  string
	Pattern     string
	NameCapture string
}

var treeSitterLangs = []tsLangConfig{
	{
		Language:   "javascript",
		Extensions: []string{".js", ".jsx", ".mjs"},
		TSLang:     javascript.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(method_definition name: (property_identifier) @name)`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(variable_declarator name: (identifier) @name value: (arrow_function))`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(variable_declarator name: (identifier) @name value: (function_expression))`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(import_statement source: (string) @name)`,
			},
			{
				DataKey: "exports", GraphLabel: "Export", NameCapture: "name",
				Pattern: `(export_statement declaration: (function_declaration name: (identifier) @name))`,
			},

			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(lexical_declaration (variable_declarator name: (identifier) @name))`,
			},

			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(export_statement source: (string) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (member_expression property: (property_identifier) @name))`,
			},
			{
				DataKey: "instantiations", GraphLabel: "", NameCapture: "name",
				Pattern: `(new_expression constructor: (identifier) @name)`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(formal_parameters (identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(class_heritage (identifier) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(field_definition property: (property_identifier) @name)`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(member_expression property: (property_identifier) @name)`,
			},

			{
				DataKey: "field_writes", GraphLabel: "", NameCapture: "name",
				Pattern: `(assignment_expression left: (member_expression property: (property_identifier) @name))`,
			},
		},
	},
	{
		Language:   "typescript",
		Extensions: []string{".ts"},
		TSLang:     typescript.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(method_definition name: (property_identifier) @name)`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(variable_declarator name: (identifier) @name value: (arrow_function))`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_declaration name: (type_identifier) @name)`,
			},
			{
				DataKey: "interfaces", GraphLabel: "Interface", NameCapture: "name",
				Pattern: `(interface_declaration name: (type_identifier) @name)`,
			},
			{
				DataKey: "types", GraphLabel: "Type", NameCapture: "name",
				Pattern: `(type_alias_declaration name: (type_identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(enum_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(import_statement source: (string) @name)`,
			},

			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(lexical_declaration (variable_declarator name: (identifier) @name))`,
			},

			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(export_statement source: (string) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (member_expression property: (property_identifier) @name))`,
			},
			{
				DataKey: "instantiations", GraphLabel: "", NameCapture: "name",
				Pattern: `(new_expression constructor: (identifier) @name)`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(required_parameter pattern: (identifier) @name)`,
			},
			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(optional_parameter pattern: (identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(extends_clause value: (identifier) @name)`,
			},

			{
				DataKey: "implements", GraphLabel: "", NameCapture: "name",
				Pattern: `(implements_clause (type_identifier) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(public_field_definition name: (property_identifier) @name)`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(member_expression property: (property_identifier) @name)`,
			},

			{
				DataKey: "field_writes", GraphLabel: "", NameCapture: "name",
				Pattern: `(assignment_expression left: (member_expression property: (property_identifier) @name))`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(decorator (identifier) @name)`,
			},
			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(decorator (call_expression function: (identifier) @name))`,
			},
		},
	},

	{
		Language:   "typescript",
		Extensions: []string{".tsx"},
		TSLang:     tsx.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(method_definition name: (property_identifier) @name)`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(variable_declarator name: (identifier) @name value: (arrow_function))`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_declaration name: (type_identifier) @name)`,
			},
			{
				DataKey: "interfaces", GraphLabel: "Interface", NameCapture: "name",
				Pattern: `(interface_declaration name: (type_identifier) @name)`,
			},
			{
				DataKey: "types", GraphLabel: "Type", NameCapture: "name",
				Pattern: `(type_alias_declaration name: (type_identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(enum_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(import_statement source: (string) @name)`,
			},

			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(lexical_declaration (variable_declarator name: (identifier) @name))`,
			},

			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(export_statement source: (string) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (member_expression property: (property_identifier) @name))`,
			},
			{
				DataKey: "instantiations", GraphLabel: "", NameCapture: "name",
				Pattern: `(new_expression constructor: (identifier) @name)`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(required_parameter pattern: (identifier) @name)`,
			},
			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(optional_parameter pattern: (identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(extends_clause value: (identifier) @name)`,
			},

			{
				DataKey: "implements", GraphLabel: "", NameCapture: "name",
				Pattern: `(implements_clause (type_identifier) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(public_field_definition name: (property_identifier) @name)`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(member_expression property: (property_identifier) @name)`,
			},

			{
				DataKey: "field_writes", GraphLabel: "", NameCapture: "name",
				Pattern: `(assignment_expression left: (member_expression property: (property_identifier) @name))`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(decorator (identifier) @name)`,
			},
			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(decorator (call_expression function: (identifier) @name))`,
			},
		},
	},
	{
		Language:   "csharp",
		Extensions: []string{".cs"},
		TSLang:     csharp.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(method_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "interfaces", GraphLabel: "Interface", NameCapture: "name",
				Pattern: `(interface_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(enum_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "structs", GraphLabel: "Struct", NameCapture: "name",
				Pattern: `(struct_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "properties", GraphLabel: "Property", NameCapture: "name",
				Pattern: `(property_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "namespaces", GraphLabel: "Namespace", NameCapture: "name",
				Pattern: `(namespace_declaration name: (identifier) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(invocation_expression function: (identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(invocation_expression function: (member_access_expression name: (identifier) @name))`,
			},
			{
				DataKey: "instantiations", GraphLabel: "", NameCapture: "name",
				Pattern: `(object_creation_expression type: (identifier) @name)`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameter name: (identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(base_list (identifier) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(field_declaration (variable_declaration (variable_declarator (identifier) @name)))`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(member_access_expression name: (identifier) @name)`,
			},

			{
				DataKey: "field_writes", GraphLabel: "", NameCapture: "name",
				Pattern: `(assignment_expression left: (member_access_expression name: (identifier) @name))`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(attribute (identifier) @name)`,
			},
		},
	},
	{
		Language:   "php",
		Extensions: []string{".php"},
		TSLang:     php.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_definition name: (name) @name)`,
			},
			{
				DataKey: "methods", GraphLabel: "Method", NameCapture: "name",
				Pattern: `(method_declaration name: (name) @name)`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_declaration name: (name) @name)`,
			},
			{
				DataKey: "interfaces", GraphLabel: "Interface", NameCapture: "name",
				Pattern: `(interface_declaration name: (name) @name)`,
			},
			{
				DataKey: "traits", GraphLabel: "Trait", NameCapture: "name",
				Pattern: `(trait_declaration name: (name) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(enum_declaration name: (name) @name)`,
			},
			{
				DataKey: "constants", GraphLabel: "Constant", NameCapture: "name",
				Pattern: `(const_declaration (const_element (name) @name))`,
			},
			{
				DataKey: "namespaces", GraphLabel: "Package", NameCapture: "name",
				Pattern: `(namespace_definition name: (namespace_name) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(function_call_expression function: (name) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(member_call_expression name: (name) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(scoped_call_expression name: (name) @name)`,
			},
			{
				DataKey: "instantiations", GraphLabel: "", NameCapture: "name",
				Pattern: `(object_creation_expression (name) @name)`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(simple_parameter name: (variable_name (name) @name))`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(base_clause (name) @name)`,
			},

			{
				DataKey: "implements", GraphLabel: "", NameCapture: "name",
				Pattern: `(class_interface_clause (name) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(property_declaration name: (variable_name (name) @name))`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(member_access_expression name: (name) @name)`,
			},

			{
				DataKey: "field_writes", GraphLabel: "", NameCapture: "name",
				Pattern: `(assignment_expression left: (member_access_expression name: (name) @name))`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(attribute (name) @name)`,
			},
		},
	},
	{
		Language:   "go",
		Extensions: []string{".go"},
		TSLang:     golang.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "methods", GraphLabel: "Method", NameCapture: "name",
				Pattern: `(method_declaration name: (field_identifier) @name)`,
			},
			{
				DataKey: "structs", GraphLabel: "Struct", NameCapture: "name",
				Pattern: `(type_declaration (type_spec name: (type_identifier) @name type: (struct_type)))`,
			},
			{
				DataKey: "interfaces", GraphLabel: "Interface", NameCapture: "name",
				Pattern: `(type_declaration (type_spec name: (type_identifier) @name type: (interface_type)))`,
			},
			{
				DataKey: "types", GraphLabel: "Type", NameCapture: "name",
				Pattern: `(type_declaration (type_spec name: (type_identifier) @name))`,
			},
			{
				DataKey: "constants", GraphLabel: "Constant", NameCapture: "name",
				Pattern: `(const_spec name: (identifier) @name)`,
			},
			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(var_spec name: (identifier) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (selector_expression field: (field_identifier) @name))`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameter_declaration name: (identifier) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(field_declaration name: (field_identifier) @name)`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(selector_expression field: (field_identifier) @name)`,
			},

			{
				DataKey: "field_writes", GraphLabel: "", NameCapture: "name",
				Pattern: `(assignment_statement left: (selector_expression field: (field_identifier) @name))`,
			},
		},
	},
	{
		Language:   "sql",
		Extensions: []string{".sql"},
		TSLang:     sql.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(create_function_statement name: (identifier) @name)`,
			},
			{
				DataKey: "tables", GraphLabel: "Table", NameCapture: "name",
				Pattern: `(create_table_statement name: (identifier) @name)`,
			},
			{
				DataKey: "views", GraphLabel: "View", NameCapture: "name",
				Pattern: `(create_view_statement name: (identifier) @name)`,
			},
		},
	},

	{
		Language:   "python",
		Extensions: []string{".py"},
		TSLang:     python.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_definition name: (identifier) @name)`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_definition name: (identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(import_statement name: (dotted_name) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(import_from_statement module_name: (dotted_name) @name)`,
			},

			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(assignment left: (identifier) @name)`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(decorator (identifier) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call function: (identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call function: (attribute attribute: (identifier) @name))`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameters (identifier) @name)`,
			},
			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameters (default_parameter name: (identifier) @name))`,
			},
			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameters (typed_parameter (identifier) @name))`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(class_definition superclasses: (argument_list (identifier) @name))`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(attribute attribute: (identifier) @name)`,
			},

			{
				DataKey: "field_writes", GraphLabel: "", NameCapture: "name",
				Pattern: `(assignment left: (attribute attribute: (identifier) @name))`,
			},
		},
	},

	{
		Language:   "java",
		Extensions: []string{".java"},
		TSLang:     java.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(method_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(constructor_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "interfaces", GraphLabel: "Interface", NameCapture: "name",
				Pattern: `(interface_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(enum_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(import_declaration (scoped_identifier) @name)`,
			},
			{
				DataKey: "packages", GraphLabel: "Package", NameCapture: "name",
				Pattern: `(package_declaration (scoped_identifier) @name)`,
			},

			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(local_variable_declaration declarator: (variable_declarator name: (identifier) @name))`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(marker_annotation name: (identifier) @name)`,
			},
			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(annotation name: (identifier) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(method_invocation name: (identifier) @name)`,
			},
			{
				DataKey: "instantiations", GraphLabel: "", NameCapture: "name",
				Pattern: `(object_creation_expression type: (type_identifier) @name)`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(formal_parameter name: (identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(superclass (type_identifier) @name)`,
			},

			{
				DataKey: "implements", GraphLabel: "", NameCapture: "name",
				Pattern: `(super_interfaces (type_list (type_identifier) @name))`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(field_declaration declarator: (variable_declarator name: (identifier) @name))`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(field_access field: (identifier) @name)`,
			},

			{
				DataKey: "field_writes", GraphLabel: "", NameCapture: "name",
				Pattern: `(assignment_expression left: (field_access field: (identifier) @name))`,
			},
		},
	},

	{
		Language:   "rust",
		Extensions: []string{".rs"},
		TSLang:     rust.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_item name: (identifier) @name)`,
			},
			{
				DataKey: "structs", GraphLabel: "Struct", NameCapture: "name",
				Pattern: `(struct_item name: (type_identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(enum_item name: (type_identifier) @name)`,
			},
			{
				DataKey: "traits", GraphLabel: "Trait", NameCapture: "name",
				Pattern: `(trait_item name: (type_identifier) @name)`,
			},
			{
				DataKey: "types", GraphLabel: "Type", NameCapture: "name",
				Pattern: `(type_item name: (type_identifier) @name)`,
			},
			{
				DataKey: "constants", GraphLabel: "Constant", NameCapture: "name",
				Pattern: `(const_item name: (identifier) @name)`,
			},
			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(static_item name: (identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(use_declaration argument: (scoped_identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(use_declaration argument: (identifier) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (scoped_identifier name: (identifier) @name))`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (field_expression field: (field_identifier) @name))`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameter pattern: (identifier) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(field_declaration name: (field_identifier) @name)`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(field_expression field: (field_identifier) @name)`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(attribute_item (attribute (identifier) @name))`,
			},

			{
				DataKey: "implements", GraphLabel: "", NameCapture: "name",
				Pattern: `(impl_item trait: (type_identifier) @name)`,
			},
		},
	},

	{
		Language:   "c",
		Extensions: []string{".c", ".h"},
		TSLang:     c.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_definition declarator: (function_declarator declarator: (identifier) @name))`,
			},
			{
				DataKey: "structs", GraphLabel: "Struct", NameCapture: "name",
				Pattern: `(struct_specifier name: (type_identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(enum_specifier name: (type_identifier) @name)`,
			},
			{
				DataKey: "types", GraphLabel: "Type", NameCapture: "name",
				Pattern: `(type_definition declarator: (type_identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(preproc_include path: (string_literal) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(preproc_include path: (system_lib_string) @name)`,
			},

			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(declaration declarator: (init_declarator declarator: (identifier) @name))`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (identifier) @name)`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameter_declaration declarator: (identifier) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(field_declaration declarator: (field_identifier) @name)`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(field_expression field: (field_identifier) @name)`,
			},
		},
	},

	{
		Language:   "cpp",
		Extensions: []string{".cpp", ".hpp", ".cc", ".cxx"},
		TSLang:     cpp.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_definition declarator: (function_declarator declarator: (identifier) @name))`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_definition declarator: (function_declarator declarator: (qualified_identifier name: (identifier) @name)))`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_specifier name: (type_identifier) @name)`,
			},
			{
				DataKey: "structs", GraphLabel: "Struct", NameCapture: "name",
				Pattern: `(struct_specifier name: (type_identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(enum_specifier name: (type_identifier) @name)`,
			},
			{
				DataKey: "namespaces", GraphLabel: "Namespace", NameCapture: "name",
				Pattern: `(namespace_definition name: (identifier) @name)`,
			},
			{
				DataKey: "types", GraphLabel: "Type", NameCapture: "name",
				Pattern: `(type_definition declarator: (type_identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(preproc_include path: (string_literal) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(preproc_include path: (system_lib_string) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression function: (field_expression field: (field_identifier) @name))`,
			},
			{
				DataKey: "instantiations", GraphLabel: "", NameCapture: "name",
				Pattern: `(new_expression type: (type_identifier) @name)`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameter_declaration declarator: (identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(base_class_clause (type_identifier) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(field_declaration declarator: (field_identifier) @name)`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(field_expression field: (field_identifier) @name)`,
			},
		},
	},

	{
		Language:   "kotlin",
		Extensions: []string{".kt", ".kts"},
		TSLang:     kotlin.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_declaration (simple_identifier) @name)`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_declaration (type_identifier) @name)`,
			},
			{
				DataKey: "interfaces", GraphLabel: "Interface", NameCapture: "name",
				Pattern: `(class_declaration (type_identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(class_declaration (type_identifier) @name)`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(import_header (identifier) @name)`,
			},
			{
				DataKey: "packages", GraphLabel: "Package", NameCapture: "name",
				Pattern: `(package_header (identifier) @name)`,
			},

			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(property_declaration (variable_declaration (simple_identifier) @name))`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(annotation (user_type (type_identifier) @name))`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression (simple_identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression (navigation_expression (simple_identifier) @name))`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameter (simple_identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(delegation_specifier (user_type (type_identifier) @name))`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(class_body (property_declaration (variable_declaration (simple_identifier) @name)))`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(navigation_expression (simple_identifier) @name)`,
			},
		},
	},

	{
		Language:   "ruby",
		Extensions: []string{".rb"},
		TSLang:     ruby.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(method name: (identifier) @name)`,
			},
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(singleton_method name: (identifier) @name)`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class name: (constant) @name)`,
			},
			{
				DataKey: "modules", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(module name: (constant) @name)`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call method: (identifier) @name)`,
			},

			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(call method: (identifier) @_method arguments: (argument_list (string (string_content) @name)) (#eq? @_method "require"))`,
			},
			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(call method: (identifier) @_method arguments: (argument_list (string (string_content) @name)) (#eq? @_method "require_relative"))`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(class superclass: (superclass (constant) @name))`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(call method: (identifier) @_method arguments: (argument_list (constant) @name) (#eq? @_method "include"))`,
			},
			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(call method: (identifier) @_method arguments: (argument_list (constant) @name) (#eq? @_method "extend"))`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(method_parameters (identifier) @name)`,
			},

			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(assignment left: (constant) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(instance_variable) @name`,
			},
		},
	},

	{
		Language:   "swift",
		Extensions: []string{".swift"},
		TSLang:     swift.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_declaration name: (simple_identifier) @name)`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_declaration name: (type_identifier) @name)`,
			},
			{
				DataKey: "structs", GraphLabel: "Struct", NameCapture: "name",
				Pattern: `(class_declaration name: (type_identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(class_declaration name: (type_identifier) @name)`,
			},
			{
				DataKey: "protocols", GraphLabel: "Interface", NameCapture: "name",
				Pattern: `(protocol_declaration name: (type_identifier) @name)`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(attribute (user_type (type_identifier) @name))`,
			},

			{
				DataKey: "variables", GraphLabel: "Variable", NameCapture: "name",
				Pattern: `(property_declaration (pattern (simple_identifier) @name))`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression (simple_identifier) @name)`,
			},
			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(call_expression (navigation_expression (simple_identifier) @name))`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(parameter name: (simple_identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(inheritance_specifier (user_type (type_identifier) @name))`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(class_body (property_declaration (pattern (simple_identifier) @name)))`,
			},

			{
				DataKey: "field_reads", GraphLabel: "", NameCapture: "name",
				Pattern: `(navigation_expression (simple_identifier) @name)`,
			},
		},
	},

	{
		Language:   "dart",
		Extensions: []string{".dart"},
		TSLang:     dart.GetLanguage(),
		Queries: []tsQueryDef{
			{
				DataKey: "functions", GraphLabel: "Function", NameCapture: "name",
				Pattern: `(function_signature name: (identifier) @name)`,
			},
			{
				DataKey: "methods", GraphLabel: "Method", NameCapture: "name",
				Pattern: `(method_signature (function_signature name: (identifier) @name))`,
			},
			{
				DataKey: "classes", GraphLabel: "Class", NameCapture: "name",
				Pattern: `(class_definition name: (identifier) @name)`,
			},
			{
				DataKey: "enums", GraphLabel: "Enum", NameCapture: "name",
				Pattern: `(enum_declaration name: (identifier) @name)`,
			},
			{
				DataKey: "interfaces", GraphLabel: "Interface", NameCapture: "name",
				Pattern: `(mixin_declaration name: (identifier) @name)`,
			},

			{
				DataKey: "imports", GraphLabel: "Module", NameCapture: "name",
				Pattern: `(import_specification (configurable_uri (uri (string_literal) @name)))`,
			},

			{
				DataKey: "calls", GraphLabel: "", NameCapture: "name",
				Pattern: `(identifier) @name`,
			},

			{
				DataKey: "parameters", GraphLabel: "Parameter", NameCapture: "name",
				Pattern: `(formal_parameter name: (identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(superclass (type_identifier) @name)`,
			},

			{
				DataKey: "implements", GraphLabel: "", NameCapture: "name",
				Pattern: `(interfaces (type_identifier) @name)`,
			},

			{
				DataKey: "heritage", GraphLabel: "", NameCapture: "name",
				Pattern: `(mixins (type_identifier) @name)`,
			},

			{
				DataKey: "fields", GraphLabel: "Field", NameCapture: "name",
				Pattern: `(declaration (initialized_identifier (identifier) @name))`,
			},

			{
				DataKey: "decorators", GraphLabel: "", NameCapture: "name",
				Pattern: `(annotation name: (identifier) @name)`,
			},
		},
	},
}

var tsExtMap map[string]*tsLangConfig

func init() {
	tsExtMap = make(map[string]*tsLangConfig)
	for i := range treeSitterLangs {
		cfg := &treeSitterLangs[i]
		for _, ext := range cfg.Extensions {
			tsExtMap[ext] = cfg
		}
	}
}

type TreeSitterParser struct {
	projectDir string
}

func (t *TreeSitterParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfg, ok := tsExtMap[ext]
	if !ok {
		return nil, fmt.Errorf("no tree-sitter grammar for %s", ext)
	}

	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}

	parser := sitter.NewParser()
	parser.SetLanguage(cfg.TSLang)

	tree, err := parser.ParseCtx(context.Background(), nil, src)
	if err != nil {
		return nil, fmt.Errorf("tree-sitter parse %s: %w", path, err)
	}
	defer tree.Close()

	root := tree.RootNode()

	result := &ParsedFile{
		Path:     path,
		Language: cfg.Language,
		IsDepend: isDepend,
		Source:   string(src),
		Entities: make(map[string][]Entity),
	}

	// Load language config from YAML (includes queries + exports + context types etc)
	var langConfig *ExternalQueryFile
	queries := cfg.Queries
	if t.projectDir != "" {
		queries = mergedQueriesFor(t.projectDir, cfg.Language, ext, cfg.Queries, cfg.TSLang)
		langConfig = resolvedLangConfigFor(t.projectDir, cfg.Language, ext)
	}

	specificLabels := map[string]bool{
		"Struct": true, "Interface": true, "Class": true, "Trait": true, "Enum": true,
	}
	seenNames := map[string]bool{}

	for _, qdef := range queries {
		q, qErr := sitter.NewQuery([]byte(qdef.Pattern), cfg.TSLang)
		if qErr != nil {
			continue
		}

		qc := sitter.NewQueryCursor()
		qc.Exec(q, root)

		for {
			match, ok := qc.NextMatch()
			if !ok {
				break
			}

			for _, capture := range match.Captures {
				name := capture.Node.Content(src)
				if name == "" {
					continue
				}

				if qdef.DataKey == "imports" {
					name = strings.Trim(name, "'\"")
				}

				if !specificLabels[qdef.GraphLabel] && seenNames[name] {
					continue
				}

				startLine := int(capture.Node.StartPoint().Row) + 1
				endLine := int(capture.Node.EndPoint().Row) + 1

				parent := capture.Node.Parent()
				entitySource := ""
				complexity := 1
				if parent != nil {
					entitySource = parent.Content(src)
					complexity = ComputeCyclomaticComplexity(entitySource)
				}

				contextName, contextType := resolveParentContext(capture.Node, src, langConfig)

				result.AddEntity(qdef.DataKey, Entity{
					Name:        name,
					Line:        startLine,
					EndLine:     endLine,
					Source:      entitySource,
					GraphLabel:  qdef.GraphLabel,
					Complexity:  complexity,
					Context:     contextName,
					ContextType: contextType,
				})

				if specificLabels[qdef.GraphLabel] {
					seenNames[name] = true
				}
			}
		}
		q.Close()
	}

	extractDocstrings(root, src, result, langConfig)

	attachDecorators(result)

	detectExports(root, src, result, cfg.Language, langConfig)

	if callEntities, ok := result.Entities["calls"]; ok {
		for _, e := range callEntities {
			result.CallSites = append(result.CallSites, CallInfo{
				Name:       e.Name,
				Line:       e.Line,
				SourceName: e.Context,
				SourceType: e.ContextType,
			})
		}
		delete(result.Entities, "calls")
	}

	if instEntities, ok := result.Entities["instantiations"]; ok {
		for _, e := range instEntities {
			result.CallSites = append(result.CallSites, CallInfo{
				Name:       e.Name,
				Line:       e.Line,
				FullName:   "new:" + e.Name,
				SourceName: e.Context,
				SourceType: e.ContextType,
			})
		}
		delete(result.Entities, "instantiations")
	}

	if heritageEntities, ok := result.Entities["heritage"]; ok {
		for _, e := range heritageEntities {
			result.References = append(result.References, ReferenceInfo{
				TargetName: e.Name,
				RelType:    "INHERITS",
				Line:       e.Line,
				SourceName: e.Context,
			})
		}
		delete(result.Entities, "heritage")
	}

	if implEntities, ok := result.Entities["implements"]; ok {
		for _, e := range implEntities {
			result.References = append(result.References, ReferenceInfo{
				TargetName: e.Name,
				RelType:    "IMPLEMENTS",
				Line:       e.Line,
				SourceName: e.Context,
			})
		}
		delete(result.Entities, "implements")
	}

	if readEntities, ok := result.Entities["field_reads"]; ok {
		for _, e := range readEntities {
			if e.Context == "" {
				continue
			}
			result.References = append(result.References, ReferenceInfo{
				TargetName: e.Name,
				RelType:    "READS_FIELD",
				Line:       e.Line,
				SourceName: e.Context,
			})
		}
		delete(result.Entities, "field_reads")
	}

	if writeEntities, ok := result.Entities["field_writes"]; ok {
		for _, e := range writeEntities {
			if e.Context == "" {
				continue
			}
			result.References = append(result.References, ReferenceInfo{
				TargetName: e.Name,
				RelType:    "WRITES_FIELD",
				Line:       e.Line,
				SourceName: e.Context,
			})
		}
		delete(result.Entities, "field_writes")
	}

	resolveReceiverTypes(result, src, cfg.Language, langConfig)

	return result, nil
}

func resolveParentContext(node *sitter.Node, src []byte, langConfig *ExternalQueryFile) (string, string) {
	parentTypes := defaultContextTypes
	anonTypes := defaultAnonFuncTypes

	if langConfig != nil {
		if len(langConfig.ContextTypes) > 0 {
			parentTypes = langConfig.ContextTypes
		}
		if len(langConfig.AnonFuncTypes) > 0 {
			anonTypes = make(map[string]bool, len(langConfig.AnonFuncTypes))
			for _, t := range langConfig.AnonFuncTypes {
				anonTypes[t] = true
			}
		}
	}

	current := node.Parent()
	for current != nil {
		nodeType := current.Type()
		if label, ok := parentTypes[nodeType]; ok {

			nameNode := current.ChildByFieldName("name")
			if nameNode != nil {
				return nameNode.Content(src), label
			}
		}

		if anonTypes[nodeType] {
			grandparent := current.Parent()
			if grandparent != nil && grandparent.Type() == "variable_declarator" {
				nameNode := grandparent.ChildByFieldName("name")
				if nameNode != nil {
					return nameNode.Content(src), "Function"
				}
			}
		}
		current = current.Parent()
	}
	return "", ""
}

// Default context types (used when no YAML config is available)
var defaultContextTypes = map[string]string{
	"class_declaration":     "Class",
	"class_definition":      "Class",
	"interface_declaration": "Interface",
	"struct_declaration":    "Struct",
	"trait_declaration":     "Trait",
	"namespace_declaration": "Namespace",
	"enum_declaration":      "Enum",
	"function_declaration":  "Function",
	"function_definition":   "Function",
	"method_declaration":    "Method",
	"method_definition":     "Method",
}

var defaultAnonFuncTypes = map[string]bool{
	"arrow_function":      true,
	"function_expression": true,
	"function":            true,
}



func extractDocstrings(root *sitter.Node, src []byte, result *ParsedFile, langConfig *ExternalQueryFile) {
	if root == nil {
		return
	}

	// Build declaration and comment type sets from YAML config
	var declTypes map[string]bool
	var comTypes map[string]bool
	if langConfig != nil && len(langConfig.DeclarationTypes) > 0 {
		declTypes = make(map[string]bool, len(langConfig.DeclarationTypes))
		for _, dt := range langConfig.DeclarationTypes {
			declTypes[dt] = true
		}
	}
	if langConfig != nil && len(langConfig.CommentTypes) > 0 {
		comTypes = make(map[string]bool, len(langConfig.CommentTypes))
		for _, ct := range langConfig.CommentTypes {
			comTypes[ct] = true
		}
	}
	if declTypes == nil {
		return
	}
	if comTypes == nil {
		comTypes = defaultCommentTypes
	}

	type entityKey struct {
		line int
		name string
	}
	entityIdx := make(map[entityKey]*Entity)
	for dataKey := range result.Entities {
		for i := range result.Entities[dataKey] {
			e := &result.Entities[dataKey][i]
			if e.GraphLabel != "" {
				entityIdx[entityKey{e.Line, e.Name}] = e
			}
		}
	}

	var walk func(node *sitter.Node)
	walk = func(node *sitter.Node) {
		for i := 0; i < int(node.ChildCount()); i++ {
			child := node.Child(i)
			if child == nil {
				continue
			}

			nodeType := child.Type()
			if declTypes[nodeType] {

				if i > 0 {
					prev := node.Child(i - 1)
					if prev != nil && comTypes[prev.Type()] {
						commentText := cleanDocstring(prev.Content(src))
						if commentText != "" {
							declLine := int(child.StartPoint().Row) + 1

							nameNode := child.ChildByFieldName("name")
							if nameNode != nil {
								name := nameNode.Content(src)
								if e, ok := entityIdx[entityKey{declLine, name}]; ok {
									e.Docstring = commentText
								}
							}
						}
					}
				}

				if nodeType == "function_definition" || nodeType == "class_definition" {
					body := child.ChildByFieldName("body")
					if body != nil && body.ChildCount() > 0 {
						firstStmt := body.Child(0)
						if firstStmt != nil && firstStmt.Type() == "expression_statement" {
							if firstStmt.ChildCount() > 0 {
								expr := firstStmt.Child(0)
								if expr != nil && expr.Type() == "string" {
									declLine := int(child.StartPoint().Row) + 1
									nameNode := child.ChildByFieldName("name")
									if nameNode != nil {
										name := nameNode.Content(src)
										if e, ok := entityIdx[entityKey{declLine, name}]; ok && e.Docstring == "" {
											e.Docstring = cleanDocstring(expr.Content(src))
										}
									}
								}
							}
						}
					}
				}
			}

			walk(child)
		}
	}

	walk(root)
}

func isCommentNode(nodeType string) bool {
	return defaultCommentTypes[nodeType]
}

var defaultCommentTypes = map[string]bool{
	"comment":           true,
	"block_comment":     true,
	"line_comment":      true,
	"multiline_comment": true,
}

func cleanDocstring(raw string) string {
	lines := strings.Split(raw, "\n")
	var cleaned []string
	for _, line := range lines {
		line = strings.TrimSpace(line)

		for _, prefix := range []string{"///", "//!", "//", "/**", "*/", "*", "# ", "#", "\"\"\"", "'''", "/*"} {
			if strings.HasPrefix(line, prefix) {
				line = strings.TrimPrefix(line, prefix)
				line = strings.TrimSpace(line)
				break
			}
		}
		if line != "" {
			cleaned = append(cleaned, line)
		}
	}
	return strings.Join(cleaned, "\n")
}

func attachDecorators(result *ParsedFile) {
	decoratorEntities, ok := result.Entities["decorators"]
	if !ok || len(decoratorEntities) == 0 {
		return
	}

	type entityRef struct {
		dataKey string
		index   int
		line    int
	}
	var allEntities []entityRef
	for dk, entities := range result.Entities {
		if dk == "decorators" || dk == "calls" || dk == "instantiations" ||
			dk == "heritage" || dk == "implements" || dk == "field_reads" || dk == "field_writes" {
			continue
		}
		for i, e := range entities {
			if e.GraphLabel != "" {
				allEntities = append(allEntities, entityRef{dk, i, e.Line})
			}
		}
	}

	for _, dec := range decoratorEntities {
		bestIdx := -1
		bestDist := int(^uint(0) >> 1)
		for j, ref := range allEntities {
			dist := ref.line - dec.Line
			if dist >= 0 && dist < bestDist {
				bestDist = dist
				bestIdx = j
			}
		}
		if bestIdx >= 0 {
			ref := allEntities[bestIdx]
			e := &result.Entities[ref.dataKey][ref.index]
			if e.Properties == nil {
				e.Properties = make(map[string]string)
			}
			if existing := e.Properties["decorators"]; existing != "" {
				e.Properties["decorators"] = existing + "," + dec.Name
			} else {
				e.Properties["decorators"] = dec.Name
			}
		}
	}

	delete(result.Entities, "decorators")
}

func resolveReceiverTypes(result *ParsedFile, src []byte, lang string, langConfig *ExternalQueryFile) {
	if len(result.CallSites) == 0 {
		return
	}

	lines := strings.Split(string(src), "\n")

	methodToClass := make(map[string]string)
	for _, dataKey := range []string{"functions", "methods"} {
		for _, e := range result.Entities[dataKey] {
			if e.Context != "" && (e.ContextType == "Class" || e.ContextType == "Struct") {
				methodToClass[e.Name] = e.Context
			}
		}
	}

	selfKeywords := selfKeywordsForLang(lang, langConfig)

	for i := range result.CallSites {
		call := &result.CallSites[i]

		if strings.HasPrefix(call.FullName, "new:") {
			call.ReceiverType = strings.TrimPrefix(call.FullName, "new:")
			continue
		}

		if call.SourceName != "" && len(selfKeywords) > 0 {
			className := methodToClass[call.SourceName]
			if className == "" {
				continue
			}

			lineIdx := call.Line - 1
			if lineIdx < 0 || lineIdx >= len(lines) {
				continue
			}
			lineText := lines[lineIdx]

			for _, kw := range selfKeywords {
				if strings.Contains(lineText, kw+call.Name) {
					call.ReceiverType = className
					break
				}
			}
		}
	}
}

func selfKeywordsForLang(lang string, langConfig *ExternalQueryFile) []string {
	_ = lang
	if langConfig != nil && langConfig.SelfKeywords != nil {
		return langConfig.SelfKeywords
	}
	return nil
}

func detectExports(root *sitter.Node, src []byte, result *ParsedFile, lang string, langConfig *ExternalQueryFile) {

	exportedNames := make(map[string]bool)

	// Determine export strategy from YAML config or hardcoded fallback
	var strategy string
	var stratConfig map[string]string
	var stratConfigList map[string][]string

	if langConfig != nil && langConfig.Exports != nil {
		strategy = langConfig.Exports.Strategy
		stratConfig = langConfig.Exports.Config
		stratConfigList = langConfig.Exports.ConfigList
	} else {
		strategy = "none"
	}

	// Pre-scan for export_statement strategy
	if strategy == "export_statement" && root != nil {
		for i := 0; i < int(root.ChildCount()); i++ {
			child := root.Child(i)
			if child == nil {
				continue
			}
			if child.Type() == "export_statement" {
				decl := child.ChildByFieldName("declaration")
				if decl != nil {
					nameNode := decl.ChildByFieldName("name")
					if nameNode != nil {
						exportedNames[nameNode.Content(src)] = true
					}
				}

				for j := 0; j < int(child.ChildCount()); j++ {
					spec := child.Child(j)
					if spec != nil && spec.Type() == "export_clause" {
						for k := 0; k < int(spec.ChildCount()); k++ {
							es := spec.Child(k)
							if es != nil && es.Type() == "export_specifier" {
								nameNode := es.ChildByFieldName("name")
								if nameNode != nil {
									exportedNames[nameNode.Content(src)] = true
								}
							}
						}
					}
				}
			}
		}
	}

	for dataKey := range result.Entities {
		if dataKey == "calls" || dataKey == "instantiations" ||
			dataKey == "heritage" || dataKey == "implements" || dataKey == "field_reads" || dataKey == "field_writes" {
			continue
		}
		for i := range result.Entities[dataKey] {
			e := &result.Entities[dataKey][i]
			if e.GraphLabel == "" || e.Name == "" {
				continue
			}

			exported := isExported(strategy, e, exportedNames, stratConfig, stratConfigList)

			if exported {
				if e.Properties == nil {
					e.Properties = make(map[string]string)
				}
				e.Properties["is_exported"] = "true"
			}
		}
	}

	delete(result.Entities, "exports")
}

// isExported determines if an entity is exported based on the export strategy.
func isExported(strategy string, e *Entity, exportedNames map[string]bool, config map[string]string, configList map[string][]string) bool {
	switch strategy {
	case "capitalized_name":
		return len(e.Name) > 0 && e.Name[0] >= 'A' && e.Name[0] <= 'Z'

	case "no_prefix":
		prefix := config["prefix"]
		if prefix == "" {
			prefix = "_"
		}
		return len(e.Name) > 0 && !strings.HasPrefix(e.Name, prefix)

	case "export_statement":
		return exportedNames[e.Name]

	case "modifier":
		keyword := config["keyword"]
		if keyword == "" {
			return false
		}
		return e.Source != "" && containsModifier(e.Source, keyword)

	case "no_modifier":
		keywords := configList["keywords"]
		if len(keywords) == 0 {
			return true
		}
		if e.Source == "" {
			return true
		}
		for _, kw := range keywords {
			if containsModifier(e.Source, kw) {
				return false
			}
		}
		return true

	case "no_static":
		return e.Source != "" && !containsModifier(e.Source, "static")

	case "none":
		return false

	default:
		return false
	}
}

func containsModifier(source, modifier string) bool {

	check := source
	if len(check) > 200 {
		check = check[:200]
	}
	if idx := strings.Index(check, "\n"); idx > 0 {
		check = check[:idx]
	}
	return strings.Contains(check, modifier+" ") || strings.Contains(check, modifier+"\t") ||
		strings.HasPrefix(strings.TrimSpace(check), modifier)
}

func HasTreeSitterForExtension(ext string) bool {
	_, ok := tsExtMap[strings.ToLower(ext)]
	return ok
}

func TreeSitterLangForExtension(ext string) string {
	if cfg, ok := tsExtMap[strings.ToLower(ext)]; ok {
		return cfg.Language
	}
	return ""
}

func GetTreeSitterParser(ext string, projectDir ...string) *TreeSitterParser {
	if HasTreeSitterForExtension(ext) {
		pd := ""
		if len(projectDir) > 0 {
			pd = projectDir[0]
		}
		return &TreeSitterParser{projectDir: pd}
	}
	return nil
}

func TreeSitterSupportedExtensions() []string {
	var exts []string
	for ext := range tsExtMap {
		exts = append(exts, ext)
	}
	return exts
}

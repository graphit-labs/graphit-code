//! Compile the closed builtin catalog and derive its Rust dispatch from one list.
use std::{
    env,
    error::Error,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

struct Grammar {
    name: &'static str,
    abi: usize,
    backend: Backend,
}
enum Backend {
    Sdk(&'static str),
    C {
        directory: &'static str,
        parser: &'static str,
        scanner: Option<&'static str>,
        include: &'static str,
        export: &'static str,
    },
}
const GRAMMARS: &[Grammar] = &[
    Grammar {
        name: "tree-sitter-bash",
        abi: 15,
        backend: Backend::C {
            directory: "bash",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_bash",
        },
    },
    Grammar {
        name: "tree-sitter-c",
        abi: 15,
        backend: Backend::C {
            directory: "c",
            parser: "src/parser.c",
            scanner: None,
            include: "src",
            export: "tree_sitter_c",
        },
    },
    Grammar {
        name: "tree-sitter-c_sharp",
        abi: 15,
        backend: Backend::C {
            directory: "c-sharp",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_c_sharp",
        },
    },
    Grammar {
        name: "tree-sitter-clojure",
        abi: 14,
        backend: Backend::C {
            directory: "clojure",
            parser: "parser.c.inc",
            scanner: None,
            include: "",
            export: "tree_sitter_clojure",
        },
    },
    Grammar {
        name: "tree-sitter-cpp",
        abi: 14,
        backend: Backend::C {
            directory: "cpp",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_cpp",
        },
    },
    Grammar {
        name: "tree-sitter-css",
        abi: 15,
        backend: Backend::C {
            directory: "css",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_css",
        },
    },
    Grammar {
        name: "tree-sitter-dart",
        abi: 15,
        backend: Backend::C {
            directory: "dart",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_dart",
        },
    },
    Grammar {
        name: "tree-sitter-dockerfile",
        abi: 14,
        backend: Backend::C {
            directory: "dockerfile",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_dockerfile",
        },
    },
    Grammar {
        name: "tree-sitter-elixir",
        abi: 14,
        backend: Backend::C {
            directory: "elixir",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_elixir",
        },
    },
    Grammar {
        name: "tree-sitter-go",
        abi: 15,
        backend: Backend::C {
            directory: "go",
            parser: "src/parser.c",
            scanner: None,
            include: "src",
            export: "tree_sitter_go",
        },
    },
    Grammar {
        name: "tree-sitter-graphql",
        abi: 13,
        backend: Backend::C {
            directory: "graphql",
            parser: "parser.c.inc",
            scanner: None,
            include: "",
            export: "tree_sitter_graphql",
        },
    },
    Grammar {
        name: "tree-sitter-groovy",
        abi: 15,
        backend: Backend::C {
            directory: "groovy",
            parser: "parser.c.inc",
            scanner: None,
            include: "",
            export: "tree_sitter_groovy",
        },
    },
    Grammar {
        name: "tree-sitter-haskell",
        abi: 14,
        backend: Backend::C {
            directory: "haskell",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_haskell",
        },
    },
    Grammar {
        name: "tree-sitter-hcl",
        abi: 15,
        backend: Backend::C {
            directory: "hcl",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_hcl",
        },
    },
    Grammar {
        name: "tree-sitter-html",
        abi: 14,
        backend: Backend::C {
            directory: "html",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_html",
        },
    },
    Grammar {
        name: "tree-sitter-java",
        abi: 14,
        backend: Backend::C {
            directory: "java",
            parser: "src/parser.c",
            scanner: None,
            include: "src",
            export: "tree_sitter_java",
        },
    },
    Grammar {
        name: "tree-sitter-javascript",
        abi: 15,
        backend: Backend::C {
            directory: "javascript",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_javascript",
        },
    },
    Grammar {
        name: "tree-sitter-json",
        abi: 14,
        backend: Backend::Sdk("tree_sitter_json"),
    },
    Grammar {
        name: "tree-sitter-julia",
        abi: 15,
        backend: Backend::C {
            directory: "julia",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_julia",
        },
    },
    Grammar {
        name: "tree-sitter-kotlin",
        abi: 14,
        backend: Backend::C {
            directory: "kotlin",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_kotlin",
        },
    },
    Grammar {
        name: "tree-sitter-lua",
        abi: 15,
        backend: Backend::C {
            directory: "lua",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_lua",
        },
    },
    Grammar {
        name: "tree-sitter-markdown",
        abi: 15,
        backend: Backend::C {
            directory: "markdown",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_markdown",
        },
    },
    Grammar {
        name: "tree-sitter-objc",
        abi: 14,
        backend: Backend::C {
            directory: "objc",
            parser: "parser.c.inc",
            scanner: None,
            include: "",
            export: "tree_sitter_objc",
        },
    },
    Grammar {
        name: "tree-sitter-php",
        abi: 15,
        backend: Backend::C {
            directory: "php",
            parser: "php/src/parser.c",
            scanner: Some("php/src/scanner.c"),
            include: "php/src",
            export: "tree_sitter_php",
        },
    },
    Grammar {
        name: "tree-sitter-plpgsql",
        abi: 15,
        backend: Backend::C {
            directory: "plpgsql",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_plpgsql",
        },
    },
    Grammar {
        name: "tree-sitter-proto",
        abi: 15,
        backend: Backend::C {
            directory: "proto",
            parser: "parser.c.inc",
            scanner: None,
            include: "",
            export: "tree_sitter_proto",
        },
    },
    Grammar {
        name: "tree-sitter-python",
        abi: 15,
        backend: Backend::C {
            directory: "python",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_python",
        },
    },
    Grammar {
        name: "tree-sitter-r",
        abi: 14,
        backend: Backend::C {
            directory: "r",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_r",
        },
    },
    Grammar {
        name: "tree-sitter-ruby",
        abi: 14,
        backend: Backend::C {
            directory: "ruby",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_ruby",
        },
    },
    Grammar {
        name: "tree-sitter-rust",
        abi: 15,
        backend: Backend::Sdk("tree_sitter_rust"),
    },
    Grammar {
        name: "tree-sitter-scala",
        abi: 15,
        backend: Backend::C {
            directory: "scala",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_scala",
        },
    },
    Grammar {
        name: "tree-sitter-sql",
        abi: 14,
        backend: Backend::C {
            directory: "sql",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_sql",
        },
    },
    Grammar {
        name: "tree-sitter-svelte",
        abi: 14,
        backend: Backend::C {
            directory: "svelte",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_svelte",
        },
    },
    Grammar {
        name: "tree-sitter-swift",
        abi: 14,
        backend: Backend::C {
            directory: "swift",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_swift",
        },
    },
    Grammar {
        name: "tree-sitter-toml",
        abi: 14,
        backend: Backend::C {
            directory: "toml",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_toml",
        },
    },
    Grammar {
        name: "tree-sitter-tsx",
        abi: 14,
        backend: Backend::C {
            directory: "tsx",
            parser: "tsx/src/parser.c",
            scanner: Some("tsx/src/scanner.c"),
            include: "tsx/src",
            export: "tree_sitter_tsx",
        },
    },
    Grammar {
        name: "tree-sitter-typescript",
        abi: 14,
        backend: Backend::C {
            directory: "typescript",
            parser: "typescript/src/parser.c",
            scanner: Some("typescript/src/scanner.c"),
            include: "typescript/src",
            export: "tree_sitter_typescript",
        },
    },
    Grammar {
        name: "tree-sitter-vue",
        abi: 15,
        backend: Backend::C {
            directory: "vue",
            parser: "parser.c.inc",
            scanner: Some("scanner.c.inc"),
            include: "",
            export: "tree_sitter_vue",
        },
    },
    Grammar {
        name: "tree-sitter-xml",
        abi: 14,
        backend: Backend::C {
            directory: "xml",
            parser: "xml/src/parser.c",
            scanner: Some("xml/src/scanner.c"),
            include: "xml/src",
            export: "tree_sitter_xml",
        },
    },
    Grammar {
        name: "tree-sitter-yaml",
        abi: 14,
        backend: Backend::C {
            directory: "yaml",
            parser: "src/parser.c",
            scanner: Some("src/scanner.c"),
            include: "src",
            export: "tree_sitter_yaml",
        },
    },
    Grammar {
        name: "tree-sitter-zig",
        abi: 14,
        backend: Backend::C {
            directory: "zig",
            parser: "src/parser.c",
            scanner: None,
            include: "src",
            export: "tree_sitter_zig",
        },
    },
];

pub fn build() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let output = PathBuf::from(env::var("OUT_DIR")?);
    let sources = manifest.join("grammars/tree_sitter");
    println!("cargo:rerun-if-changed=tree_sitter_build.rs");
    println!("cargo:rerun-if-changed=grammars/tree_sitter");
    let mut declarations = String::from("unsafe extern \"C\" {\n");
    let mut dispatch =
        String::from("fn acquire(id: &str) -> Option<tree_sitter::Language> {\n    match id {\n");
    let mut names = String::from(
        "/// Literal effective grammar keys supported by this executable.\npub const BUILTIN_GRAMMAR_NAMES: &[&str] = &[\n",
    );
    let mut abis = String::from("#[cfg(test)]\nconst EXPECTED_ABIS: &[usize] = &[\n");
    for grammar in GRAMMARS {
        writeln!(names, "    {:?},", grammar.name)?;
        writeln!(abis, "    {},", grammar.abi)?;
        let alias = if grammar.name == "tree-sitter-c_sharp" {
            " | \"tree-sitter-c-sharp\""
        } else {
            ""
        };
        match grammar.backend {
            Backend::Sdk(module) => {
                writeln!(
                    dispatch,
                    "        {:?}{alias} => Some({module}::LANGUAGE.into()),",
                    grammar.name
                )?;
            }
            Backend::C {
                directory,
                parser,
                scanner,
                include,
                export,
            } => {
                let root = sources.join(directory);
                let mut compiler = cc::Build::new();
                compiler
                    .std("c11")
                    .debug(false)
                    .warnings(false)
                    .include(root.join(include));
                compiler.file(compilation_source(
                    &root, parser, directory, "parser", &output,
                )?);
                if let Some(scanner) = scanner {
                    compiler.file(compilation_source(
                        &root, scanner, directory, "scanner", &output,
                    )?);
                }
                compiler.try_compile(&format!("graphit_{export}"))?;
                writeln!(declarations, "    fn {export}() -> *const (); ")?;
                // SAFETY: exports are actual generated Tree-sitter language functions
                // from the closed, vendored catalog, never caller-controlled pointers.
                writeln!(
                    dispatch,
                    "        {:?}{alias} => Some(unsafe {{ tree_sitter::Language::new(tree_sitter_language::LanguageFn::from_raw({export})) }}),",
                    grammar.name
                )?;
            }
        }
    }
    declarations.push_str("}\n");
    dispatch.push_str("        _ => None,\n    }\n}\n");
    names.push_str("];\n");
    abis.push_str("];\n");
    fs::write(
        output.join("tree_sitter_builtin.rs"),
        format!("{declarations}{dispatch}{names}{abis}"),
    )?;
    Ok(())
}

fn compilation_source(
    root: &Path,
    source: &str,
    directory: &str,
    kind: &str,
    output: &Path,
) -> Result<PathBuf, std::io::Error> {
    if source.ends_with(".inc") {
        let wrapper = output.join(format!("{directory}_{kind}.c"));
        fs::write(&wrapper, format!("#include \"{source}\"\n"))?;
        Ok(wrapper)
    } else {
        Ok(root.join(source))
    }
}

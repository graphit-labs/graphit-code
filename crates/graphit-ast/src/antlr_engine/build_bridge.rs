//! Private build composition for generated recognizers and the small cxx bridge.

pub fn build_recognizers(
    runtime: &super::build_support::RuntimeInstall,
) -> Result<(), impl std::error::Error + use<>> {
    let mut build = cxx_build::bridge("src/antlr_engine/bridge.rs");
    build
        .std("c++17")
        .include("src")
        .include("src/antlr_engine/cpp/actions")
        .include("src/antlr_engine/cpp/generated")
        .include(&runtime.include_dir)
        .file("src/antlr_engine/cpp/bridge.cpp");
    build.file("src/antlr_engine/cpp/profile_queries.cpp");
    for source in [
        "actions/PlSqlParserBase.cpp",
        "actions/PostgreSQLLexerBase.cpp",
        "actions/PostgreSQLParserBase.cpp",
        "generated/plsql/PlSqlLexer.cpp",
        "generated/plsql/PlSqlParser.cpp",
        "generated/postgresql/PostgreSQLLexer.cpp",
        "generated/postgresql/PostgreSQLParser.cpp",
        "generated/db2/Db2Lexer.cpp",
        "generated/db2/Db2Parser.cpp",
        "generated/db2/Db2UnicodeLexer.cpp",
        "generated/db2/Db2UnicodeParser.cpp",
        "generated/tsql/TSqlLexer.cpp",
        "generated/tsql/TSqlParser.cpp",
        "generated/cobol85/Cobol85Lexer.cpp",
        "generated/cobol85/Cobol85Parser.cpp",
        "generated/cobol85/Cobol85PreprocessorLexer.cpp",
        "generated/cobol85/Cobol85PreprocessorParser.cpp",
    ] {
        let path = format!("src/antlr_engine/cpp/{source}");
        println!("cargo:rerun-if-changed={path}");
        build.file(path);
    }
    let target_os = std::env::var("CARGO_CFG_TARGET_OS");
    let windows = target_os.as_deref() == Ok("windows");
    if windows {
        build.define("ANTLR4CPP_STATIC", None);
    }
    if windows || target_os.as_deref() == Ok("macos") {
        let header = "src/antlr_engine/cpp/platform_math_tokens.hpp";
        println!("cargo:rerun-if-changed={header}");
        let compiler = build.get_compiler();
        if compiler.is_like_msvc() {
            build
                .include("src/antlr_engine/cpp")
                .flag("/FIplatform_math_tokens.hpp")
                .flag("/bigobj")
                .flag("/EHsc");
        } else {
            build.flag("-include").flag(header);
            if windows && compiler.is_like_gnu() {
                build.flag("-Wa,-mbig-obj");
            }
        }
    }
    println!("cargo:rerun-if-changed=src/antlr_engine/bridge.rs");
    println!("cargo:rerun-if-changed=src/antlr_engine/cpp/bridge.hpp");
    println!("cargo:rerun-if-changed=src/antlr_engine/cpp/bridge.cpp");
    println!("cargo:rerun-if-changed=src/antlr_engine/cpp/profile_queries.cpp");
    println!("cargo:rerun-if-changed=src/antlr_engine/cpp/profile_queries.hpp");
    println!("cargo:rerun-if-changed=src/antlr_engine/cpp/actions");
    println!("cargo:rerun-if-changed=src/antlr_engine/cpp/generated");
    build.try_compile("graphit-antlr-bridge")
}

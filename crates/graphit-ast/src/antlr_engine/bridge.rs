//! Operation-local, typed boundary to the official ANTLR C++ SDK.

use super::OperationControl;

fn check(control: &mut OperationControl<'_>) -> bool {
    control.check()
}

fn begin_query(control: &mut OperationControl<'_>, index: usize) -> bool {
    control.query_index = Some(index);
    control.check()
}

#[allow(unsafe_code)]
#[cxx::bridge(namespace = "graphit::antlr")]
pub(super) mod ffi {
    #[derive(Debug)]
    #[repr(u8)]
    enum NativeDialect {
        PlSql = 0,
        PostgreSql = 1,
        Db2 = 2,
        TSql = 3,
        Cobol85 = 4,
    }
    #[derive(Debug)]
    #[repr(u8)]
    enum NativeStatus {
        Success = 0,
        Stopped = 1,
    }
    #[derive(Debug)]
    #[repr(u8)]
    enum SpanKind {
        RegularHalfOpen = 0,
        Eof = 1,
        Unindexed = 2,
    }
    #[derive(Debug)]
    #[repr(u8)]
    enum UnindexedReason {
        NotApplicable = 0,
        InvalidSdkIndex = 1,
        InsertedRecoveryToken = 2,
        MissingTokenBounds = 3,
        MissingRuleBoundary = 4,
        ReversedRuleBoundary = 5,
        MissingDiagnosticPosition = 6,
    }
    #[derive(Debug)]
    #[repr(u8)]
    enum NodeKind {
        Rule = 0,
        Terminal = 1,
        Error = 2,
    }
    #[derive(Debug)]
    #[repr(u8)]
    enum DiagnosticPhase {
        Lexer = 0,
        Parser = 1,
    }
    struct NativeSpan {
        kind: SpanKind,
        start: u64,
        end: u64,
        reason: UnindexedReason,
    }
    struct NativeToken {
        kind_id: u64,
        channel: u64,
        span: NativeSpan,
        eof: bool,
        text: String,
    }
    struct NativeNode {
        kind: NodeKind,
        kind_id: u64,
        has_parent: bool,
        parent: u64,
        span: NativeSpan,
        missing: bool,
    }
    struct NativeDiagnostic {
        phase: DiagnosticPhase,
        grammar_id: u64,
        line: u64,
        column: u64,
        message: String,
        span: NativeSpan,
    }
    struct NativeBody {
        host_parent: u64,
        span: NativeSpan,
    }
    struct NativeQueryMatch {
        query_index: u64,
        node_index: u64,
        captures: Vec<NativeCapture>,
    }
    struct NativeCapture {
        has_node: bool,
        node_index: u64,
    }
    struct NativeCaptureStep {
        xpath: String,
        nearest_descendant: String,
        nearest_ancestor: String,
    }
    struct NativeCapturePath {
        steps: Vec<NativeCaptureStep>,
    }
    struct NativeProfileQuery {
        pattern: String,
        select: String,
        keyword: String,
        child_rule: String,
        negate: bool,
        captures: Vec<NativeCapturePath>,
    }
    struct NativeSyntax {
        status: NativeStatus,
        stream_scalar_count: u64,
        grammar_names: Vec<String>,
        rule_names: Vec<String>,
        token_kind_names: Vec<String>,
        tokens: Vec<NativeToken>,
        nodes: Vec<NativeNode>,
        diagnostics: Vec<NativeDiagnostic>,
        bodies: Vec<NativeBody>,
        query_matches: Vec<NativeQueryMatch>,
    }
    extern "Rust" {
        type OperationControl<'a>;
        fn check(control: &mut OperationControl<'_>) -> bool;
        fn begin_query(control: &mut OperationControl<'_>, index: usize) -> bool;
    }
    unsafe extern "C++" {
        include!("antlr_engine/cpp/bridge.hpp");
        type PreparedProfileQueries;
        fn prepare_profile_queries(
            dialect: NativeDialect,
            preprocessor: bool,
            queries: &[NativeProfileQuery],
            control: &mut OperationControl<'_>,
        ) -> Result<UniquePtr<PreparedProfileQueries>>;
        fn recognize_prepared_profile_queries(
            dialect: NativeDialect,
            preprocessor: bool,
            source: &str,
            queries: &PreparedProfileQueries,
            control: &mut OperationControl<'_>,
        ) -> Result<NativeSyntax>;
        fn recognize(
            dialect: NativeDialect,
            source: &str,
            control: &mut OperationControl<'_>,
        ) -> Result<NativeSyntax>;
        fn recognize_queries(
            dialect: NativeDialect,
            source: &str,
            paths: &[String],
            control: &mut OperationControl<'_>,
        ) -> Result<NativeSyntax>;
        fn lex(
            dialect: NativeDialect,
            source: &str,
            control: &mut OperationControl<'_>,
        ) -> Result<NativeSyntax>;
    }
}

//! Process-local terminal choices, installed once after successful argument parsing.
use graphit_app::cli_output::{Format, Output};
use std::{io, sync::OnceLock};

static CONTEXT: OnceLock<(Format, String)> = OnceLock::new();
pub fn configure(json: bool, command: String) {
    let _ = CONTEXT.set((if json { Format::Json } else { Format::Text }, command));
}
pub fn format() -> Format {
    CONTEXT.get().map(|v| v.0).unwrap_or_default()
}
pub fn stdout() -> Output<io::Stdout> {
    Output::new(
        io::stdout(),
        format(),
        CONTEXT.get().map(|v| v.1.as_str()).unwrap_or("Graphit"),
    )
}

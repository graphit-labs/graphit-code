use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_knowledge::preview::{
    MarkdownPreview, MarkdownPreviewError, MarkdownPreviewLimits, PreviewSource, preview_markdown,
};
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    ops::Range,
    time::{Duration, Instant},
};

fn limits(input: usize, output: usize, scalars: usize) -> MarkdownPreviewLimits {
    MarkdownPreviewLimits {
        max_input_bytes: NonZeroUsize::new(input).unwrap(),
        max_output_bytes: NonZeroUsize::new(output).unwrap(),
        max_body_scalars: NonZeroUsize::new(scalars).unwrap(),
    }
}

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

fn assert_preview(
    preview: &MarkdownPreview<'_>,
    input: &str,
    text: &str,
    source: PreviewSource,
    range: Range<usize>,
    truncated: bool,
) {
    assert_eq!(preview.text(), text);
    assert_eq!(preview.source(), source);
    assert_eq!(preview.source_range(), range);
    assert_eq!(preview.truncated(), truncated);
    assert!(input.is_char_boundary(range.start));
    assert!(input.is_char_boundary(range.end));
    assert_eq!(
        input.get(range),
        Some(if truncated {
            text.strip_suffix('…').unwrap()
        } else {
            text
        })
    );
    if !truncated {
        assert_eq!(preview.text().as_ptr(), input.as_ptr());
    }
}

fn assert_limit(error: MarkdownPreviewError, label: &'static str) {
    assert!(matches!(error, MarkdownPreviewError::LimitExceeded(found) if found == label));
    assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    assert!(error.source().is_none());
}

#[test]
fn summary_preserves_outer_whitespace_crlf_and_markdown() {
    let summary = "  **Resumo**\r\n\n- item  ";
    let preview = preview_markdown(&context(), "Other body", summary, limits(100, 100, 1)).unwrap();
    assert_preview(
        &preview,
        summary,
        summary,
        PreviewSource::Summary,
        0..summary.len(),
        false,
    );
}

#[test]
fn unicode_whitespace_summary_selects_raw_indented_body() {
    let body = "    code\n\n**Evidence**\n- first\n- second\n";
    let preview = preview_markdown(
        &context(),
        body,
        " \t\n\u{a0}\u{2003}",
        limits(100, 100, 100),
    )
    .unwrap();
    assert_preview(
        &preview,
        body,
        body,
        PreviewSource::Body,
        0..body.len(),
        false,
    );
}

#[test]
fn empty_and_whitespace_bodies_are_preserved() {
    for body in ["", " \n\t"] {
        let preview = preview_markdown(&context(), body, "", limits(3, 3, 3)).unwrap();
        assert_preview(
            &preview,
            body,
            body,
            PreviewSource::Body,
            0..body.len(),
            false,
        );
    }
    assert_eq!(
        preview_markdown(&context(), "", "", limits(1, 1, 1))
            .unwrap()
            .text(),
        ""
    );
}

#[test]
fn exact_scalar_cap_and_editorial_word_boundary_preserve_source() {
    let body = String::from("ação 🍀");
    let full = preview_markdown(&context(), &body, "", limits(11, 11, 6)).unwrap();
    assert_preview(&full, &body, &body, PreviewSource::Body, 0..11, false);
    let cut = preview_markdown(&context(), &body, "", limits(11, 9, 5)).unwrap();
    assert_preview(&cut, &body, "ação…", PreviewSource::Body, 0..6, true);
    assert_eq!(body, "ação 🍀");
}

#[test]
fn cjk_prefix_charges_marker_bytes_without_charging_source_scalars() {
    let body = "界".repeat(241);
    let preview = preview_markdown(&context(), &body, "", limits(723, 723, 240)).unwrap();
    let expected = format!("{}…", "界".repeat(240));
    assert_preview(
        &preview,
        &body,
        &expected,
        PreviewSource::Body,
        0..720,
        true,
    );
    assert_eq!(preview.text().len(), 723);
    assert_eq!(preview.text().chars().count(), 241);
    assert_limit(
        preview_markdown(&context(), &body, "", limits(723, 722, 240)).unwrap_err(),
        "output bytes",
    );
}

#[test]
fn unicode_delimiters_scalar_clusters_last_boundary_and_lookahead_are_explicit() {
    for (body, cap, end, output) in [
        ("abé\u{2003}cdef", 6, 4, "abé…"),
        ("e\u{301}XYZ", 2, 3, "e\u{301}…"),
        ("👨‍👩‍👧‍👦X", 2, 7, "👨‍…"),
        ("a b c dXYZ", 7, 5, "a b c…"),
        ("abcdef ghi", 6, 6, "abcdef…"),
    ] {
        let preview = preview_markdown(&context(), body, "", limits(100, 100, cap)).unwrap();
        assert_preview(&preview, body, output, PreviewSource::Body, 0..end, true);
    }
}

#[test]
fn markdown_clipping_does_not_balance_escape_or_render_syntax() {
    for (body, cap, end, output) in [
        ("```rs\nx()\n```\n- [a](b)\n<b>x</b>", 4, 4, "```r…"),
        ("- a\n- b", 2, 1, "-…"),
        ("`xyz`", 3, 3, "`xy…"),
        ("[a](b)", 4, 4, "[a](…"),
        ("<b>x</b>", 4, 4, "<b>x…"),
    ] {
        let original = String::from(body);
        let preview = preview_markdown(&context(), &original, "", limits(100, 100, cap)).unwrap();
        assert_preview(
            &preview,
            &original,
            output,
            PreviewSource::Body,
            0..end,
            true,
        );
        assert_eq!(original, body);
    }
}

#[test]
fn raw_input_quota_includes_unselected_body_and_max_caps_use_actual_text() {
    let body = "界".repeat(100);
    let summary = "sum";
    let preview = preview_markdown(&context(), &body, summary, limits(303, 3, 1)).unwrap();
    assert_preview(
        &preview,
        summary,
        summary,
        PreviewSource::Summary,
        0..3,
        false,
    );
    assert_limit(
        preview_markdown(&context(), &body, summary, limits(302, 3, 1)).unwrap_err(),
        "input bytes",
    );
    for (body, summary, selected, source) in [
        ("é", "", "é", PreviewSource::Body),
        ("ignored", " summary ", " summary ", PreviewSource::Summary),
    ] {
        let preview = preview_markdown(
            &context(),
            body,
            summary,
            limits(usize::MAX, usize::MAX, usize::MAX),
        )
        .unwrap();
        let input = match source {
            PreviewSource::Body => body,
            PreviewSource::Summary => summary,
        };
        assert_preview(&preview, input, selected, source, 0..selected.len(), false);
    }
}

#[test]
fn hard_output_quota_rejects_integral_selection_without_fallback() {
    for (body, summary, output, source) in [
        ("x", " é ", " é ", PreviewSource::Summary),
        ("é\n", "", "é\n", PreviewSource::Body),
    ] {
        let preview =
            preview_markdown(&context(), body, summary, limits(100, output.len(), 100)).unwrap();
        let input = match source {
            PreviewSource::Body => body,
            PreviewSource::Summary => summary,
        };
        assert_preview(&preview, input, output, source, 0..output.len(), false);
        assert_limit(
            preview_markdown(
                &context(),
                body,
                summary,
                limits(100, output.len() - 1, 100),
            )
            .unwrap_err(),
            "output bytes",
        );
    }
}

#[test]
fn cancellation_and_expired_deadline_precede_quota_with_original_causes() {
    let past = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    for (cancelled, deadline, expected) in [
        (
            true,
            Instant::now() + Duration::from_secs(30),
            io::ErrorKind::Interrupted,
        ),
        (false, past, io::ErrorKind::TimedOut),
        (true, past, io::ErrorKind::Interrupted),
    ] {
        let cancellation = Cancellation::default();
        if cancelled {
            cancellation.cancel();
        }
        let context = SnapshotContext::new(deadline, cancellation);
        let error = preview_markdown(&context, "too large", "", limits(1, 1, 1)).unwrap_err();
        assert_eq!(error.kind(), expected);
        let cause = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
        assert_eq!(cause.kind(), expected);
    }
}

#[test]
fn original_request_preserves_inputs_and_cancellation() {
    let original = context();
    let body = String::from("abcdefghi");
    let preview = preview_markdown(&original, &body, "", limits(9, 7, 4)).unwrap();
    assert_preview(&preview, &body, "abcd…", PreviewSource::Body, 0..4, true);
    original.cancellation().cancel();
    let error = preview_markdown(&original, &body, "", limits(9, 7, 4)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(preview.text(), "abcd…");
    assert_eq!(body, "abcdefghi");
}

#[test]
fn errors_hide_text_and_borrowed_preview_does_not_borrow_context() {
    let body = String::from("private body text");
    let summary = String::from("private summary text");
    let preview = {
        let request = context();
        preview_markdown(&request, &body, &summary, limits(100, 100, 1)).unwrap()
    };
    assert_preview(
        &preview,
        &summary,
        &summary,
        PreviewSource::Summary,
        0..summary.len(),
        false,
    );
    let request = context();
    let quota = preview_markdown(&request, &body, &summary, limits(1, 1, 1)).unwrap_err();
    request.cancellation().cancel();
    let cancelled = preview_markdown(&request, &body, &summary, limits(100, 100, 1)).unwrap_err();
    for error in [quota, cancelled] {
        let display = error.to_string();
        assert!(!display.contains(&body));
        assert!(!display.contains(&summary));
    }
}

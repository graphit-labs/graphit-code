//! Resolve the existing focused Rust test selectors.
use std::path::Path;
pub(super) fn run_selected(root: &Path, arguments: &[String]) -> Result<(), String> {
    let selected = select_fast_tests(arguments)?;
    for test in selected {
        super::execute_test(root, test)?;
    }
    Ok(())
}

fn select_fast_tests(arguments: &[String]) -> Result<Vec<super::FastTest>, String> {
    if arguments.is_empty() {
        return Err("test requires at least one registered selector".into());
    }
    for (index, label) in arguments.iter().enumerate() {
        if !super::REGISTERED_TESTS
            .iter()
            .any(|test| test.label() == *label)
        {
            return Err(format!("unknown fast guard {label:?}"));
        }
        if arguments[..index].contains(label) {
            return Err(format!("duplicate fast guard {label:?}"));
        }
    }
    Ok(super::REGISTERED_TESTS
        .iter()
        .filter(|test| arguments.iter().any(|label| *label == test.label()))
        .copied()
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_selection_is_rejected_before_fixture_or_workspace_access() {
        let root = Path::new("guard-selection-root-that-does-not-exist");
        assert!(
            run_selected(root, &["unknown/target".into()])
                .unwrap_err()
                .starts_with("unknown fast guard")
        );
        assert!(
            run_selected(root, &vec!["graphit-core/source_reader".into(); 2])
                .unwrap_err()
                .starts_with("duplicate fast guard")
        );
        assert!(
            run_selected(root, &[])
                .unwrap_err()
                .contains("at least one")
        );
        assert!(!root.exists());
    }
    #[test]
    fn selections_follow_catalog_order() {
        let selected = select_fast_tests(&[
            "graphit-knowledge/staleness".into(),
            "graphit-core/source_reader".into(),
            "graphit-ai/direct_embeddings".into(),
            "graphit-app/knowledge_antigravity_ask".into(),
            "graphit-app/knowledge_grok_ask".into(),
            "graphit-ai/cli_grok".into(),
            "graphit-ai/cli_antigravity".into(),
            "graphit-ai/cli_kimi".into(),
            "graphit-ai/cli_kimi_unit".into(),
            "graphit-ai/cli_copilot".into(),
            "graphit-app/knowledge_copilot_ask".into(),
        ])
        .unwrap();
        assert_eq!(
            selected
                .iter()
                .map(super::super::FastTest::label)
                .collect::<Vec<_>>(),
            [
                "graphit-ai/cli_antigravity",
                "graphit-app/knowledge_antigravity_ask",
                "graphit-ai/cli_grok",
                "graphit-app/knowledge_grok_ask",
                "graphit-ai/cli_kimi_unit",
                "graphit-ai/cli_copilot",
                "graphit-app/knowledge_copilot_ask",
                "graphit-ai/cli_kimi",
                "graphit-core/source_reader",
                "graphit-ai/direct_embeddings",
                "graphit-knowledge/staleness"
            ]
        );
        assert!(select_fast_tests(&[]).is_err());
    }
    #[test]
    fn integration_arguments_do_not_repeat_library_tests() {
        let labels: std::collections::HashSet<_> = super::super::REGISTERED_TESTS
            .iter()
            .map(super::super::FastTest::label)
            .collect();
        assert_eq!(labels.len(), super::super::REGISTERED_TESTS.len());
        assert_eq!(
            select_fast_tests(&[
                "graphit-daemon/watcher".into(),
                "graphit-daemon/supervisor".into()
            ])
            .unwrap()
            .len(),
            2
        );
        for test in super::super::REGISTERED_TESTS {
            if let super::super::FastTest::Integration { package, target } = test {
                assert_eq!(
                    test.arguments(),
                    ["test", "--locked", "-p", package, "--test", target]
                );
            }
        }
        let kimi_unit = select_fast_tests(&["graphit-ai/cli_kimi_unit".into()]).unwrap();
        assert_eq!(kimi_unit.len(), 1);
        assert_eq!(
            kimi_unit[0].arguments(),
            [
                "test",
                "--locked",
                "-p",
                "graphit-ai",
                "--lib",
                "cli_kimi::tests::"
            ]
        );
        let kimi_integration = select_fast_tests(&["graphit-ai/cli_kimi".into()]).unwrap();
        assert_eq!(kimi_integration.len(), 1);
        assert_eq!(
            kimi_integration[0].arguments(),
            ["test", "--locked", "-p", "graphit-ai", "--test", "cli_kimi"]
        );
        let selected = select_fast_tests(&["graphit-auth/records".into()]).unwrap();
        let filter = selected[0].arguments()[5];
        assert!("issuer::tests::records::valid_case".contains(filter));
        assert!(!"issuer::tests::records_deadline::".contains(filter));
    }
}

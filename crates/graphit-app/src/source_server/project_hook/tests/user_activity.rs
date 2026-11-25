use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_activity_is_one_accepted_instant_across_real_projects_without_user_authority() {
    let mut fixture = configured_fixture(
        super::super::super::test_fixture::fixture_with_projects(
            &[],
            Brand::default(),
            NativeBinding::KiroV3,
            &["project", "second"],
        ),
        json!({"memory":false,"task":false}),
    );
    let (_, project) = attach_activity(&mut fixture);
    let project_before = *project.borrow();
    let (sender, user) = tokio::sync::watch::channel(None);
    fixture.server.attach_user_activity(sender).unwrap();
    let (connection, serving) = client(&fixture).await;
    assert!(user.borrow().is_none());
    let session = connection
        .call_tool(request(json!({"engine":"kiro","event":"SessionStart"})))
        .await
        .unwrap();
    assert_eq!(session.is_error, Some(false));
    assert!(user.borrow().is_none());
    let second = fixture.projects[1].0.root();
    let accepted = connection
        .call_tool(request(json!({
            "engine":"kiro","event":"UserPromptSubmit",
            "project_dir":graphit_core::native_path::NativePath::new(second.to_owned()).unwrap(),
        })))
        .await
        .unwrap();
    assert_eq!(accepted.is_error, Some(false), "{accepted:?}");
    assert_eq!(
        *project.borrow(),
        project_before,
        "PROJECT lane retains its exact tuple"
    );
    let second_observation = user.borrow().unwrap();
    let accepted = connection
        .call_tool(request(json!({"engine":"kiro","event":"UserPromptSubmit"})))
        .await
        .unwrap();
    assert_eq!(accepted.is_error, Some(false), "{accepted:?}");
    assert_eq!(
        Some(*project.borrow()),
        *user.borrow(),
        "one accepted Instant is shared"
    );
    assert!(user.borrow().unwrap() >= second_observation);
    connection.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabled_hooks_never_emit_a_user_activity_observation() {
    let mut fixture = configured(&[], json!({"hooks":false,"memory":false,"task":false}));
    let (sender, user) = tokio::sync::watch::channel(None);
    fixture.server.attach_user_activity(sender).unwrap();
    let (connection, serving) = client(&fixture).await;
    let response = connection
        .call_tool(request(json!({"engine":"kiro","event":"UserPromptSubmit"})))
        .await
        .unwrap();
    assert_eq!(response.is_error, Some(false));
    assert!(user.borrow().is_none());
    connection.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

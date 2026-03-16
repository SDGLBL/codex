use anyhow::Result;
use codex_core::ThreadConfigSnapshot;
use codex_core::config::AgentRoleConfig;
use codex_core::features::Feature;
use codex_protocol::ThreadId;
use codex_protocol::openai_models::ReasoningEffort;
use core_test_support::responses::ResponsesRequest;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_response_once_match;
use core_test_support::responses::mount_sse_once_match;
use core_test_support::responses::sse;
use core_test_support::responses::sse_response;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::time::Duration;
use tokio::time::Instant;
use tokio::time::sleep;
use wiremock::MockServer;

const SPAWN_CALL_ID: &str = "spawn-call-1";
const TURN_0_FORK_PROMPT: &str = "seed fork context";
const TURN_1_PROMPT: &str = "spawn a child and continue";
const TURN_2_NO_WAIT_PROMPT: &str = "follow up without wait";
const CHILD_PROMPT: &str = "child: do work";
const INHERITED_MODEL: &str = "gpt-5.2-codex";
const INHERITED_REASONING_EFFORT: ReasoningEffort = ReasoningEffort::XHigh;
const REQUESTED_MODEL: &str = "gpt-5.1";
const REQUESTED_REASONING_EFFORT: ReasoningEffort = ReasoningEffort::Low;
const ROLE_MODEL: &str = "gpt-5.1-codex-max";
const ROLE_REASONING_EFFORT: ReasoningEffort = ReasoningEffort::High;
const RESUMED_CHILD_PROMPT: &str = "child: resumed follow up";

fn body_contains(req: &wiremock::Request, text: &str) -> bool {
    let is_zstd = req
        .headers
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|entry| entry.trim().eq_ignore_ascii_case("zstd"))
        });
    let bytes = if is_zstd {
        zstd::stream::decode_all(std::io::Cursor::new(&req.body)).ok()
    } else {
        Some(req.body.clone())
    };
    bytes
        .and_then(|body| String::from_utf8(body).ok())
        .is_some_and(|body| body.contains(text))
}

fn has_subagent_notification(req: &ResponsesRequest) -> bool {
    req.message_input_texts("user")
        .iter()
        .any(|text| text.contains("<subagent_notification>"))
}

async fn wait_for_spawned_thread_id(test: &TestCodex) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let ids = test.thread_manager.list_thread_ids().await;
        if let Some(spawned_id) = ids
            .iter()
            .find(|id| **id != test.session_configured.session_id)
        {
            return Ok(spawned_id.to_string());
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for spawned thread id");
        }
        sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_for_requests(
    mock: &core_test_support::responses::ResponseMock,
) -> Result<Vec<ResponsesRequest>> {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let requests = mock.requests();
        if !requests.is_empty() {
            return Ok(requests);
        }
        if Instant::now() >= deadline {
            anyhow::bail!("expected at least 1 request, got {}", requests.len());
        }
        sleep(Duration::from_millis(10)).await;
    }
}

async fn setup_turn_one_with_spawned_child(
    server: &MockServer,
    child_response_delay: Option<Duration>,
) -> Result<(TestCodex, String)> {
    let spawn_args = serde_json::to_string(&json!({
        "message": CHILD_PROMPT,
    }))?;

    mount_sse_once_match(
        server,
        |req: &wiremock::Request| body_contains(req, TURN_1_PROMPT),
        sse(vec![
            ev_response_created("resp-turn1-1"),
            ev_function_call(SPAWN_CALL_ID, "spawn_agent", &spawn_args),
            ev_completed("resp-turn1-1"),
        ]),
    )
    .await;

    let child_sse = sse(vec![
        ev_response_created("resp-child-1"),
        ev_assistant_message("msg-child-1", "child done"),
        ev_completed("resp-child-1"),
    ]);
    let child_request_log = if let Some(delay) = child_response_delay {
        mount_response_once_match(
            server,
            |req: &wiremock::Request| {
                body_contains(req, CHILD_PROMPT) && !body_contains(req, SPAWN_CALL_ID)
            },
            sse_response(child_sse).set_delay(delay),
        )
        .await
    } else {
        mount_sse_once_match(
            server,
            |req: &wiremock::Request| {
                body_contains(req, CHILD_PROMPT) && !body_contains(req, SPAWN_CALL_ID)
            },
            child_sse,
        )
        .await
    };

    let _turn1_followup = mount_sse_once_match(
        server,
        |req: &wiremock::Request| body_contains(req, SPAWN_CALL_ID),
        sse(vec![
            ev_response_created("resp-turn1-2"),
            ev_assistant_message("msg-turn1-2", "parent done"),
            ev_completed("resp-turn1-2"),
        ]),
    )
    .await;

    #[allow(clippy::expect_used)]
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Collab)
            .expect("test config should allow feature update");
    });
    let test = builder.build(server).await?;
    test.submit_turn(TURN_1_PROMPT).await?;
    if child_response_delay.is_none() {
        let _ = wait_for_requests(&child_request_log).await?;
        let rollout_path = test
            .codex
            .rollout_path()
            .ok_or_else(|| anyhow::anyhow!("expected parent rollout path"))?;
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            let has_notification = tokio::fs::read_to_string(&rollout_path)
                .await
                .is_ok_and(|rollout| rollout.contains("<subagent_notification>"));
            if has_notification {
                break;
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "timed out waiting for parent rollout to include subagent notification"
                );
            }
            sleep(Duration::from_millis(10)).await;
        }
    }
    let spawned_id = wait_for_spawned_thread_id(&test).await?;

    Ok((test, spawned_id))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subagent_notification_is_included_without_wait() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let (test, _spawned_id) = setup_turn_one_with_spawned_child(&server, None).await?;

    let turn2 = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, TURN_2_NO_WAIT_PROMPT),
        sse(vec![
            ev_response_created("resp-turn2-1"),
            ev_assistant_message("msg-turn2-1", "no wait path"),
            ev_completed("resp-turn2-1"),
        ]),
    )
    .await;
    test.submit_turn(TURN_2_NO_WAIT_PROMPT).await?;

    let turn2_requests = wait_for_requests(&turn2).await?;
    assert!(turn2_requests.iter().any(has_subagent_notification));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawned_child_without_fork_uses_child_thread_id_for_session_header() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let (test, spawned_id) = setup_turn_one_with_spawned_child(&server, None).await?;
    let requests = server.received_requests().await.unwrap_or_default();
    let child_request = requests
        .into_iter()
        .find(|request| {
            body_contains(request, CHILD_PROMPT) && !body_contains(request, SPAWN_CALL_ID)
        })
        .ok_or_else(|| anyhow::anyhow!("expected non-fork child request"))?;

    assert_eq!(
        child_request
            .headers
            .get("session_id")
            .and_then(|value| value.to_str().ok()),
        Some(spawned_id.as_str())
    );
    assert_ne!(spawned_id, test.session_configured.session_id.to_string());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawned_child_receives_forked_parent_context() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;

    let seed_turn = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, TURN_0_FORK_PROMPT),
        sse(vec![
            ev_response_created("resp-seed-1"),
            ev_assistant_message("msg-seed-1", "seeded"),
            ev_completed("resp-seed-1"),
        ]),
    )
    .await;

    let spawn_args = serde_json::to_string(&json!({
        "message": CHILD_PROMPT,
        "fork_context": true,
    }))?;
    let spawn_turn = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, TURN_1_PROMPT),
        sse(vec![
            ev_response_created("resp-turn1-1"),
            ev_function_call(SPAWN_CALL_ID, "spawn_agent", &spawn_args),
            ev_completed("resp-turn1-1"),
        ]),
    )
    .await;

    let child_request_log = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, CHILD_PROMPT),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_assistant_message("msg-child-1", "child done"),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;

    let _turn1_followup = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, SPAWN_CALL_ID),
        sse(vec![
            ev_response_created("resp-turn1-2"),
            ev_assistant_message("msg-turn1-2", "parent done"),
            ev_completed("resp-turn1-2"),
        ]),
    )
    .await;

    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Collab)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    test.submit_turn(TURN_0_FORK_PROMPT).await?;
    let _ = seed_turn.single_request();

    test.submit_turn(TURN_1_PROMPT).await?;
    let _ = spawn_turn.single_request();

    let parent_session_id = test.session_configured.session_id.to_string();
    let child_request = wait_for_requests(&child_request_log)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("expected forked child request"))?;
    assert!(child_request.body_contains_text(TURN_0_FORK_PROMPT));
    assert!(child_request.body_contains_text("seeded"));
    assert_eq!(
        child_request.header("session_id").as_deref(),
        Some(parent_session_id.as_str())
    );

    assert!(child_request.body_contains_text(SPAWN_CALL_ID));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resumed_forked_child_preserves_persisted_parent_wire_session_id() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;

    let seed_turn = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, TURN_0_FORK_PROMPT),
        sse(vec![
            ev_response_created("resp-seed-1"),
            ev_assistant_message("msg-seed-1", "seeded"),
            ev_completed("resp-seed-1"),
        ]),
    )
    .await;

    let spawn_args = serde_json::to_string(&json!({
        "message": CHILD_PROMPT,
        "fork_context": true,
    }))?;
    let spawn_turn = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, TURN_1_PROMPT),
        sse(vec![
            ev_response_created("resp-turn1-1"),
            ev_function_call(SPAWN_CALL_ID, "spawn_agent", &spawn_args),
            ev_completed("resp-turn1-1"),
        ]),
    )
    .await;

    let child_request_log = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, CHILD_PROMPT),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_assistant_message("msg-child-1", "child done"),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;

    let _turn1_followup = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, SPAWN_CALL_ID),
        sse(vec![
            ev_response_created("resp-turn1-2"),
            ev_assistant_message("msg-turn1-2", "parent done"),
            ev_completed("resp-turn1-2"),
        ]),
    )
    .await;

    #[allow(clippy::expect_used)]
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Collab)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    test.submit_turn(TURN_0_FORK_PROMPT).await?;
    let _ = seed_turn.single_request();

    test.submit_turn(TURN_1_PROMPT).await?;
    let _ = spawn_turn.single_request();

    let parent_session_id = test.session_configured.session_id.to_string();
    let spawned_id = wait_for_spawned_thread_id(&test).await?;
    let child_request = wait_for_requests(&child_request_log)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("expected forked child request"))?;
    assert_eq!(
        child_request.header("session_id").as_deref(),
        Some(parent_session_id.as_str())
    );

    let child_rollout_path = test
        .thread_manager
        .get_thread(codex_protocol::ThreadId::from_string(&spawned_id)?)
        .await?
        .rollout_path()
        .ok_or_else(|| anyhow::anyhow!("expected child rollout path"))?;

    let resumed_child_turn = mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, RESUMED_CHILD_PROMPT),
        sse(vec![
            ev_response_created("resp-child-resumed-1"),
            ev_assistant_message("msg-child-resumed-1", "resumed child done"),
            ev_completed("resp-child-resumed-1"),
        ]),
    )
    .await;

    let mut resume_builder = test_codex()
        .with_home(test.home.clone())
        .with_config(|config| {
            config
                .features
                .enable(Feature::Collab)
                .expect("test config should allow feature update");
        });
    let resumed = resume_builder
        .resume(&server, test.home.clone(), child_rollout_path)
        .await?;
    assert_eq!(
        resumed.session_configured.session_id.to_string(),
        spawned_id
    );

    resumed.submit_turn(RESUMED_CHILD_PROMPT).await?;

    let resumed_request = resumed_child_turn.single_request();
    assert_eq!(
        resumed_request.header("session_id").as_deref(),
        Some(parent_session_id.as_str())
    );

    Ok(())
}

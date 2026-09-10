use codex_core::TurnInput;
use codex_core::TurnInputRequest;
use codex_history::RolloutItem;
use codex_protocol::protocol::EventMsg;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::responses::strip_metadata_from_json;
use core_test_support::test_codex::TestCodexBuilder;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use test_case::test_case;

const OUTPUT: &str = "Check the scheduled task.\nPreserve <tags>, $variables, and 中文.";

fn compatible_provider() -> TestCodexBuilder {
    test_codex().with_config(|config| {
        config.model_provider_id = "compatible".to_string();
        config.model_provider.name = "Compatible Responses".to_string();
        config.model_provider.requires_openai_auth = false;
    })
}

fn standalone_output(name: &str) -> Value {
    json!({
        "id": "fco_app_event",
        "type": "function_call_output",
        "name": name,
        "namespace": "codex_app",
        "output": OUTPUT,
    })
}

fn compatible_message(name: &str) -> Value {
    json!({
        "id": "msg_fco_app_event",
        "type": "message",
        "role": "user",
        "content": [{
            "type": "input_text",
            "text": format!(
                "<codex_app_tool_output>\nSource: codex_app.{name}\n\n{OUTPUT}\n</codex_app_tool_output>"
            ),
        }],
    })
}

#[test_case("automation_update"; "scheduled task")]
#[test_case("create_thread"; "created task")]
#[test_case("send_message_to_thread"; "task message")]
#[test_case("fork_thread"; "forked task")]
#[test_case("handoff_thread"; "task handoff")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standalone_app_output_reaches_compatible_provider(name: &str) -> anyhow::Result<()> {
    let server = start_mock_server().await;
    let response = mount_sse_once(
        &server,
        sse(vec![ev_response_created("turn"), ev_completed("turn")]),
    )
    .await;
    let test = compatible_provider().build_with_auto_env(&server).await?;

    test.codex
        .start_or_steer_turn(TurnInputRequest::new(TurnInput::ResponseItem(
            serde_json::from_value(standalone_output(name))?,
        )))
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let request = response.single_request();
    let messages = request
        .input()
        .into_iter()
        .filter(|item| item["id"] == "msg_fco_app_event")
        .collect::<Vec<_>>();
    assert_eq!(messages, vec![compatible_message(name)]);
    assert!(request.inputs_of_type("function_call_output").is_empty());

    test.codex.flush_rollout().await?;
    let history = test.codex.load_history(/*include_archived*/ false).await?;
    let saved_events = history
        .items
        .iter()
        .filter_map(|item| match item {
            RolloutItem::ResponseItem(envelope)
                if envelope
                    .item
                    .id()
                    .is_some_and(|id| id.as_str() == "fco_app_event") =>
            {
                Some(strip_metadata_from_json(json!(envelope.item)))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(saved_events, vec![standalone_output(name)]);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standalone_app_output_is_normalized_again_after_resume() -> anyhow::Result<()> {
    let server = start_mock_server().await;
    let response = mount_sse_once(&server, sse(vec![ev_completed("before-resume")])).await;
    let initial = compatible_provider().build_with_auto_env(&server).await?;
    initial
        .codex
        .start_or_steer_turn(TurnInputRequest::new(TurnInput::ResponseItem(
            serde_json::from_value(standalone_output("automation_update"))?,
        )))
        .await?;
    wait_for_event(&initial.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    let initial_request = response.single_request();

    let resumed = compatible_provider().restart(&server, &initial).await?;
    let response = mount_sse_once(&server, sse(vec![ev_completed("after-resume")])).await;
    resumed.submit_text_turn("Continue after restart.").await?;
    let resumed_request = response.single_request();

    for request in [initial_request, resumed_request] {
        let events = request
            .input()
            .into_iter()
            .filter(|item| item["id"] == "msg_fco_app_event")
            .collect::<Vec<_>>();
        assert_eq!(events, vec![compatible_message("automation_update")]);
        assert!(request.inputs_of_type("function_call_output").is_empty());
    }

    Ok(())
}

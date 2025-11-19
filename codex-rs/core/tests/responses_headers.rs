use std::sync::Arc;

use codex_app_server_protocol::AuthMode;
use codex_core::AuthManager;
use codex_core::CodexAuth;
use codex_core::ContentItem;
use codex_core::ConversationManager;
use codex_core::ModelClient;
use codex_core::ModelProviderInfo;
use codex_core::NewConversation;
use codex_core::Prompt;
use codex_core::ResponseEvent;
use codex_core::ResponseItem;
use codex_core::WireApi;
use codex_core::protocol::EventMsg;
use codex_core::protocol::Op;
use codex_otel::otel_event_manager::OtelEventManager;
use codex_protocol::ConversationId;
use codex_protocol::protocol::SessionSource;
use codex_protocol::user_input::UserInput;
use core_test_support::load_default_config_for_test;
use core_test_support::responses;
use core_test_support::wait_for_event;
use futures::StreamExt;
use serde_json::Value;
use tempfile::TempDir;
use wiremock::matchers::header;

#[tokio::test]
async fn responses_stream_includes_subagent_header_on_review() {
    core_test_support::skip_if_no_network!();

    let server = responses::start_mock_server().await;
    let response_body = responses::sse(vec![
        responses::ev_response_created("resp-1"),
        responses::ev_completed("resp-1"),
    ]);

    let request_recorder = responses::mount_sse_once_match(
        &server,
        header("x-openai-subagent", "review"),
        response_body,
    )
    .await;

    let provider = ModelProviderInfo {
        name: "mock".into(),
        base_url: Some(format!("{}/v1", server.uri())),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
        requires_openai_auth: false,
    };

    let codex_home = TempDir::new().expect("failed to create TempDir");
    let mut config = load_default_config_for_test(&codex_home);
    config.model_provider_id = provider.name.clone();
    config.model_provider = provider.clone();
    let effort = config.model_reasoning_effort;
    let summary = config.model_reasoning_summary;
    let config = Arc::new(config);

    let conversation_id = ConversationId::new();

    let otel_event_manager = OtelEventManager::new(
        conversation_id,
        config.model.as_str(),
        config.model_family.slug.as_str(),
        None,
        Some("test@test.com".to_string()),
        Some(AuthMode::ChatGPT),
        false,
        "test".to_string(),
    );

    let client = ModelClient::new(
        Arc::clone(&config),
        None,
        otel_event_manager,
        provider,
        effort,
        summary,
        conversation_id,
        conversation_id,
        SessionSource::SubAgent(codex_protocol::protocol::SubAgentSource::Review),
    );

    let mut prompt = Prompt::default();
    prompt.input = vec![ResponseItem::Message {
        id: None,
        role: "user".into(),
        content: vec![ContentItem::InputText {
            text: "hello".into(),
        }],
    }];

    let mut stream = client.stream(&prompt).await.expect("stream failed");
    while let Some(event) = stream.next().await {
        if matches!(event, Ok(ResponseEvent::Completed { .. })) {
            break;
        }
    }

    let request = request_recorder.single_request();
    assert_eq!(
        request.header("x-openai-subagent").as_deref(),
        Some("review")
    );
}

#[tokio::test]
async fn responses_stream_includes_subagent_header_on_other() {
    core_test_support::skip_if_no_network!();

    let server = responses::start_mock_server().await;
    let response_body = responses::sse(vec![
        responses::ev_response_created("resp-1"),
        responses::ev_completed("resp-1"),
    ]);

    let request_recorder = responses::mount_sse_once_match(
        &server,
        header("x-openai-subagent", "my-task"),
        response_body,
    )
    .await;

    let provider = ModelProviderInfo {
        name: "mock".into(),
        base_url: Some(format!("{}/v1", server.uri())),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
        requires_openai_auth: false,
    };

    let codex_home = TempDir::new().expect("failed to create TempDir");
    let mut config = load_default_config_for_test(&codex_home);
    config.model_provider_id = provider.name.clone();
    config.model_provider = provider.clone();
    let effort = config.model_reasoning_effort;
    let summary = config.model_reasoning_summary;
    let config = Arc::new(config);

    let conversation_id = ConversationId::new();

    let otel_event_manager = OtelEventManager::new(
        conversation_id,
        config.model.as_str(),
        config.model_family.slug.as_str(),
        None,
        Some("test@test.com".to_string()),
        Some(AuthMode::ChatGPT),
        false,
        "test".to_string(),
    );

    let client = ModelClient::new(
        Arc::clone(&config),
        None,
        otel_event_manager,
        provider,
        effort,
        summary,
        conversation_id,
        conversation_id,
        SessionSource::SubAgent(codex_protocol::protocol::SubAgentSource::Other(
            "my-task".to_string(),
        )),
    );

    let mut prompt = Prompt::default();
    prompt.input = vec![ResponseItem::Message {
        id: None,
        role: "user".into(),
        content: vec![ContentItem::InputText {
            text: "hello".into(),
        }],
    }];

    let mut stream = client.stream(&prompt).await.expect("stream failed");
    while let Some(event) = stream.next().await {
        if matches!(event, Ok(ResponseEvent::Completed { .. })) {
            break;
        }
    }

    let request = request_recorder.single_request();
    assert_eq!(
        request.header("x-openai-subagent").as_deref(),
        Some("my-task")
    );
}

#[tokio::test]
async fn responses_stream_reuses_wire_session_id_for_headers() {
    core_test_support::skip_if_no_network!();

    let server = responses::start_mock_server().await;
    let response_body = responses::sse(vec![
        responses::ev_response_created("resp-1"),
        responses::ev_completed("resp-1"),
    ]);

    let request_recorder = responses::mount_sse_once(&server, response_body).await;

    let provider = ModelProviderInfo {
        name: "mock".into(),
        base_url: Some(format!("{}/v1", server.uri())),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
        requires_openai_auth: false,
    };

    let codex_home = TempDir::new().expect("failed to create TempDir");
    let mut config = load_default_config_for_test(&codex_home);
    config.model_provider_id = provider.name.clone();
    config.model_provider = provider.clone();
    let effort = config.model_reasoning_effort;
    let summary = config.model_reasoning_summary;
    let config = Arc::new(config);

    let conversation_id = ConversationId::new();
    let wire_session_id = ConversationId::new();
    let conversation_id_str = conversation_id.to_string();
    let wire_session_id_str = wire_session_id.to_string();

    let otel_event_manager = OtelEventManager::new(
        conversation_id,
        config.model.as_str(),
        config.model_family.slug.as_str(),
        None,
        Some("test@test.com".to_string()),
        Some(AuthMode::ChatGPT),
        false,
        "test".to_string(),
    );

    let client = ModelClient::new(
        Arc::clone(&config),
        None,
        otel_event_manager,
        provider,
        effort,
        summary,
        conversation_id,
        wire_session_id,
        SessionSource::Cli,
    );

    let mut prompt = Prompt::default();
    prompt.input = vec![ResponseItem::Message {
        id: None,
        role: "user".into(),
        content: vec![ContentItem::InputText {
            text: "hello".into(),
        }],
    }];

    let mut stream = client.stream(&prompt).await.expect("stream failed");
    while let Some(event) = stream.next().await {
        if matches!(event, Ok(ResponseEvent::Completed { .. })) {
            break;
        }
    }

    let request = request_recorder.single_request();
    assert_eq!(
        request.header("conversation_id").as_deref(),
        Some(wire_session_id_str.as_str())
    );
    assert_eq!(
        request.header("session_id").as_deref(),
        Some(wire_session_id_str.as_str())
    );

    let extra_header = request.header("extra").expect("extra header missing");
    let extra_json: Value =
        serde_json::from_str(&extra_header).expect("extra header should be valid JSON");
    assert_eq!(
        extra_json.get("session_id").and_then(Value::as_str),
        Some(wire_session_id_str.as_str())
    );

    let body = request.body_json();
    assert_eq!(
        body.get("prompt_cache_key").and_then(Value::as_str),
        Some(conversation_id_str.as_str())
    );
}

#[tokio::test]
async fn resume_reuses_wire_session_header_after_fork() {
    core_test_support::skip_if_no_network!();

    let server = responses::start_mock_server().await;
    let sse_body = responses::sse(vec![
        responses::ev_response_created("resp-1"),
        responses::ev_completed("resp-1"),
    ]);

    // Base conversation (two turns).
    responses::mount_sse_once(&server, sse_body.clone()).await;
    responses::mount_sse_once(&server, sse_body.clone()).await;
    // Forked conversation turn.
    responses::mount_sse_once(&server, sse_body.clone()).await;
    // Resumed conversation turn (capture headers).
    let resume_mock = responses::mount_sse_once(&server, sse_body).await;

    let provider = ModelProviderInfo {
        name: "mock".into(),
        base_url: Some(format!("{}/v1", server.uri())),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
        requires_openai_auth: false,
    };

    let codex_home = TempDir::new().expect("failed to create TempDir");
    let mut config = load_default_config_for_test(&codex_home);
    config.model_provider_id = provider.name.clone();
    config.model_provider = provider.clone();

    let manager = ConversationManager::with_auth(CodexAuth::from_api_key("dummy"));
    let NewConversation {
        conversation: base_conv,
        conversation_id: base_id,
        ..
    } = manager
        .new_conversation(config.clone())
        .await
        .expect("create base conversation");

    // Seed base conversation with two user turns so forked history retains state.
    base_conv
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "hello base 1".into(),
            }],
        })
        .await
        .expect("submit base turn 1");
    wait_for_event(&base_conv, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    base_conv
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "hello base 2".into(),
            }],
        })
        .await
        .expect("submit base turn 2");
    wait_for_event(&base_conv, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    let base_path = base_conv.rollout_path();

    let NewConversation {
        conversation: fork_conv,
        conversation_id: fork_id,
        ..
    } = manager
        .fork_conversation(1, config.clone(), base_path.clone())
        .await
        .expect("fork conversation");
    assert_ne!(
        base_id, fork_id,
        "forked conversation should have distinct conversation_id"
    );

    fork_conv
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "hello fork".into(),
            }],
        })
        .await
        .expect("submit fork turn");
    wait_for_event(&fork_conv, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    let fork_path = fork_conv.rollout_path();

    let resume_auth = AuthManager::from_auth_for_testing(CodexAuth::from_api_key("dummy"));
    let NewConversation {
        conversation: resumed_conv,
        ..
    } = manager
        .resume_conversation_from_rollout(config.clone(), fork_path, resume_auth)
        .await
        .expect("resume conversation");

    resumed_conv
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "hello resume".into(),
            }],
        })
        .await
        .expect("submit resumed turn");
    wait_for_event(&resumed_conv, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    let resume_request = resume_mock.single_request();
    let base_id_str = base_id.to_string();
    assert_eq!(
        resume_request.header("conversation_id").as_deref(),
        Some(base_id_str.as_str())
    );
    assert_eq!(
        resume_request.header("session_id").as_deref(),
        Some(base_id_str.as_str())
    );
    let extra_header = resume_request
        .header("extra")
        .expect("extra header missing");
    let extra_json: Value =
        serde_json::from_str(&extra_header).expect("extra header should be valid JSON");
    assert_eq!(
        extra_json.get("session_id").and_then(Value::as_str),
        Some(base_id_str.as_str())
    );

    let body = resume_request.body_json();
    assert_eq!(
        body.get("prompt_cache_key").and_then(Value::as_str),
        Some(fork_id.to_string().as_str())
    );
}

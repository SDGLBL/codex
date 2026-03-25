#![cfg(not(target_os = "windows"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use anyhow::Result;
use core_test_support::responses;
use core_test_support::test_codex_exec::test_codex_exec;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

const SPAWN_CALL_ID: &str = "spawn-call-1";
const PARENT_PROMPT: &str = "spawn a child and finish";

fn find_sidecar_path(output_dir: &Path, expected_extension: Option<&str>) -> Result<PathBuf> {
    let mut matches = fs::read_dir(output_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("subagent-"))
        })
        .filter(|path| match expected_extension {
            Some(extension) => path.extension().and_then(|value| value.to_str()) == Some(extension),
            None => path.extension().is_none(),
        })
        .collect::<Vec<_>>();
    matches.sort();

    assert_eq!(matches.len(), 1);
    Ok(matches.remove(0))
}

async fn run_exec_with_subagent_sidecar(
    json_mode: bool,
    output_dir_name: &str,
    child_prompt: &str,
    child_message: &str,
) -> Result<(PathBuf, String)> {
    let test = test_codex_exec();
    let output_dir = test.cwd_path().join(output_dir_name);
    let server = responses::start_mock_server().await;
    let spawn_args = serde_json::to_string(&json!({
        "message": child_prompt,
    }))?;
    let follow_up_response = responses::sse(vec![
        responses::ev_response_created("resp-follow-up"),
        responses::ev_assistant_message("msg-follow-up", child_message),
        responses::ev_completed("resp-follow-up"),
    ]);

    let request_log = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(vec![
                responses::ev_response_created("resp-parent-1"),
                responses::ev_function_call(SPAWN_CALL_ID, "spawn_agent", &spawn_args),
                responses::ev_completed("resp-parent-1"),
            ]),
            follow_up_response.clone(),
            follow_up_response,
        ],
    )
    .await;

    let mut cmd = test.cmd_with_server(&server);
    cmd.arg("--skip-git-repo-check")
        .arg("--subagent-output-dir")
        .arg(&output_dir);
    if json_mode {
        cmd.arg("--json");
    }
    cmd.arg(PARENT_PROMPT).assert().success();

    assert!(
        request_log
            .requests()
            .into_iter()
            .any(|request| request.body_contains_text(child_prompt)),
        "missing child request for prompt: {child_prompt}"
    );
    let sidecar_path = find_sidecar_path(&output_dir, json_mode.then_some("jsonl"))?;
    let contents = fs::read_to_string(&sidecar_path)?;

    Ok((sidecar_path, contents))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_writes_subagent_human_sidecar() -> Result<()> {
    let (sidecar_path, contents) = run_exec_with_subagent_sidecar(
        /*json_mode*/ false,
        "subagent-human",
        "child: human sidecar",
        "child human done",
    )
    .await?;

    assert_eq!(sidecar_path.extension(), None);
    assert!(contents.contains("OpenAI Codex v"));
    assert!(contents.contains("child human done"));
    assert!(!contents.contains('\u{1b}'));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_writes_subagent_json_sidecar() -> Result<()> {
    let (sidecar_path, contents) = run_exec_with_subagent_sidecar(
        /*json_mode*/ true,
        "subagent-json",
        "child: json sidecar",
        "child json done",
    )
    .await?;

    assert_eq!(
        sidecar_path.extension().and_then(|value| value.to_str()),
        Some("jsonl")
    );
    assert!(contents.contains("\"type\":\"thread.started\""));
    assert!(contents.contains("child json done"));
    Ok(())
}

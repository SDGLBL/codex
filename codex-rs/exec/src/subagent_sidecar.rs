use crate::event_processor_with_jsonl_output::EventProcessorWithJsonOutput;
use crate::exec_events;
use codex_app_server_protocol::ServerNotification;
use codex_core::web_search::web_search_detail;
use codex_protocol::protocol::AgentMessageEvent;
use codex_protocol::protocol::AgentStatus;
use codex_protocol::protocol::BackgroundEventEvent;
use codex_protocol::protocol::CollabAgentInteractionEndEvent;
use codex_protocol::protocol::CollabAgentSpawnEndEvent;
use codex_protocol::protocol::CollabCloseEndEvent;
use codex_protocol::protocol::CollabResumeBeginEvent;
use codex_protocol::protocol::CollabResumeEndEvent;
use codex_protocol::protocol::CollabWaitingBeginEvent;
use codex_protocol::protocol::CollabWaitingEndEvent;
use codex_protocol::protocol::DeprecationNoticeEvent;
use codex_protocol::protocol::ErrorEvent;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ExecCommandBeginEvent;
use codex_protocol::protocol::ExecCommandEndEvent;
use codex_protocol::protocol::FileChange;
use codex_protocol::protocol::McpInvocation;
use codex_protocol::protocol::McpToolCallBeginEvent;
use codex_protocol::protocol::McpToolCallEndEvent;
use codex_protocol::protocol::PatchApplyBeginEvent;
use codex_protocol::protocol::PatchApplyEndEvent;
use codex_protocol::protocol::SessionConfiguredEvent;
use codex_protocol::protocol::StreamErrorEvent;
use codex_protocol::protocol::TurnAbortReason;
use codex_protocol::protocol::TurnCompleteEvent;
use codex_protocol::protocol::TurnDiffEvent;
use codex_protocol::protocol::WarningEvent;
use codex_protocol::protocol::WebSearchEndEvent;
use codex_utils_elapsed::format_duration;
use serde_json::Value as JsonValue;
use shlex::try_join;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufWriter;
use std::io::Error;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidecarMode {
    Json,
    Human,
}

pub(crate) struct SubagentSidecarRegistry {
    output_dir: PathBuf,
    mode: SidecarMode,
    primary_thread_id: String,
    nicknames: HashMap<String, String>,
    seen_threads: HashSet<String>,
    active_threads: HashSet<String>,
    threads: HashMap<String, ThreadSidecar>,
}

impl SubagentSidecarRegistry {
    pub(crate) fn new(
        primary_thread_id: String,
        output_dir: PathBuf,
        json_mode: bool,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&output_dir)?;
        Ok(Self {
            output_dir,
            mode: if json_mode {
                SidecarMode::Json
            } else {
                SidecarMode::Human
            },
            primary_thread_id,
            nicknames: HashMap::new(),
            seen_threads: HashSet::new(),
            active_threads: HashSet::new(),
            threads: HashMap::new(),
        })
    }

    pub(crate) fn observe_server_notification(
        &mut self,
        notification: &ServerNotification,
    ) -> std::io::Result<()> {
        match notification {
            ServerNotification::ThreadStarted(notification) => {
                if notification.thread.id == self.primary_thread_id {
                    return Ok(());
                }
                self.mark_thread_seen(notification.thread.id.as_str());
                self.mark_thread_active(notification.thread.id.as_str());
                if let Some(agent_nickname) = notification
                    .thread
                    .agent_nickname
                    .as_deref()
                    .filter(|agent_nickname| !agent_nickname.is_empty())
                {
                    self.record_nickname(notification.thread.id.as_str(), agent_nickname)?;
                }
                self.observe_thread_event(
                    notification.thread.id.as_str(),
                    exec_events::ThreadEvent::ThreadStarted(exec_events::ThreadStartedEvent {
                        thread_id: notification.thread.id.clone(),
                    }),
                )?;
            }
            ServerNotification::TurnStarted(notification) => {
                self.mark_thread_seen(notification.thread_id.as_str());
                self.mark_thread_active(notification.thread_id.as_str());
                self.observe_thread_event(
                    notification.thread_id.as_str(),
                    exec_events::ThreadEvent::TurnStarted(exec_events::TurnStartedEvent::default()),
                )?;
            }
            ServerNotification::TurnCompleted(notification) => {
                self.mark_thread_seen(notification.thread_id.as_str());
                self.mark_thread_inactive(notification.thread_id.as_str());
                let thread_event = match notification.turn.status {
                    codex_app_server_protocol::TurnStatus::Completed => {
                        exec_events::ThreadEvent::TurnCompleted(exec_events::TurnCompletedEvent {
                            usage: exec_events::Usage::default(),
                        })
                    }
                    codex_app_server_protocol::TurnStatus::Failed => {
                        let message = notification
                            .turn
                            .error
                            .as_ref()
                            .map(|error| error.message.clone())
                            .unwrap_or_else(|| "turn failed".to_string());
                        exec_events::ThreadEvent::TurnFailed(exec_events::TurnFailedEvent {
                            error: exec_events::ThreadErrorEvent { message },
                        })
                    }
                    codex_app_server_protocol::TurnStatus::Interrupted => {
                        exec_events::ThreadEvent::TurnFailed(exec_events::TurnFailedEvent {
                            error: exec_events::ThreadErrorEvent {
                                message: "task interrupted".to_string(),
                            },
                        })
                    }
                    codex_app_server_protocol::TurnStatus::InProgress => return Ok(()),
                };
                self.observe_thread_event(notification.thread_id.as_str(), thread_event)?;
            }
            ServerNotification::ItemStarted(notification) => {
                self.capture_thread_ids_from_app_item(&notification.item);
                self.mark_thread_seen(notification.thread_id.as_str());
                if let Some(item) = convert_app_thread_item(notification.item.clone()) {
                    self.observe_thread_event(
                        notification.thread_id.as_str(),
                        exec_events::ThreadEvent::ItemStarted(exec_events::ItemStartedEvent {
                            item,
                        }),
                    )?;
                }
            }
            ServerNotification::ItemCompleted(notification) => {
                self.capture_thread_ids_from_app_item(&notification.item);
                self.mark_thread_seen(notification.thread_id.as_str());
                if let Some(item) = convert_app_thread_item(notification.item.clone()) {
                    self.observe_thread_event(
                        notification.thread_id.as_str(),
                        exec_events::ThreadEvent::ItemCompleted(exec_events::ItemCompletedEvent {
                            item,
                        }),
                    )?;
                }
            }
            ServerNotification::Error(notification) => {
                if notification.thread_id != self.primary_thread_id {
                    self.mark_thread_seen(notification.thread_id.as_str());
                    if !notification.will_retry {
                        self.mark_thread_inactive(notification.thread_id.as_str());
                    }
                    let message = match notification.error.additional_details.as_deref() {
                        Some(details) if !details.trim().is_empty() => {
                            format!("{} ({details})", notification.error.message)
                        }
                        _ => notification.error.message.clone(),
                    };
                    self.observe_thread_event(
                        notification.thread_id.as_str(),
                        exec_events::ThreadEvent::Error(exec_events::ThreadErrorEvent { message }),
                    )?;
                }
            }
            ServerNotification::ThreadStatusChanged(notification) => {
                self.mark_thread_seen(notification.thread_id.as_str());
                match notification.status {
                    codex_app_server_protocol::ThreadStatus::Active { .. } => {
                        self.mark_thread_active(notification.thread_id.as_str());
                    }
                    codex_app_server_protocol::ThreadStatus::Idle
                    | codex_app_server_protocol::ThreadStatus::NotLoaded
                    | codex_app_server_protocol::ThreadStatus::SystemError => {
                        self.mark_thread_inactive(notification.thread_id.as_str());
                    }
                }
            }
            ServerNotification::ThreadClosed(notification) => {
                self.mark_thread_seen(notification.thread_id.as_str());
                self.mark_thread_inactive(notification.thread_id.as_str());
            }
            _ => {}
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn observe_event(
        &mut self,
        conversation_id: Option<&str>,
        event: &Event,
    ) -> std::io::Result<()> {
        self.capture_nicknames(event)?;

        let Some(thread_id) = conversation_id else {
            return Ok(());
        };
        if thread_id == self.primary_thread_id {
            return Ok(());
        }

        let nickname = self.nicknames.get(thread_id).cloned();
        let thread = self
            .threads
            .entry(thread_id.to_string())
            .or_insert_with(|| ThreadSidecar::new(thread_id, self.mode));
        if let Some(nickname) = nickname.as_deref() {
            thread.record_nickname(nickname, &self.output_dir)?;
        }
        thread.push_event(event.clone(), &self.output_dir)
    }

    pub(crate) fn finalize(&mut self) -> std::io::Result<()> {
        for (thread_id, thread) in &mut self.threads {
            if let Some(nickname) = self.nicknames.get(thread_id).cloned() {
                thread.record_nickname(nickname.as_str(), &self.output_dir)?;
            }
            thread.finalize(&self.output_dir)?;
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn capture_nicknames(&mut self, event: &Event) -> std::io::Result<()> {
        match &event.msg {
            EventMsg::CollabAgentSpawnEnd(CollabAgentSpawnEndEvent {
                new_thread_id: Some(new_thread_id),
                new_agent_nickname: Some(new_agent_nickname),
                ..
            }) if !new_agent_nickname.is_empty() => {
                self.record_nickname(new_thread_id.to_string().as_str(), new_agent_nickname)?;
            }
            EventMsg::CollabResumeBegin(CollabResumeBeginEvent {
                receiver_thread_id,
                receiver_agent_nickname: Some(receiver_agent_nickname),
                ..
            }) if !receiver_agent_nickname.is_empty() => {
                self.record_nickname(
                    receiver_thread_id.to_string().as_str(),
                    receiver_agent_nickname,
                )?;
            }
            EventMsg::CollabResumeEnd(CollabResumeEndEvent {
                receiver_thread_id,
                receiver_agent_nickname: Some(receiver_agent_nickname),
                ..
            }) if !receiver_agent_nickname.is_empty() => {
                self.record_nickname(
                    receiver_thread_id.to_string().as_str(),
                    receiver_agent_nickname,
                )?;
            }
            EventMsg::CollabAgentInteractionEnd(CollabAgentInteractionEndEvent {
                receiver_thread_id,
                receiver_agent_nickname: Some(receiver_agent_nickname),
                ..
            }) if !receiver_agent_nickname.is_empty() => {
                self.record_nickname(
                    receiver_thread_id.to_string().as_str(),
                    receiver_agent_nickname,
                )?;
            }
            EventMsg::CollabCloseEnd(CollabCloseEndEvent {
                receiver_thread_id,
                receiver_agent_nickname: Some(receiver_agent_nickname),
                ..
            }) if !receiver_agent_nickname.is_empty() => {
                self.record_nickname(
                    receiver_thread_id.to_string().as_str(),
                    receiver_agent_nickname,
                )?;
            }
            EventMsg::CollabWaitingBegin(CollabWaitingBeginEvent {
                receiver_agents, ..
            }) => {
                for receiver in receiver_agents {
                    if let Some(agent_nickname) = receiver
                        .agent_nickname
                        .as_deref()
                        .filter(|agent_nickname| !agent_nickname.is_empty())
                    {
                        self.record_nickname(
                            receiver.thread_id.to_string().as_str(),
                            agent_nickname,
                        )?;
                    }
                }
            }
            EventMsg::CollabWaitingEnd(CollabWaitingEndEvent { agent_statuses, .. }) => {
                for status in agent_statuses {
                    if let Some(agent_nickname) = status
                        .agent_nickname
                        .as_deref()
                        .filter(|agent_nickname| !agent_nickname.is_empty())
                    {
                        self.record_nickname(
                            status.thread_id.to_string().as_str(),
                            agent_nickname,
                        )?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn record_nickname(&mut self, thread_id: &str, nickname: &str) -> std::io::Result<()> {
        self.nicknames
            .entry(thread_id.to_string())
            .or_insert_with(|| nickname.to_string());
        if let Some(thread) = self.threads.get_mut(thread_id) {
            thread.record_nickname(nickname, &self.output_dir)?;
        }
        Ok(())
    }

    fn observe_thread_event(
        &mut self,
        thread_id: &str,
        thread_event: exec_events::ThreadEvent,
    ) -> std::io::Result<()> {
        if thread_id == self.primary_thread_id {
            return Ok(());
        }

        let nickname = self.nicknames.get(thread_id).cloned();
        let thread = self
            .threads
            .entry(thread_id.to_string())
            .or_insert_with(|| ThreadSidecar::new(thread_id, self.mode));
        if let Some(nickname) = nickname.as_deref() {
            thread.record_nickname(nickname, &self.output_dir)?;
        }
        thread.push_thread_event(thread_event, &self.output_dir)
    }

    pub(crate) fn has_seen_threads(&self) -> bool {
        !self.seen_threads.is_empty()
    }

    pub(crate) fn has_active_threads(&self) -> bool {
        !self.active_threads.is_empty()
    }

    pub(crate) fn thread_ids_needing_backfill(&self) -> Vec<String> {
        self.seen_threads
            .iter()
            .filter(|thread_id| {
                self.threads
                    .get(thread_id.as_str())
                    .is_none_or(ThreadSidecar::needs_backfill)
            })
            .cloned()
            .collect()
    }

    pub(crate) fn ingest_thread(
        &mut self,
        thread: codex_app_server_protocol::Thread,
    ) -> std::io::Result<()> {
        if thread.id == self.primary_thread_id {
            return Ok(());
        }

        let thread_id = thread.id.clone();
        if let Some(agent_nickname) = thread.agent_nickname.as_deref() {
            self.record_nickname(thread_id.as_str(), agent_nickname)?;
        }

        let nickname = self.nicknames.get(thread_id.as_str()).cloned();
        let thread_sidecar = self
            .threads
            .entry(thread_id.clone())
            .or_insert_with(|| ThreadSidecar::new(thread_id.as_str(), self.mode));
        if let Some(nickname) = nickname.as_deref() {
            thread_sidecar.record_nickname(nickname, &self.output_dir)?;
        }
        if !thread_sidecar.has_thread_started() {
            thread_sidecar.push_thread_event(
                exec_events::ThreadEvent::ThreadStarted(exec_events::ThreadStartedEvent {
                    thread_id: thread_id.clone(),
                }),
                &self.output_dir,
            )?;
        }
        for turn in thread.turns {
            for item in turn.items {
                if let Some(item) = convert_app_thread_item(item) {
                    thread_sidecar.push_thread_event(
                        exec_events::ThreadEvent::ItemCompleted(exec_events::ItemCompletedEvent {
                            item,
                        }),
                        &self.output_dir,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn mark_thread_seen(&mut self, thread_id: &str) {
        if thread_id != self.primary_thread_id {
            self.seen_threads.insert(thread_id.to_string());
        }
    }

    fn mark_thread_active(&mut self, thread_id: &str) {
        if thread_id != self.primary_thread_id {
            self.active_threads.insert(thread_id.to_string());
        }
    }

    fn mark_thread_inactive(&mut self, thread_id: &str) {
        self.active_threads.remove(thread_id);
    }

    fn capture_thread_ids_from_app_item(&mut self, item: &codex_app_server_protocol::ThreadItem) {
        if let codex_app_server_protocol::ThreadItem::CollabAgentToolCall {
            receiver_thread_ids,
            ..
        } = item
        {
            for receiver_thread_id in receiver_thread_ids {
                self.mark_thread_seen(receiver_thread_id);
                self.mark_thread_active(receiver_thread_id);
            }
        }
    }
}

struct ThreadSidecar {
    mode: SidecarMode,
    thread_id: String,
    nickname: Option<String>,
    writer: Option<SidecarWriter>,
    buffered_events: Vec<Event>,
    buffered_thread_events: Vec<exec_events::ThreadEvent>,
    recorded_item_events: usize,
    saw_thread_started: bool,
}

impl ThreadSidecar {
    fn new(thread_id: &str, mode: SidecarMode) -> Self {
        Self {
            mode,
            thread_id: thread_id.to_string(),
            nickname: None,
            writer: None,
            buffered_events: Vec::new(),
            buffered_thread_events: Vec::new(),
            recorded_item_events: 0,
            saw_thread_started: false,
        }
    }

    fn record_nickname(&mut self, nickname: &str, output_dir: &Path) -> std::io::Result<()> {
        if self.nickname.as_deref() == Some(nickname) {
            return Ok(());
        }
        self.nickname = Some(nickname.to_string());
        self.ensure_writer(output_dir, /*force_without_nickname*/ false)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn push_event(&mut self, event: Event, output_dir: &Path) -> std::io::Result<()> {
        if matches!(event.msg, EventMsg::SessionConfigured(_)) {
            self.saw_thread_started = true;
        }
        if let Some(writer) = &mut self.writer {
            return writer.write_event(&event);
        }
        self.buffered_events.push(event);
        self.ensure_writer(output_dir, /*force_without_nickname*/ false)
    }

    fn push_thread_event(
        &mut self,
        thread_event: exec_events::ThreadEvent,
        output_dir: &Path,
    ) -> std::io::Result<()> {
        if matches!(&thread_event, exec_events::ThreadEvent::ThreadStarted(_))
            && self.saw_thread_started
        {
            return Ok(());
        }

        if !self.saw_thread_started
            && !matches!(&thread_event, exec_events::ThreadEvent::ThreadStarted(_))
        {
            self.push_thread_event(
                exec_events::ThreadEvent::ThreadStarted(exec_events::ThreadStartedEvent {
                    thread_id: self.thread_id.clone(),
                }),
                output_dir,
            )?;
        }

        match &thread_event {
            exec_events::ThreadEvent::ThreadStarted(_) => {
                self.saw_thread_started = true;
            }
            exec_events::ThreadEvent::ItemStarted(_)
            | exec_events::ThreadEvent::ItemUpdated(_)
            | exec_events::ThreadEvent::ItemCompleted(_) => {
                self.recorded_item_events += 1;
            }
            _ => {}
        }
        if let Some(writer) = &mut self.writer {
            return writer.write_thread_event(&thread_event);
        }
        self.buffered_thread_events.push(thread_event);
        self.ensure_writer(output_dir, /*force_without_nickname*/ false)
    }

    fn finalize(&mut self, output_dir: &Path) -> std::io::Result<()> {
        self.ensure_writer(output_dir, /*force_without_nickname*/ true)
    }

    fn ensure_writer(
        &mut self,
        output_dir: &Path,
        force_without_nickname: bool,
    ) -> std::io::Result<()> {
        if self.writer.is_some()
            || (self.buffered_events.is_empty() && self.buffered_thread_events.is_empty())
        {
            return Ok(());
        }

        if self.nickname.is_none() && !force_without_nickname {
            return Ok(());
        }

        let path = build_sidecar_path(
            output_dir,
            self.nickname.as_deref(),
            self.thread_id.as_str(),
            self.mode,
        );
        let mut writer = SidecarWriter::create(path, self.mode)?;
        for thread_event in self.buffered_thread_events.drain(..) {
            writer.write_thread_event(&thread_event)?;
        }
        for event in self.buffered_events.drain(..) {
            writer.write_event(&event)?;
        }
        self.writer = Some(writer);
        Ok(())
    }

    fn needs_backfill(&self) -> bool {
        self.recorded_item_events == 0
    }

    fn has_thread_started(&self) -> bool {
        self.saw_thread_started
    }
}

enum SidecarWriter {
    Json(Box<JsonSidecarWriter>),
    Human(HumanSidecarWriter),
}

impl SidecarWriter {
    fn create(path: PathBuf, mode: SidecarMode) -> std::io::Result<Self> {
        match mode {
            SidecarMode::Json => Ok(Self::Json(Box::new(JsonSidecarWriter::new(path)?))),
            SidecarMode::Human => Ok(Self::Human(HumanSidecarWriter::new(path)?)),
        }
    }

    fn write_event(&mut self, event: &Event) -> std::io::Result<()> {
        match self {
            Self::Json(writer) => writer.write_event(event),
            Self::Human(writer) => writer.write_event(event),
        }
    }

    fn write_thread_event(&mut self, event: &exec_events::ThreadEvent) -> std::io::Result<()> {
        match self {
            Self::Json(writer) => writer.write_thread_event(event),
            Self::Human(writer) => writer.write_thread_event(event),
        }
    }
}

struct JsonSidecarWriter {
    file: BufWriter<File>,
    processor: EventProcessorWithJsonOutput,
}

impl JsonSidecarWriter {
    fn new(path: PathBuf) -> std::io::Result<Self> {
        Ok(Self {
            file: open_sidecar_file(&path)?,
            processor: EventProcessorWithJsonOutput::new(None),
        })
    }

    fn write_event(&mut self, event: &Event) -> std::io::Result<()> {
        for thread_event in self.processor.collect_thread_events(event) {
            let line = serde_json::to_string(&thread_event)
                .map_err(|err| Error::other(format!("failed to serialize thread event: {err}")))?;
            writeln!(self.file, "{line}")?;
        }
        self.file.flush()
    }

    fn write_thread_event(&mut self, event: &exec_events::ThreadEvent) -> std::io::Result<()> {
        let line = serde_json::to_string(event)
            .map_err(|err| Error::other(format!("failed to serialize thread event: {err}")))?;
        writeln!(self.file, "{line}")?;
        self.file.flush()
    }
}

struct HumanSidecarWriter {
    file: BufWriter<File>,
}

impl HumanSidecarWriter {
    fn new(path: PathBuf) -> std::io::Result<Self> {
        Ok(Self {
            file: open_sidecar_file(&path)?,
        })
    }

    fn write_event(&mut self, event: &Event) -> std::io::Result<()> {
        match &event.msg {
            EventMsg::SessionConfigured(SessionConfiguredEvent {
                session_id, model, ..
            }) => {
                writeln!(self.file, "OpenAI Codex v{VERSION} (subagent transcript)")?;
                writeln!(self.file, "codex session {session_id}")?;
                writeln!(self.file, "model: {model}")?;
                writeln!(self.file)?;
            }
            EventMsg::BackgroundEvent(BackgroundEventEvent { message }) => {
                writeln!(self.file, "{message}")?;
            }
            EventMsg::Warning(WarningEvent { message }) => {
                writeln!(self.file, "warning: {message}")?;
            }
            EventMsg::Error(ErrorEvent { message, .. }) => {
                writeln!(self.file, "ERROR: {message}")?;
            }
            EventMsg::StreamError(StreamErrorEvent {
                message,
                additional_details,
                ..
            }) => {
                if let Some(additional_details) = additional_details
                    .as_deref()
                    .filter(|details| !details.trim().is_empty())
                {
                    writeln!(self.file, "{message} ({additional_details})")?;
                } else {
                    writeln!(self.file, "{message}")?;
                }
            }
            EventMsg::AgentMessage(AgentMessageEvent { message, .. }) => {
                writeln!(self.file, "codex")?;
                writeln!(self.file, "{message}")?;
            }
            EventMsg::AgentReasoning(reasoning) => {
                writeln!(self.file, "thinking")?;
                writeln!(self.file, "{}", reasoning.text)?;
            }
            EventMsg::ExecCommandBegin(ExecCommandBeginEvent { command, cwd, .. }) => {
                writeln!(
                    self.file,
                    "exec\n{} in {}",
                    escape_command(command),
                    cwd.to_string_lossy()
                )?;
            }
            EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                aggregated_output,
                duration,
                exit_code,
                ..
            }) => {
                let duration = format_duration(*duration);
                if *exit_code == 0 {
                    writeln!(self.file, "succeeded in {duration}:")?;
                } else {
                    writeln!(self.file, "exited {exit_code} in {duration}:")?;
                }
                if !aggregated_output.is_empty() {
                    writeln!(self.file, "{aggregated_output}")?;
                }
            }
            EventMsg::McpToolCallBegin(McpToolCallBeginEvent { invocation, .. }) => {
                writeln!(self.file, "tool {}", format_mcp_invocation(invocation))?;
            }
            EventMsg::McpToolCallEnd(McpToolCallEndEvent {
                invocation,
                duration,
                result,
                ..
            }) => {
                let status = if result.is_ok() { "success" } else { "failed" };
                writeln!(
                    self.file,
                    "{} {status} in {}:",
                    format_mcp_invocation(invocation),
                    format_duration(*duration)
                )?;
                if let Ok(result) = result {
                    let value = serde_json::to_value(result).map_err(|err| {
                        Error::other(format!("failed to serialize MCP result: {err}"))
                    })?;
                    writeln!(self.file, "{}", pretty_json(&value)?)?;
                }
            }
            EventMsg::WebSearchBegin(_) => {
                writeln!(self.file, "Searching the web...")?;
            }
            EventMsg::WebSearchEnd(WebSearchEndEvent { query, action, .. }) => {
                let detail = web_search_detail(Some(action), query);
                if detail.is_empty() {
                    writeln!(self.file, "Searched the web")?;
                } else {
                    writeln!(self.file, "Searched: {detail}")?;
                }
            }
            EventMsg::PatchApplyBegin(PatchApplyBeginEvent { changes, .. }) => {
                writeln!(self.file, "file update")?;
                for (path, change) in changes {
                    match change {
                        FileChange::Add { .. } | FileChange::Delete { .. } => {
                            writeln!(
                                self.file,
                                "{} {}",
                                format_file_change(change),
                                path.to_string_lossy()
                            )?;
                        }
                        FileChange::Update { move_path, .. } => {
                            if let Some(move_path) = move_path {
                                writeln!(
                                    self.file,
                                    "{} {} -> {}",
                                    format_file_change(change),
                                    path.to_string_lossy(),
                                    move_path.to_string_lossy()
                                )?;
                            } else {
                                writeln!(
                                    self.file,
                                    "{} {}",
                                    format_file_change(change),
                                    path.to_string_lossy()
                                )?;
                            }
                        }
                    }
                }
            }
            EventMsg::PatchApplyEnd(PatchApplyEndEvent {
                success,
                stdout,
                stderr,
                ..
            }) => {
                if *success {
                    writeln!(self.file, "apply_patch exited 0:")?;
                    if !stdout.is_empty() {
                        writeln!(self.file, "{stdout}")?;
                    }
                } else {
                    writeln!(self.file, "apply_patch exited 1:")?;
                    if !stderr.is_empty() {
                        writeln!(self.file, "{stderr}")?;
                    }
                }
            }
            EventMsg::PlanUpdate(plan_update) => {
                writeln!(self.file, "Plan update")?;
                if let Some(explanation) = plan_update
                    .explanation
                    .as_deref()
                    .filter(|explanation| !explanation.trim().is_empty())
                {
                    writeln!(self.file, "{explanation}")?;
                }
                for step in &plan_update.plan {
                    writeln!(self.file, "- {:?} {}", step.status, step.step)?;
                }
            }
            EventMsg::TurnDiff(TurnDiffEvent { unified_diff }) => {
                writeln!(self.file, "file update:")?;
                writeln!(self.file, "{unified_diff}")?;
            }
            EventMsg::TurnComplete(TurnCompleteEvent { .. }) => {
                writeln!(self.file, "task complete")?;
            }
            EventMsg::TurnAborted(turn_aborted) => {
                let message = match turn_aborted.reason {
                    TurnAbortReason::Interrupted => "task interrupted",
                    TurnAbortReason::Replaced => "task aborted: replaced by a new task",
                    TurnAbortReason::ReviewEnded => "task aborted: review ended",
                };
                writeln!(self.file, "{message}")?;
            }
            EventMsg::ContextCompacted(_) => {
                writeln!(self.file, "context compacted")?;
            }
            EventMsg::DeprecationNotice(DeprecationNoticeEvent { summary, details }) => {
                writeln!(self.file, "deprecated: {summary}")?;
                if let Some(details) = details {
                    writeln!(self.file, "{details}")?;
                }
            }
            EventMsg::CollabAgentSpawnEnd(CollabAgentSpawnEndEvent {
                call_id,
                new_thread_id,
                prompt,
                status,
                ..
            }) => {
                writeln!(
                    self.file,
                    "{} {}:",
                    format_collab_invocation("spawn_agent", call_id, Some(prompt)),
                    format_collab_status(status)
                )?;
                if let Some(new_thread_id) = new_thread_id {
                    writeln!(self.file, "  agent: {new_thread_id}")?;
                }
            }
            EventMsg::CollabWaitingBegin(CollabWaitingBeginEvent {
                call_id,
                receiver_thread_ids,
                ..
            }) => {
                writeln!(
                    self.file,
                    "{}",
                    format_collab_invocation("wait_agent", call_id, None)
                )?;
                writeln!(
                    self.file,
                    "  receivers: {}",
                    format_receiver_list(receiver_thread_ids)
                )?;
            }
            EventMsg::CollabWaitingEnd(CollabWaitingEndEvent {
                call_id, statuses, ..
            }) => {
                writeln!(
                    self.file,
                    "{} complete:",
                    format_collab_invocation("wait_agent", call_id, None)
                )?;
                for (thread_id, status) in statuses {
                    writeln!(self.file, "  {thread_id}: {}", format_collab_status(status))?;
                }
            }
            EventMsg::ViewImageToolCall(view_image) => {
                writeln!(self.file, "viewed image {}", view_image.path.display())?;
            }
            _ => {}
        }
        self.file.flush()
    }

    fn write_thread_event(&mut self, event: &exec_events::ThreadEvent) -> std::io::Result<()> {
        match event {
            exec_events::ThreadEvent::ThreadStarted(event) => {
                writeln!(self.file, "OpenAI Codex v{VERSION} (subagent transcript)")?;
                writeln!(self.file, "codex session {}", event.thread_id)?;
                writeln!(self.file)?;
            }
            exec_events::ThreadEvent::TurnStarted(_) | exec_events::ThreadEvent::ItemUpdated(_) => {
            }
            exec_events::ThreadEvent::TurnCompleted(_) => {
                writeln!(self.file, "task complete")?;
            }
            exec_events::ThreadEvent::TurnFailed(event) => {
                writeln!(self.file, "ERROR: {}", event.error.message)?;
            }
            exec_events::ThreadEvent::Error(event) => {
                writeln!(self.file, "ERROR: {}", event.message)?;
            }
            exec_events::ThreadEvent::ItemStarted(event) => {
                self.write_thread_item_started(&event.item)?;
            }
            exec_events::ThreadEvent::ItemCompleted(event) => {
                self.write_thread_item_completed(&event.item)?;
            }
        }
        self.file.flush()
    }

    fn write_thread_item_started(&mut self, item: &exec_events::ThreadItem) -> std::io::Result<()> {
        match &item.details {
            exec_events::ThreadItemDetails::CommandExecution(command) => {
                writeln!(self.file, "exec")?;
                writeln!(self.file, "{}", command.command)?;
            }
            exec_events::ThreadItemDetails::McpToolCall(tool_call) => {
                writeln!(
                    self.file,
                    "tool {}.{}({})",
                    tool_call.server,
                    tool_call.tool,
                    serde_json::to_string(&tool_call.arguments)
                        .unwrap_or_else(|_| tool_call.arguments.to_string())
                )?;
            }
            exec_events::ThreadItemDetails::CollabToolCall(collab) => {
                writeln!(
                    self.file,
                    "{}({})",
                    format_collab_tool_name(&collab.tool),
                    item.id
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn write_thread_item_completed(
        &mut self,
        item: &exec_events::ThreadItem,
    ) -> std::io::Result<()> {
        match &item.details {
            exec_events::ThreadItemDetails::AgentMessage(message) => {
                writeln!(self.file, "codex")?;
                writeln!(self.file, "{}", message.text)?;
            }
            exec_events::ThreadItemDetails::Reasoning(reasoning) => {
                writeln!(self.file, "thinking")?;
                writeln!(self.file, "{}", reasoning.text)?;
            }
            exec_events::ThreadItemDetails::CommandExecution(command) => {
                match command.status {
                    exec_events::CommandExecutionStatus::Completed => {
                        writeln!(self.file, "succeeded:")?;
                    }
                    exec_events::CommandExecutionStatus::Failed => {
                        writeln!(self.file, "failed:")?;
                    }
                    exec_events::CommandExecutionStatus::Declined => {
                        writeln!(self.file, "declined:")?;
                    }
                    exec_events::CommandExecutionStatus::InProgress => {
                        writeln!(self.file, "exec")?;
                        writeln!(self.file, "{}", command.command)?;
                    }
                }
                if !command.aggregated_output.is_empty() {
                    writeln!(self.file, "{}", command.aggregated_output)?;
                }
            }
            exec_events::ThreadItemDetails::FileChange(file_change) => {
                writeln!(self.file, "file update")?;
                for change in &file_change.changes {
                    writeln!(
                        self.file,
                        "{} {}",
                        format_patch_change_kind(&change.kind),
                        change.path
                    )?;
                }
            }
            exec_events::ThreadItemDetails::McpToolCall(tool_call) => {
                let status = match tool_call.status {
                    exec_events::McpToolCallStatus::Completed => "completed",
                    exec_events::McpToolCallStatus::Failed => "failed",
                    exec_events::McpToolCallStatus::InProgress => "in progress",
                };
                writeln!(
                    self.file,
                    "{}.{} {status}",
                    tool_call.server, tool_call.tool
                )?;
                if let Some(result) = &tool_call.result {
                    writeln!(self.file, "{}", pretty_json(&serde_json::json!(result))?)?;
                }
                if let Some(error) = &tool_call.error {
                    writeln!(self.file, "{}", error.message)?;
                }
            }
            exec_events::ThreadItemDetails::CollabToolCall(collab) => {
                let status = match collab.status {
                    exec_events::CollabToolCallStatus::Completed => "completed",
                    exec_events::CollabToolCallStatus::Failed => "failed",
                    exec_events::CollabToolCallStatus::InProgress => "in progress",
                };
                writeln!(
                    self.file,
                    "{}({}) {status}",
                    format_collab_tool_name(&collab.tool),
                    item.id
                )?;
            }
            exec_events::ThreadItemDetails::Error(error) => {
                writeln!(self.file, "warning: {}", error.message)?;
            }
            exec_events::ThreadItemDetails::WebSearch(_)
            | exec_events::ThreadItemDetails::TodoList(_) => {}
        }
        Ok(())
    }
}

fn build_sidecar_path(
    output_dir: &Path,
    nickname: Option<&str>,
    thread_id: &str,
    mode: SidecarMode,
) -> PathBuf {
    let stem = if let Some(nickname) = nickname.filter(|nickname| !nickname.is_empty()) {
        format!("subagent-{}-{thread_id}", sanitize_file_component(nickname))
    } else {
        format!("subagent-{thread_id}")
    };
    match mode {
        SidecarMode::Json => output_dir.join(format!("{stem}.jsonl")),
        SidecarMode::Human => output_dir.join(stem),
    }
}

fn sanitize_file_component(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    let mut previous_was_separator = false;
    for ch in value.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            previous_was_separator = false;
            Some(ch.to_ascii_lowercase())
        } else if previous_was_separator {
            None
        } else {
            previous_was_separator = true;
            Some('-')
        };
        if let Some(next) = next {
            sanitized.push(next);
        }
    }
    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        "agent".to_string()
    } else {
        sanitized.to_string()
    }
}

fn open_sidecar_file(path: &Path) -> std::io::Result<BufWriter<File>> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    Ok(BufWriter::new(file))
}

fn pretty_json(value: &JsonValue) -> std::io::Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|err| Error::other(format!("failed to pretty print JSON: {err}")))
}

fn escape_command(command: &[String]) -> String {
    try_join(command.iter().map(String::as_str)).unwrap_or_else(|_| command.join(" "))
}

fn format_file_change(change: &FileChange) -> &'static str {
    match change {
        FileChange::Add { .. } => "A",
        FileChange::Delete { .. } => "D",
        FileChange::Update {
            move_path: Some(_), ..
        } => "R",
        FileChange::Update {
            move_path: None, ..
        } => "M",
    }
}

fn format_collab_invocation(tool: &str, call_id: &str, prompt: Option<&str>) -> String {
    let prompt = prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(|prompt| truncate_preview(prompt, 120));
    match prompt {
        Some(prompt) => format!("{tool}({call_id}, prompt=\"{prompt}\")"),
        None => format!("{tool}({call_id})"),
    }
}

fn format_collab_status(status: &AgentStatus) -> String {
    match status {
        AgentStatus::PendingInit => "pending init".to_string(),
        AgentStatus::Running => "running".to_string(),
        AgentStatus::Interrupted => "interrupted".to_string(),
        AgentStatus::Completed(Some(message)) => {
            let preview = truncate_preview(message.trim(), 120);
            if preview.is_empty() {
                "completed".to_string()
            } else {
                format!("completed: \"{preview}\"")
            }
        }
        AgentStatus::Completed(None) => "completed".to_string(),
        AgentStatus::Errored(message) => {
            let preview = truncate_preview(message.trim(), 120);
            if preview.is_empty() {
                "errored".to_string()
            } else {
                format!("errored: \"{preview}\"")
            }
        }
        AgentStatus::Shutdown => "shutdown".to_string(),
        AgentStatus::NotFound => "not found".to_string(),
    }
}

fn format_receiver_list(ids: &[codex_protocol::ThreadId]) -> String {
    if ids.is_empty() {
        return "none".to_string();
    }
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let preview = text.chars().take(max_chars).collect::<String>();
    format!("{preview}...")
}

fn format_mcp_invocation(invocation: &McpInvocation) -> String {
    let fq_tool_name = format!("{}.{}", invocation.server, invocation.tool);
    let args_str = invocation
        .arguments
        .as_ref()
        .map(|value: &serde_json::Value| {
            serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
        })
        .unwrap_or_default();

    if args_str.is_empty() {
        format!("{fq_tool_name}()")
    } else {
        format!("{fq_tool_name}({args_str})")
    }
}

fn convert_app_thread_item(
    item: codex_app_server_protocol::ThreadItem,
) -> Option<exec_events::ThreadItem> {
    let converted = match item {
        codex_app_server_protocol::ThreadItem::AgentMessage { id, text, .. } => {
            exec_events::ThreadItem {
                id,
                details: exec_events::ThreadItemDetails::AgentMessage(
                    exec_events::AgentMessageItem { text },
                ),
            }
        }
        codex_app_server_protocol::ThreadItem::Reasoning {
            id,
            summary,
            content,
        } => {
            let text = if !content.is_empty() {
                content.join("\n")
            } else {
                summary.join("\n")
            };
            exec_events::ThreadItem {
                id,
                details: exec_events::ThreadItemDetails::Reasoning(exec_events::ReasoningItem {
                    text,
                }),
            }
        }
        codex_app_server_protocol::ThreadItem::CommandExecution {
            id,
            command,
            aggregated_output,
            exit_code,
            status,
            ..
        } => exec_events::ThreadItem {
            id,
            details: exec_events::ThreadItemDetails::CommandExecution(
                exec_events::CommandExecutionItem {
                    command,
                    aggregated_output: aggregated_output.unwrap_or_default(),
                    exit_code,
                    status: convert_command_execution_status(status),
                },
            ),
        },
        codex_app_server_protocol::ThreadItem::FileChange {
            id,
            changes,
            status,
        } => exec_events::ThreadItem {
            id,
            details: exec_events::ThreadItemDetails::FileChange(exec_events::FileChangeItem {
                changes: changes
                    .into_iter()
                    .map(|change| exec_events::FileUpdateChange {
                        path: change.path,
                        kind: convert_patch_change_kind(change.kind),
                    })
                    .collect(),
                status: convert_patch_apply_status(status),
            }),
        },
        codex_app_server_protocol::ThreadItem::McpToolCall {
            id,
            server,
            tool,
            arguments,
            result,
            error,
            status,
            ..
        } => exec_events::ThreadItem {
            id,
            details: exec_events::ThreadItemDetails::McpToolCall(exec_events::McpToolCallItem {
                server,
                tool,
                arguments,
                result: result.map(|result| exec_events::McpToolCallItemResult {
                    content: result.content,
                    structured_content: result.structured_content,
                }),
                error: error.map(|error| exec_events::McpToolCallItemError {
                    message: error.message,
                }),
                status: convert_mcp_tool_call_status(status),
            }),
        },
        codex_app_server_protocol::ThreadItem::CollabAgentToolCall {
            id,
            tool,
            sender_thread_id,
            receiver_thread_ids,
            prompt,
            agents_states,
            status,
            ..
        } => {
            let tool = convert_collab_tool(tool)?;
            exec_events::ThreadItem {
                id,
                details: exec_events::ThreadItemDetails::CollabToolCall(
                    exec_events::CollabToolCallItem {
                        tool,
                        sender_thread_id,
                        receiver_thread_ids,
                        prompt,
                        agents_states: agents_states
                            .into_iter()
                            .map(|(thread_id, state)| {
                                (
                                    thread_id,
                                    exec_events::CollabAgentState {
                                        status: convert_collab_agent_status(state.status),
                                        message: state.message,
                                    },
                                )
                            })
                            .collect(),
                        status: convert_collab_tool_call_status(status),
                    },
                ),
            }
        }
        _ => return None,
    };
    Some(converted)
}

fn convert_command_execution_status(
    status: codex_app_server_protocol::CommandExecutionStatus,
) -> exec_events::CommandExecutionStatus {
    match status {
        codex_app_server_protocol::CommandExecutionStatus::InProgress => {
            exec_events::CommandExecutionStatus::InProgress
        }
        codex_app_server_protocol::CommandExecutionStatus::Completed => {
            exec_events::CommandExecutionStatus::Completed
        }
        codex_app_server_protocol::CommandExecutionStatus::Failed => {
            exec_events::CommandExecutionStatus::Failed
        }
        codex_app_server_protocol::CommandExecutionStatus::Declined => {
            exec_events::CommandExecutionStatus::Declined
        }
    }
}

fn convert_patch_apply_status(
    status: codex_app_server_protocol::PatchApplyStatus,
) -> exec_events::PatchApplyStatus {
    match status {
        codex_app_server_protocol::PatchApplyStatus::InProgress => {
            exec_events::PatchApplyStatus::InProgress
        }
        codex_app_server_protocol::PatchApplyStatus::Completed => {
            exec_events::PatchApplyStatus::Completed
        }
        codex_app_server_protocol::PatchApplyStatus::Failed
        | codex_app_server_protocol::PatchApplyStatus::Declined => {
            exec_events::PatchApplyStatus::Failed
        }
    }
}

fn convert_patch_change_kind(
    kind: codex_app_server_protocol::PatchChangeKind,
) -> exec_events::PatchChangeKind {
    match kind {
        codex_app_server_protocol::PatchChangeKind::Add => exec_events::PatchChangeKind::Add,
        codex_app_server_protocol::PatchChangeKind::Delete => exec_events::PatchChangeKind::Delete,
        codex_app_server_protocol::PatchChangeKind::Update { .. } => {
            exec_events::PatchChangeKind::Update
        }
    }
}

fn convert_mcp_tool_call_status(
    status: codex_app_server_protocol::McpToolCallStatus,
) -> exec_events::McpToolCallStatus {
    match status {
        codex_app_server_protocol::McpToolCallStatus::InProgress => {
            exec_events::McpToolCallStatus::InProgress
        }
        codex_app_server_protocol::McpToolCallStatus::Completed => {
            exec_events::McpToolCallStatus::Completed
        }
        codex_app_server_protocol::McpToolCallStatus::Failed => {
            exec_events::McpToolCallStatus::Failed
        }
    }
}

fn convert_collab_tool(
    tool: codex_app_server_protocol::CollabAgentTool,
) -> Option<exec_events::CollabTool> {
    let tool = match tool {
        codex_app_server_protocol::CollabAgentTool::SpawnAgent => {
            exec_events::CollabTool::SpawnAgent
        }
        codex_app_server_protocol::CollabAgentTool::SendInput => exec_events::CollabTool::SendInput,
        codex_app_server_protocol::CollabAgentTool::Wait => exec_events::CollabTool::Wait,
        codex_app_server_protocol::CollabAgentTool::CloseAgent => {
            exec_events::CollabTool::CloseAgent
        }
        codex_app_server_protocol::CollabAgentTool::ResumeAgent => return None,
    };
    Some(tool)
}

fn convert_collab_tool_call_status(
    status: codex_app_server_protocol::CollabAgentToolCallStatus,
) -> exec_events::CollabToolCallStatus {
    match status {
        codex_app_server_protocol::CollabAgentToolCallStatus::InProgress => {
            exec_events::CollabToolCallStatus::InProgress
        }
        codex_app_server_protocol::CollabAgentToolCallStatus::Completed => {
            exec_events::CollabToolCallStatus::Completed
        }
        codex_app_server_protocol::CollabAgentToolCallStatus::Failed => {
            exec_events::CollabToolCallStatus::Failed
        }
    }
}

fn convert_collab_agent_status(
    status: codex_app_server_protocol::CollabAgentStatus,
) -> exec_events::CollabAgentStatus {
    match status {
        codex_app_server_protocol::CollabAgentStatus::PendingInit => {
            exec_events::CollabAgentStatus::PendingInit
        }
        codex_app_server_protocol::CollabAgentStatus::Running => {
            exec_events::CollabAgentStatus::Running
        }
        codex_app_server_protocol::CollabAgentStatus::Interrupted => {
            exec_events::CollabAgentStatus::Interrupted
        }
        codex_app_server_protocol::CollabAgentStatus::Completed => {
            exec_events::CollabAgentStatus::Completed
        }
        codex_app_server_protocol::CollabAgentStatus::Errored => {
            exec_events::CollabAgentStatus::Errored
        }
        codex_app_server_protocol::CollabAgentStatus::Shutdown => {
            exec_events::CollabAgentStatus::Shutdown
        }
        codex_app_server_protocol::CollabAgentStatus::NotFound => {
            exec_events::CollabAgentStatus::NotFound
        }
    }
}

fn format_patch_change_kind(kind: &exec_events::PatchChangeKind) -> &'static str {
    match kind {
        exec_events::PatchChangeKind::Add => "A",
        exec_events::PatchChangeKind::Delete => "D",
        exec_events::PatchChangeKind::Update => "M",
    }
}

fn format_collab_tool_name(tool: &exec_events::CollabTool) -> &'static str {
    match tool {
        exec_events::CollabTool::SpawnAgent => "spawn_agent",
        exec_events::CollabTool::SendInput => "send_input",
        exec_events::CollabTool::Wait => "wait_agent",
        exec_events::CollabTool::CloseAgent => "close_agent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::ThreadStartedNotification;
    use codex_protocol::ThreadId;
    use codex_protocol::config_types::ApprovalsReviewer;
    use codex_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
    use codex_protocol::protocol::AskForApproval;
    use codex_protocol::protocol::SandboxPolicy;
    use pretty_assertions::assert_eq;
    use std::time::Duration;
    use tempfile::TempDir;

    fn event(message: EventMsg) -> Event {
        Event {
            id: String::new(),
            msg: message,
        }
    }

    fn session_configured(thread_id: ThreadId) -> Event {
        event(EventMsg::SessionConfigured(SessionConfiguredEvent {
            session_id: thread_id,
            forked_from_id: None,
            thread_name: None,
            model: "gpt-5.2-codex".to_string(),
            model_provider_id: "openai".to_string(),
            service_tier: None,
            approval_policy: AskForApproval::Never,
            approvals_reviewer: ApprovalsReviewer::User,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            cwd: PathBuf::from("/tmp"),
            reasoning_effort: None,
            history_log_id: 0,
            history_entry_count: 0,
            initial_messages: None,
            network_proxy: None,
            rollout_path: None,
        }))
    }

    #[test]
    fn json_sidecar_uses_nickname_file_name_when_parent_reports_it() {
        let tempdir = TempDir::new().expect("create tempdir");
        let primary = ThreadId::new().to_string();
        let child = ThreadId::new().to_string();
        let child_id = ThreadId::from_string(&child).expect("valid thread id");
        let mut registry = SubagentSidecarRegistry::new(
            primary.clone(),
            tempdir.path().to_path_buf(),
            /*json_mode*/ true,
        )
        .expect("create registry");

        registry
            .observe_event(Some(child.as_str()), &session_configured(child_id))
            .expect("buffer child event");
        registry
            .observe_event(
                Some(primary.as_str()),
                &event(EventMsg::CollabAgentSpawnEnd(CollabAgentSpawnEndEvent {
                    call_id: "spawn-1".to_string(),
                    sender_thread_id: ThreadId::from_string(&primary).expect("valid thread id"),
                    new_thread_id: Some(child_id),
                    new_agent_nickname: Some("Scout".to_string()),
                    new_agent_role: None,
                    prompt: "child work".to_string(),
                    model: "gpt-5.2-codex".to_string(),
                    reasoning_effort: ReasoningEffortConfig::default(),
                    status: AgentStatus::Running,
                })),
            )
            .expect("resolve nickname");
        registry.finalize().expect("finalize");

        let path = tempdir.path().join(format!("subagent-scout-{child}.jsonl"));
        let contents = std::fs::read_to_string(&path).expect("read sidecar");
        assert!(contents.contains("\"type\":\"thread.started\""));
        assert!(contents.contains(&child));
    }

    #[test]
    fn json_sidecar_falls_back_to_thread_id_when_nickname_missing() {
        let tempdir = TempDir::new().expect("create tempdir");
        let primary = ThreadId::new().to_string();
        let child = ThreadId::new().to_string();
        let child_id = ThreadId::from_string(&child).expect("valid thread id");
        let mut registry = SubagentSidecarRegistry::new(
            primary,
            tempdir.path().to_path_buf(),
            /*json_mode*/ true,
        )
        .expect("create registry");

        registry
            .observe_event(Some(child.as_str()), &session_configured(child_id))
            .expect("buffer child event");
        registry.finalize().expect("finalize");

        let path = tempdir.path().join(format!("subagent-{child}.jsonl"));
        let contents = std::fs::read_to_string(path).expect("read sidecar");
        assert!(contents.contains("\"type\":\"thread.started\""));
    }

    #[test]
    fn human_sidecar_is_best_effort_and_ansi_free() {
        let tempdir = TempDir::new().expect("create tempdir");
        let primary = ThreadId::new().to_string();
        let child = ThreadId::new().to_string();
        let child_id = ThreadId::from_string(&child).expect("valid thread id");
        let mut registry = SubagentSidecarRegistry::new(
            primary,
            tempdir.path().to_path_buf(),
            /*json_mode*/ false,
        )
        .expect("create registry");

        registry
            .observe_server_notification(&ServerNotification::ThreadStarted(
                ThreadStartedNotification {
                    thread: codex_app_server_protocol::Thread {
                        id: child.clone(),
                        preview: String::new(),
                        ephemeral: true,
                        model_provider: "openai".to_string(),
                        created_at: 0,
                        updated_at: 0,
                        status: codex_app_server_protocol::ThreadStatus::Idle,
                        path: None,
                        cwd: PathBuf::from("/tmp"),
                        cli_version: "test".to_string(),
                        source: codex_app_server_protocol::SessionSource::Exec,
                        agent_nickname: Some("Planner Bot".to_string()),
                        agent_role: None,
                        git_info: None,
                        name: None,
                        turns: Vec::new(),
                    },
                },
            ))
            .expect("seed nickname");
        registry
            .observe_event(Some(child.as_str()), &session_configured(child_id))
            .expect("write session configured");
        registry
            .observe_event(
                Some(child.as_str()),
                &event(EventMsg::AgentMessage(AgentMessageEvent {
                    message: "child done".to_string(),
                    phase: None,
                    memory_citation: None,
                })),
            )
            .expect("write agent message");
        registry
            .observe_event(
                Some(child.as_str()),
                &event(EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                    call_id: "call-1".to_string(),
                    command: vec![
                        "python3".to_string(),
                        "-c".to_string(),
                        "print(2)".to_string(),
                    ],
                    cwd: PathBuf::from("/tmp"),
                    parsed_cmd: Vec::new(),
                    process_id: None,
                    turn_id: "turn-1".to_string(),
                    source: codex_protocol::protocol::ExecCommandSource::Agent,
                    interaction_input: None,
                })),
            )
            .expect("write exec begin");
        registry
            .observe_event(
                Some(child.as_str()),
                &event(EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                    call_id: "call-1".to_string(),
                    command: vec![
                        "python3".to_string(),
                        "-c".to_string(),
                        "print(2)".to_string(),
                    ],
                    cwd: PathBuf::from("/tmp"),
                    parsed_cmd: Vec::new(),
                    process_id: None,
                    aggregated_output: "2".to_string(),
                    exit_code: 0,
                    duration: Duration::from_millis(10),
                    formatted_output: "2".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    turn_id: "turn-1".to_string(),
                    source: codex_protocol::protocol::ExecCommandSource::Agent,
                    interaction_input: None,
                    status: codex_protocol::protocol::ExecCommandStatus::Completed,
                })),
            )
            .expect("write exec end");
        registry.finalize().expect("finalize");

        let path = tempdir.path().join(format!("subagent-planner-bot-{child}"));
        let contents = std::fs::read_to_string(path).expect("read human sidecar");
        assert!(contents.contains("OpenAI Codex v"));
        assert!(contents.contains("codex\nchild done"));
        assert!(contents.contains("succeeded in"));
        assert!(!contents.contains('\u{1b}'));
    }

    #[test]
    fn sanitize_file_component_collapses_non_alphanumeric_runs() {
        assert_eq!(sanitize_file_component("Planner Bot!!"), "planner-bot");
        assert_eq!(sanitize_file_component("___"), "agent");
    }
}

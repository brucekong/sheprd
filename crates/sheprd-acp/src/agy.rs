//! Antigravity (`agy acp`) with local `/usage` support.
//!
//! `agy acp` speaks ACP on its standard input and output, but echoes prompts
//! back as `user_message_chunk` updates during `session/prompt`, omits the
//! TUI's `/usage` command from `available_commands_update`, and forwards
//! `/usage` to the model if sent as a prompt. This bridge drops live prompt
//! echoes outside `session/load`, adds `/usage` to the session's command list,
//! and answers `/usage` (and its `/quota` alias) locally from
//! `agy -p /usage --output-format json`.

use std::{
    ffi::{OsStr, OsString},
    io::Read,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use acp::v1 as wire;
use crossbeam_channel::{Receiver, RecvTimeoutError, TryRecvError, bounded};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    harness::{Out, Reach, Translated, Translator},
    rpc::Inbound,
};

const MAX_USAGE_BYTES: usize = 1024 * 1024;
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const USAGE_TIMEOUT: Duration = Duration::from_secs(35);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const FAST_WAIT: Duration = Duration::from_millis(10);
/// Earlier builds could send `/usage` to the model as a prompt.
const MINIMUM_VERSION: (u64, u64, u64) = (1, 1, 11);

#[derive(Debug, thiserror::Error)]
pub(crate) enum UsageError {
    #[error("could not run agy for usage: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("agy 1.1.11 or later is needed for usage reports")]
    UnsupportedVersion,
    #[error("agy timed out while reading model quota usage")]
    TimedOut,
    #[error("agy could not report model quota usage; run `agy` to sign in")]
    CommandFailed,
    #[error("agy returned an invalid usage report: {0}")]
    InvalidReport(#[from] serde_json::Error),
    #[error("agy reported no model quota windows")]
    EmptyReport,
}

/// How the session reaches `agy acp`.
pub(crate) fn reach(program: OsString) -> Box<dyn Reach> {
    Box::new(Translated(Bridge::new(move |cancel| {
        fetch_usage(&program, cancel)
    })))
}

struct PendingUsage {
    id: wire::RequestId,
    session_id: String,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Result<String, UsageError>>,
    next_poll: Instant,
}

pub(crate) struct Bridge<F> {
    fetch: Arc<F>,
    loading: Option<wire::RequestId>,
    pending_usage: Option<PendingUsage>,
}

impl<F> Bridge<F>
where
    F: Fn(&AtomicBool) -> Result<String, UsageError> + Send + Sync + 'static,
{
    pub(crate) fn new(fetch: F) -> Self {
        Self {
            fetch: Arc::new(fetch),
            loading: None,
            pending_usage: None,
        }
    }

    fn start_usage(&mut self, id: wire::RequestId, session_id: String) -> Vec<Out> {
        if let Some(prev) = self.pending_usage.take() {
            prev.cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let fetch = Arc::clone(&self.fetch);
        let (tx, rx) = bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send(fetch(&worker_cancel));
        });
        match rx.recv_timeout(FAST_WAIT) {
            Ok(result) => finish_usage(id, &session_id, result),
            Err(RecvTimeoutError::Timeout) => {
                self.pending_usage = Some(PendingUsage {
                    id,
                    session_id,
                    cancel,
                    rx,
                    next_poll: Instant::now() + POLL_INTERVAL,
                });
                Vec::new()
            }
            Err(RecvTimeoutError::Disconnected) => {
                finish_usage(id, &session_id, Err(UsageError::CommandFailed))
            }
        }
    }
}

fn finish_usage(
    id: wire::RequestId,
    session_id: &str,
    result: Result<String, UsageError>,
) -> Vec<Out> {
    let update = match result.and_then(|body| format_report(&body)) {
        Ok(markdown) => wire::SessionUpdate::AgentMessageChunk(wire::ContentChunk::new(
            wire::ContentBlock::Text(wire::TextContent::new(markdown)),
        )),
        Err(error) => wire::SessionUpdate::Notice(wire::Notice::new(
            wire::NoticeSeverity::Warning,
            error.to_string(),
        )),
    };
    let notification = wire::SessionNotification::new(session_id.to_owned(), update);
    let mut outs = Vec::with_capacity(2);
    if let Ok(params) = serde_json::to_value(notification) {
        outs.push(Out::Session(Inbound::Notification {
            method: "session/update".to_owned(),
            params,
        }));
    }
    let stop = wire::PromptResponse::new(wire::StopReason::EndTurn);
    if let Ok(result) = serde_json::to_value(stop) {
        outs.push(Out::Session(Inbound::Response {
            id,
            result: Ok(result),
        }));
    }
    outs
}

impl<F> Translator for Bridge<F>
where
    F: Fn(&AtomicBool) -> Result<String, UsageError> + Send + Sync + 'static,
{
    fn next_tick(&self) -> Option<Instant> {
        self.pending_usage.as_ref().map(|pending| pending.next_poll)
    }

    fn tick(&mut self) -> Vec<Out> {
        let Some(pending) = self.pending_usage.as_mut() else {
            return Vec::new();
        };
        match pending.rx.try_recv() {
            Ok(result) => {
                let Some(done) = self.pending_usage.take() else {
                    return Vec::new();
                };
                finish_usage(done.id, &done.session_id, result)
            }
            Err(TryRecvError::Empty) => {
                pending.next_poll = Instant::now() + POLL_INTERVAL;
                Vec::new()
            }
            Err(TryRecvError::Disconnected) => {
                let Some(done) = self.pending_usage.take() else {
                    return Vec::new();
                };
                finish_usage(done.id, &done.session_id, Err(UsageError::CommandFailed))
            }
        }
    }

    fn start(&mut self) -> Vec<Out> {
        Vec::new()
    }

    fn on_session(&mut self, inbound: Inbound) -> Vec<Out> {
        match inbound {
            Inbound::Request { id, method, params } => {
                if method == "session/load" {
                    self.loading = Some(id.clone());
                }
                if method == "session/prompt"
                    && let Some(session_id) = usage_prompt_session(&params)
                {
                    return self.start_usage(id, session_id);
                }
                vec![Out::Harness(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": method,
                    "params": params,
                }))]
            }
            Inbound::Notification { method, params } => {
                if method == "session/cancel"
                    && let Some(pending) = self.pending_usage.take()
                {
                    pending.cancel.store(true, Ordering::Relaxed);
                    let stop = wire::PromptResponse::new(wire::StopReason::Cancelled);
                    return serde_json::to_value(stop)
                        .map(|result| {
                            vec![Out::Session(Inbound::Response {
                                id: pending.id,
                                result: Ok(result),
                            })]
                        })
                        .unwrap_or_default();
                }
                vec![Out::Harness(json!({
                    "jsonrpc": "2.0",
                    "method": method,
                    "params": params,
                }))]
            }
            Inbound::Response { id, result } => {
                let message = match result {
                    Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                    Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
                };
                vec![Out::Harness(message)]
            }
            Inbound::Closed(_) => {
                if let Some(pending) = self.pending_usage.take() {
                    pending.cancel.store(true, Ordering::Relaxed);
                }
                Vec::new()
            }
        }
    }

    fn on_harness(&mut self, message: &Value) -> Vec<Out> {
        if self.loading.is_none() && is_user_message_chunk(message) {
            return Vec::new();
        }
        if self.loading.is_some() && response_id(message).as_ref() == self.loading.as_ref() {
            self.loading = None;
        }
        let mut message = message.clone();
        inject_usage_command(&mut message);
        let mut outs = Vec::new();
        let opened_session = opened_session_id(&message);
        if let Ok(bytes) = serde_json::to_vec(&message)
            && let Some(inbound) = crate::rpc::classify(&bytes)
        {
            outs.push(Out::Session(inbound));
        }
        if let Some(session_id) = opened_session {
            outs.push(Out::Session(Inbound::Notification {
                method: "session/update".to_owned(),
                params: json!({
                    "sessionId": session_id,
                    "update": {
                        "sessionUpdate": "available_commands_update",
                        "availableCommands": [usage_command_json()],
                    }
                }),
            }));
        }
        outs
    }
}

fn is_user_message_chunk(message: &Value) -> bool {
    message.get("method").and_then(Value::as_str) == Some("session/update")
        && message
            .pointer("/params/update/sessionUpdate")
            .and_then(Value::as_str)
            == Some("user_message_chunk")
}

fn response_id(message: &Value) -> Option<wire::RequestId> {
    if message.get("method").is_some() {
        return None;
    }
    serde_json::from_value(message.get("id")?.clone()).ok()
}

fn usage_command_json() -> Value {
    json!({
        "name": "usage",
        "description": "View model quota usage",
    })
}

fn opened_session_id(message: &Value) -> Option<String> {
    message
        .pointer("/result/sessionId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

fn inject_usage_command(message: &mut Value) {
    let is_commands = message.get("method").and_then(Value::as_str) == Some("session/update")
        && message
            .pointer("/params/update/sessionUpdate")
            .and_then(Value::as_str)
            == Some("available_commands_update");
    if !is_commands {
        return;
    }
    let Some(commands) = message
        .pointer_mut("/params/update/availableCommands")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    let has_usage = commands
        .iter()
        .any(|cmd| cmd.get("name").and_then(Value::as_str) == Some("usage"));
    if !has_usage {
        commands.insert(0, usage_command_json());
    }
}

/// Returns the `sessionId` when a `session/prompt` request is solely `/usage`
/// or `/quota`.
fn usage_prompt_session(params: &Value) -> Option<String> {
    let session_id = params.get("sessionId").and_then(Value::as_str)?;
    let blocks = params.get("prompt").and_then(Value::as_array)?;
    if blocks.len() != 1 {
        return None;
    }
    let block = blocks.first()?;
    if block.get("type").and_then(Value::as_str) != Some("text") {
        return None;
    }
    let text = block.get("text").and_then(Value::as_str)?.trim();
    matches!(text, "/usage" | "/quota").then(|| session_id.to_owned())
}

fn fetch_usage(program: &OsStr, cancel: &AtomicBool) -> Result<String, UsageError> {
    check_version(program, cancel)?;
    let temp_dir = std::env::temp_dir().join(format!("sheprd-agy-usage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)?;
    let result = run_command_in(
        program,
        &[
            "-p",
            "/usage",
            "--output-format",
            "json",
            "--print-timeout",
            "30s",
        ],
        Some(&temp_dir),
        USAGE_TIMEOUT,
        cancel,
    );
    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

fn check_version(program: &OsStr, cancel: &AtomicBool) -> Result<(), UsageError> {
    let stdout = run_command_in(program, &["--version"], None, VERSION_TIMEOUT, cancel)?;
    if parse_version(&stdout).is_none_or(|version| version < MINIMUM_VERSION) {
        return Err(UsageError::UnsupportedVersion);
    }
    Ok(())
}

/// Parses a plain `major.minor.patch` version token from `agy --version`.
pub(crate) fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim();
    let text = text
        .rsplit(' ')
        .next()
        .unwrap_or(text)
        .trim_start_matches('v');
    let mut parts = text.split('.').map(|part| part.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

fn run_command_in(
    program: &OsStr,
    args: &[&str],
    cwd: Option<&std::path::Path>,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<String, UsageError> {
    let mut command = sheprd_client::new_command(program);
    command
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let mut child = command.spawn()?;
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(out) = &mut stdout {
            let _ = out.take(MAX_USAGE_BYTES as u64).read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(UsageError::TimedOut);
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    let bytes = reader.join().unwrap_or_default();
    if !status.success() {
        return Err(UsageError::CommandFailed);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[derive(Deserialize)]
struct CliReport {
    status: String,
    command: ReportCommand,
}

#[derive(Deserialize)]
struct ReportCommand {
    name: String,
    data: ReportSummary,
}

#[derive(Deserialize)]
struct ReportSummary {
    groups: Option<Vec<ReportGroup>>,
}

#[derive(Deserialize)]
struct ReportGroup {
    #[serde(alias = "displayName")]
    display_name: Option<String>,
    name: Option<String>,
    buckets: Option<Vec<ReportBucket>>,
}

#[derive(Deserialize)]
struct ReportBucket {
    #[serde(alias = "displayName")]
    display_name: Option<String>,
    name: Option<String>,
    disabled: Option<bool>,
    #[serde(alias = "remainingFraction")]
    remaining_fraction: Option<f64>,
    remaining: Option<RemainingValue>,
    #[serde(alias = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Deserialize)]
struct RemainingValue {
    #[serde(alias = "remaining_fraction", rename = "remainingFraction")]
    remaining_fraction: Option<f64>,
    case: Option<String>,
    value: Option<f64>,
}

impl RemainingValue {
    fn fraction(&self) -> Option<f64> {
        self.remaining_fraction.or_else(|| {
            (self.case.as_deref() == Some("remainingFraction"))
                .then_some(self.value)
                .flatten()
        })
    }
}

pub(crate) fn format_report(body: &str) -> Result<String, UsageError> {
    let report: CliReport = serde_json::from_str(body.trim())?;
    if report.status != "SUCCESS" || report.command.name != "usage" {
        return Err(UsageError::CommandFailed);
    }
    let mut lines = Vec::new();
    for group in report.command.data.groups.unwrap_or_default() {
        let group_name = group
            .display_name
            .or(group.name)
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Quota".to_owned());
        let mut group_lines = Vec::new();
        for bucket in group.buckets.unwrap_or_default() {
            if bucket.disabled == Some(true) {
                continue;
            }
            let Some(fraction) = bucket
                .remaining_fraction
                .or_else(|| bucket.remaining.as_ref().and_then(RemainingValue::fraction))
                .filter(|fraction| fraction.is_finite())
            else {
                continue;
            };
            let percent = (fraction.clamp(0.0, 1.0) * 100.0).round() as u32;
            let bucket_name = bucket
                .display_name
                .or(bucket.name)
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "Limit".to_owned());
            let reset = bucket
                .reset_time
                .as_deref()
                .map(str::trim)
                .filter(|reset| !reset.is_empty());
            match reset {
                Some(reset) => group_lines.push(format!(
                    "- **{bucket_name}:** `{percent}%` remaining (resets `{reset}`)"
                )),
                None => group_lines.push(format!("- **{bucket_name}:** `{percent}%` remaining")),
            }
        }
        if !group_lines.is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("**{group_name}**"));
            lines.extend(group_lines);
        }
    }
    if lines.is_empty() {
        return Err(UsageError::EmptyReport);
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests;

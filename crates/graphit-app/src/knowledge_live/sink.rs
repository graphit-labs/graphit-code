//! Typed ACP notifications become Live events before the native process exits.
use super::{Access, EventData, service::LiveSessionService};
use agent_client_protocol::schema::v1::{
    ContentBlock, SessionNotification, SessionUpdate, ToolCall, ToolCallStatus,
};
use graphit_ai::cli_kiro::{KiroDeliveryPhase, KiroSink};
use graphit_core::file_io::SnapshotContext;
use serde_json::Value;
use std::{collections::BTreeMap, io};
use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextKind {
    Answer,
    Thinking,
}
struct ToolState {
    call: ToolCall,
    output: Option<Value>,
}

pub(super) struct LiveSink<'a> {
    service: &'a LiveSessionService,
    access: &'a Access<'a>,
    session: &'a str,
    turn: &'a str,
    allowed: &'a (dyn Fn() -> bool + Sync),
    secrets: Vec<Zeroizing<String>>,
    pending: Option<(TextKind, String)>,
    tools: BTreeMap<String, ToolState>,
}
impl<'a> LiveSink<'a> {
    pub fn new(
        service: &'a LiveSessionService,
        access: &'a Access<'a>,
        session: &'a str,
        turn: &'a str,
        allowed: &'a (dyn Fn() -> bool + Sync),
        secrets: &[String],
    ) -> Self {
        Self {
            service,
            access,
            session,
            turn,
            allowed,
            secrets: secrets
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| Zeroizing::new(s.clone()))
                .collect(),
            pending: None,
            tools: BTreeMap::new(),
        }
    }
    fn check(&self, original: &SnapshotContext) -> io::Result<()> {
        if !self.access.original().snapshot().same_request(original) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.access.write()?;
        if !(self.allowed)() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        original.check()
    }
    fn emit(&self, data: EventData) -> io::Result<()> {
        self.service
            .emit(self.access, self.session, Some(self.turn), data)
            .map(|_| ())
            .map_err(|e| io::Error::new(e.kind(), e))
    }
    fn redact(&self, mut value: String) -> String {
        for secret in &self.secrets {
            value = value.replace(secret.as_str(), "[redacted]");
        }
        value
    }
    fn public_value(&self, value: Value) -> io::Result<Value> {
        match value {
            Value::String(s) => Ok(Value::String(self.redact(s))),
            Value::Array(values) => values
                .into_iter()
                .map(|v| self.public_value(v))
                .collect::<io::Result<Vec<_>>>()
                .map(Value::Array),
            Value::Object(values) => {
                let mut result = serde_json::Map::new();
                for (key, value) in values {
                    if key == "_meta" {
                        continue;
                    }
                    let key = self.redact(key);
                    if result.insert(key, self.public_value(value)?).is_some() {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                }
                Ok(Value::Object(result))
            }
            other => Ok(other),
        }
    }
    fn text_event(&self, kind: TextKind, text: String) -> io::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        self.emit(match kind {
            TextKind::Answer => EventData::Text { text },
            TextKind::Thinking => EventData::Thinking { text },
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        if let Some((kind, pending)) = self.pending.take()
            && !pending.is_empty()
        {
            // A proper secret prefix is intentionally withheld across chunk boundaries.
            self.text_event(kind, "[redacted]".into())?;
        }
        Ok(())
    }
    fn text(&mut self, kind: TextKind, chunk: &str) -> io::Result<()> {
        if self
            .pending
            .as_ref()
            .is_some_and(|(previous, _)| *previous != kind)
        {
            self.flush()?;
        }
        let mut text = self
            .pending
            .take()
            .map_or_else(String::new, |(_, text)| text);
        text.try_reserve(chunk.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        text.push_str(chunk);
        let text = self.redact(text);
        let mut withheld = 0;
        for secret in &self.secrets {
            for (length, _) in secret.char_indices().skip(1) {
                if length > withheld && text.ends_with(&secret[..length]) {
                    withheld = length;
                }
            }
        }
        let split = text.len() - withheld;
        let pending = text[split..].to_owned();
        self.text_event(kind, text[..split].to_owned())?;
        self.pending = Some((kind, pending));
        Ok(())
    }
    fn tool(&mut self, id: String, call: ToolCall) -> io::Result<()> {
        let terminal = matches!(
            call.status,
            ToolCallStatus::Completed | ToolCallStatus::Failed
        );
        let output = if terminal {
            Some(self.public_value(match &call.raw_output {
                Some(value) => value.clone(),
                None => serde_json::to_value(&call.content).map_err(io::Error::other)?,
            })?)
        } else {
            None
        };
        if let Some(previous) = self.tools.get(&id)
            && previous.output.is_some()
        {
            if !terminal || previous.call.status != call.status || previous.output != output {
                return Err(io::ErrorKind::InvalidData.into());
            }
            return Ok(());
        }
        let previous = self.tools.get(&id).and_then(|s| s.output.as_ref());
        if let Some(output) = &output {
            if let Some(previous) = previous {
                if previous != output {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            } else {
                self.emit(EventData::ToolResult {
                    tool_call_id: self.redact(id.clone()),
                    output: output.clone(),
                    is_error: call.status == ToolCallStatus::Failed,
                })?;
            }
        }
        self.tools.insert(id, ToolState { call, output });
        Ok(())
    }
    pub fn finish(&mut self) -> io::Result<()> {
        self.access.write()?;
        if !(self.allowed)() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.flush()
    }
}
impl KiroSink for LiveSink<'_> {
    async fn push<'a>(
        &'a mut self,
        phase: KiroDeliveryPhase,
        event: &'a SessionNotification,
        original: &'a SnapshotContext,
    ) -> io::Result<()> {
        self.check(original)?;
        let private_id = event.session_id.to_string();
        if !private_id.is_empty() && !self.secrets.iter().any(|s| s.as_str() == private_id) {
            self.secrets.push(Zeroizing::new(private_id));
        }
        // Loading replays the native transcript; Live already owns its persisted public history.
        if phase != KiroDeliveryPhase::Prompting {
            return self.check(original);
        }
        match &event.update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let ContentBlock::Text(text) = &chunk.content {
                    self.text(TextKind::Answer, &text.text)?;
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                if let ContentBlock::Text(text) = &chunk.content {
                    self.text(TextKind::Thinking, &text.text)?;
                }
            }
            SessionUpdate::ToolCall(call) => {
                self.flush()?;
                let id = call.tool_call_id.to_string();
                let public_id = self.redact(id.clone());
                if self
                    .tools
                    .keys()
                    .any(|existing| self.redact(existing.clone()) == public_id)
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                self.emit(EventData::ToolUse {
                    tool_call_id: self.redact(id.clone()),
                    name: call.name.clone().map(|name| self.redact(name)),
                    title: self.redact(call.title.clone()),
                    input: self.public_value(call.raw_input.clone().unwrap_or(Value::Null))?,
                })?;
                self.tool(id, call.clone())?;
            }
            SessionUpdate::ToolCallUpdate(update) => {
                self.flush()?;
                let id = update.tool_call_id.to_string();
                let mut call = self
                    .tools
                    .get(&id)
                    .ok_or(io::ErrorKind::InvalidData)?
                    .call
                    .clone();
                call.update(update.fields.clone());
                self.tool(id, call)?;
            }
            _ => {}
        }
        self.check(original)
    }
}

//! Subscriber delivery follows the persisted journal; it never owns the engine sink.
use super::{
    Access, Event, KnownTerminal, ReplayCursor, Session, SessionState,
    service::{LiveError, LiveSessionService},
};
use serde::Serialize;
use std::io;
use tokio::sync::{mpsc, oneshot};

#[derive(Serialize)]
pub(crate) struct TailOpening {
    pub session: Session,
    /// Requested filter and observed head are distinct, including a future cursor.
    pub after: ReplayCursor,
    pub terminal: Option<KnownTerminal>,
}
pub(crate) enum TailMessage {
    Event(Event),
    /// An observed native outcome whose journal write may have failed. No journal sequence is invented.
    Terminal(KnownTerminal),
}
pub(crate) struct TailDelivery {
    pub opened: oneshot::Sender<TailOpening>,
    pub events: mpsc::Sender<TailMessage>,
}
impl LiveSessionService {
    pub async fn forward_events(
        &self,
        access: &Access<'_>,
        id: &str,
        cursor: Option<ReplayCursor>,
        delivery: TailDelivery,
        allowed: &(dyn Fn() -> bool + Sync),
    ) -> Result<Session, LiveError> {
        let mut observation = self.observe(access, id, cursor.as_ref())?;
        let mut after = cursor.unwrap_or_else(|| ReplayCursor {
            generation: observation.replay.session.cursor.generation.clone(),
            sequence: 0,
        });
        let mut known = observation.terminal.clone();
        delivery
            .opened
            .send(TailOpening {
                session: observation.replay.session.clone(),
                after: after.clone(),
                terminal: known.clone(),
            })
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        loop {
            access.read()?;
            if !allowed() {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            for event in observation.replay.events {
                let sequence = event.sequence;
                self.deliver(access, &delivery.events, TailMessage::Event(event), allowed)
                    .await?;
                after.sequence = sequence;
            }
            if observation.terminal != known {
                if let Some(terminal) = observation.terminal.clone() {
                    self.deliver(
                        access,
                        &delivery.events,
                        TailMessage::Terminal(terminal),
                        allowed,
                    )
                    .await?;
                }
                known = observation.terminal;
            }
            if observation.replay.session.state == SessionState::Closed {
                return Ok(observation.replay.session);
            }
            tokio::select! {
                result = observation.changed.changed() => result.map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?,
                () = delivery.events.closed() => return Err(io::ErrorKind::BrokenPipe.into()),
                () = access.original().cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                () = self.stop.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                () = deadline(access) => return Err(io::ErrorKind::TimedOut.into()),
            }
            observation = self.observe(access, id, Some(&after))?;
        }
    }
    async fn deliver(
        &self,
        access: &Access<'_>,
        channel: &mpsc::Sender<TailMessage>,
        message: TailMessage,
        allowed: &(dyn Fn() -> bool + Sync),
    ) -> Result<(), LiveError> {
        // Backpressure holds only this subscriber; execution persists through a separate sink.
        let permit = tokio::select! {
            permit = channel.reserve() => permit.map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?,
            () = access.original().cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = self.stop.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = deadline(access) => return Err(io::ErrorKind::TimedOut.into()),
        };
        access.read()?;
        if !allowed() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        permit.send(message);
        Ok(())
    }
}
async fn deadline(access: &Access<'_>) {
    match access.original().deadline() {
        Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
        None => std::future::pending::<()>().await,
    }
}

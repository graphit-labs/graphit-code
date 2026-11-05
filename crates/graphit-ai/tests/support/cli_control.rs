//! Test-only native CLI control sockets, bounded by the caller's Original.
use graphit_core::file_io::SnapshotContext;
use std::{future::Future, io, net::TcpListener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn bounded<T>(
    original: &SnapshotContext,
    operation: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
    let deadline = original.required_deadline()?;
    let result = tokio::select! {
        biased;
        () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
        result = operation => result,
    };
    original.check()?;
    result
}

pub async fn peer(listener: &TcpListener, original: &SnapshotContext) -> io::Result<TcpStream> {
    let listener = tokio::net::TcpListener::from_std(listener.try_clone()?)?;
    let (stream, _) = bounded(original, listener.accept()).await?;
    Ok(stream)
}

pub async fn byte(peer: &mut TcpStream, original: &SnapshotContext) -> io::Result<Option<u8>> {
    let mut byte = [0];
    match bounded(original, peer.read(&mut byte)).await? {
        0 => Ok(None),
        _ => Ok(Some(byte[0])),
    }
}

pub async fn acknowledge(peer: &mut TcpStream, original: &SnapshotContext) -> io::Result<()> {
    bounded(original, peer.write_all(b"A")).await
}

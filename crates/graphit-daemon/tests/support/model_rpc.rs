use super::disconnect_api::DisconnectTransport;
use super::model_api::*;
use graphit_core::{
    file_lock::Cancellation,
    tcp_listener::{self, Options, PortSelection},
};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle};
pub fn server(
    service: ModelService,
    secret: Arc<RuntimeSecret>,
    limits: RpcLimits,
) -> (SocketAddr, Cancellation, JoinHandle<ModelResult<()>>) {
    let directory = tempfile::tempdir().unwrap();
    let options = super::child_api::ChildOptions::standard().unwrap();
    let global = Arc::new(
        super::child_api::GlobalModelConfig::new(
            directory.path().to_owned(),
            options.capture,
            options.source,
            options.manifest,
        )
        .unwrap(),
    );
    let connection = Cancellation::default();
    let bound = tcp_listener::reserve(
        Options {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            ports: PortSelection::Ephemeral,
            timeout: Duration::from_secs(1),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let address = bound.address();
    let listener = bound.into_listener();
    listener.set_nonblocking(true).unwrap();
    let listener = TcpListener::from_std(listener).unwrap();
    let server_cancel = connection.clone();
    let task = tokio::spawn(async move {
        let _directory = directory;
        let (stream, _) = tokio::select! {
            biased;
            _ = server_cancel.cancelled() => return Ok(()),
            accepted = listener.accept() => accepted.unwrap(),
        };
        let transport =
            DisconnectTransport::new(server_transport(stream, limits)?, server_cancel.clone());
        serve_connection(transport, service, secret, limits, global, server_cancel).await
    });
    (address, connection, task)
}

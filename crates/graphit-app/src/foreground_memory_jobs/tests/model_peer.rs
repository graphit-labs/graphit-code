//! Deterministic compute boundary for tests of model consumers.
use futures_util::{SinkExt, StreamExt};
use graphit_ai::local_execution::{LocalDeviceRegistration, LocalProvider};
use graphit_daemon::model_service::{
    EmbedOutput, ExpectedModel, ModelRpcRequest, ModelRpcResponse, ModelServiceClient, RpcLimits,
    server_transport,
};
use sha2::{Digest, Sha256};
use std::{io, net::SocketAddr, sync::Arc};
use tarpc::{ClientMessage, Response};
use tokio::{net::TcpListener, task::JoinSet};
use tokio_util::sync::CancellationToken;

use crate::foreground_memory_jobs::{MemoryJobResources, OwnedLocalEmbedding};

pub(crate) struct ModelPeer {
    pub(crate) address: SocketAddr,
    stop: CancellationToken,
    task: Option<tokio::task::JoinHandle<io::Result<()>>>,
}

impl ModelPeer {
    pub(crate) async fn bind(resources: &mut MemoryJobResources, expected: ExpectedModel) -> Self {
        let expected = Arc::new(expected);
        let model = Self::start(Arc::clone(&expected), resources.rpc).await;
        resources.local = Some(OwnedLocalEmbedding {
            client: Arc::new(ModelServiceClient::new(resources.rpc).unwrap()),
            expected,
            address: model.address,
        });
        model
    }

    async fn start(expected: Arc<ExpectedModel>, limits: RpcLimits) -> Self {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let stop = CancellationToken::new();
        let cancellation = stop.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    () = cancellation.cancelled() => break,
                    accepted = listener.accept() => {
                        let (stream, _) = accepted?;
                        let expected = Arc::clone(&expected);
                        let cancellation = cancellation.clone();
                        connections.spawn(async move {
                            let mut transport = server_transport(stream, limits).map_err(io::Error::other)?;
                            loop {
                                let incoming = tokio::select! {
                                    () = cancellation.cancelled() => break,
                                    incoming = transport.next() => incoming,
                                };
                                let Some(incoming) = incoming else { break };
                                let ClientMessage::Request(request) = incoming? else { continue };
                                let ModelRpcRequest::Embed { slot, expected_config_id, texts, .. } = request.message else {
                                    panic!("unexpected operation on the embedding fixture");
                                };
                                assert_eq!(slot, crate::models::DEFAULT_SLOT);
                                assert_eq!(expected_config_id, expected.config_id());
                                let dimensions = expected.dimensions().unwrap();
                                let vectors = texts.iter().map(|text| vector(text, dimensions as usize)).collect();
                                let response = Response {
                                    request_id: request.id,
                                    message: Ok(ModelRpcResponse::Embed(Ok(EmbedOutput {
                                        config_id: expected_config_id,
                                        device: LocalDeviceRegistration {
                                            requested_provider: LocalProvider::CPU,
                                            requested_device_id: 0,
                                            provider: LocalProvider::CPU,
                                            device_id: 0,
                                            fallback_reason: None,
                                        },
                                        generation: 1,
                                        model: expected.model().to_owned(),
                                        revision: expected.revision().to_owned(),
                                        dimensions,
                                        vectors,
                                    }))),
                                };
                                tokio::select! {
                                    () = cancellation.cancelled() => break,
                                    sent = transport.send(response) => sent?,
                                }
                            }
                            Ok::<_, io::Error>(())
                        });
                    }
                    result = connections.join_next(), if !connections.is_empty() => {
                        result.expect("active model connection").map_err(io::Error::other)??;
                    }
                }
            }
            drop(listener);
            while let Some(result) = connections.join_next().await {
                result.map_err(io::Error::other)??;
            }
            Ok(())
        });
        Self {
            address,
            stop,
            task: Some(task),
        }
    }

    pub(crate) async fn shutdown(&mut self) -> io::Result<()> {
        self.stop.cancel();
        if let Some(task) = self.task.take() {
            task.await.map_err(io::Error::other)??;
        }
        Ok(())
    }
}

impl Drop for ModelPeer {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

fn vector(text: &str, dimensions: usize) -> Vec<f32> {
    assert!(dimensions >= 2);
    let digest = Sha256::digest(text.as_bytes());
    let mut vector = vec![0.; dimensions];
    vector[0] = 0.8;
    vector[1 + usize::from(digest[0]) % (dimensions - 1)] = 0.6;
    vector
}

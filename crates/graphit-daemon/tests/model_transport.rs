use futures::{SinkExt, StreamExt};
use graphit_core::file_lock::Cancellation;
use graphit_daemon::model_transport::DisconnectTransport;
use std::{io, time::Duration};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

async fn connection() -> io::Result<(TcpStream, TcpStream)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let (client, server) = tokio::join!(TcpStream::connect(address), listener.accept());
    Ok((client?, server?.0))
}

fn framed(stream: TcpStream) -> Framed<TcpStream, LengthDelimitedCodec> {
    LengthDelimitedCodec::builder()
        .max_frame_length(1024)
        .new_framed(stream)
}

#[tokio::test]
async fn tcp_eof_cancels_only_its_connection_and_preserves_frames() -> io::Result<()> {
    timeout(Duration::from_secs(5), async {
        let (a, peer_a) = connection().await?;
        let (b, peer_b) = connection().await?;
        let cancelled_a = Cancellation::default();
        let cancelled_b = Cancellation::default();
        let mut a = DisconnectTransport::new(framed(a), cancelled_a.clone());
        let mut b = DisconnectTransport::new(framed(b), cancelled_b.clone());
        let mut peer_a = framed(peer_a);
        let mut peer_b = framed(peer_b);

        peer_a.send(b"request-a".as_slice().into()).await?;
        assert_eq!(a.next().await.transpose()?.unwrap().as_ref(), b"request-a");
        a.send(b"reply-a".as_slice().into()).await?;
        assert_eq!(
            peer_a.next().await.transpose()?.unwrap().as_ref(),
            b"reply-a"
        );
        peer_a.send(Vec::new().into()).await?;
        assert!(a.next().await.transpose()?.unwrap().is_empty());
        assert!(!cancelled_a.is_cancelled());

        drop(peer_a);
        assert!(a.next().await.is_none());
        assert!(cancelled_a.is_cancelled());
        peer_b.send(b"request-b".as_slice().into()).await?;
        assert_eq!(b.next().await.transpose()?.unwrap().as_ref(), b"request-b");
        b.send(b"reply-b".as_slice().into()).await?;
        assert_eq!(
            peer_b.next().await.transpose()?.unwrap().as_ref(),
            b"reply-b"
        );
        assert!(!cancelled_b.is_cancelled());
        b.close().await?;
        assert!(cancelled_b.is_cancelled());
        Ok(())
    })
    .await
    .expect("loopback transport should complete")
}

#[tokio::test]
async fn sdk_decode_error_and_drop_signal_the_existing_token() -> io::Result<()> {
    timeout(Duration::from_secs(5), async {
        let (client, mut peer) = connection().await?;
        let cancellation = Cancellation::default();
        let mut transport = DisconnectTransport::new(framed(client), cancellation.clone());
        peer.write_all(&1025_u32.to_be_bytes()).await?;
        let error = transport.next().await.unwrap().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "frame size too big");
        assert!(cancellation.is_cancelled());

        let (client, _peer) = connection().await?;
        let cancellation = Cancellation::default();
        let transport = DisconnectTransport::new(framed(client), cancellation.clone());
        assert!(!cancellation.is_cancelled());
        drop(transport);
        assert!(cancellation.is_cancelled());
        Ok(())
    })
    .await
    .expect("loopback transport should complete")
}

#[test]
pub fn native_fixture() {
    use std::{
        io::{Read, Write},
        net::TcpStream,
        time::Duration,
    };
    let Ok(address) = std::env::var("GRAPHIT_LIFECYCLE_FIXTURE") else {
        return;
    };
    let mut peer = TcpStream::connect(address).expect("actual native fixture connects");
    peer.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
    peer.write_all(b"R").unwrap();
    std::io::stdout().write_all(b"actual-first").unwrap();
    std::io::stdout().flush().unwrap();
    let mut ack = [0];
    peer.read_exact(&mut ack)
        .expect("fixture awaits actual parent ACK");
    assert_eq!(&ack, b"A");
    std::io::stdout().write_all(b"actual-second").unwrap();
    std::io::stdout().flush().unwrap();
    // Status7 is reached only after the real parent acknowledged this child.
    std::process::exit(7);
}

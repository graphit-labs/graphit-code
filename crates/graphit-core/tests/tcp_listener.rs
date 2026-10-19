use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream},
    num::NonZeroU16,
    sync::{Arc, Barrier},
    time::Duration,
};

use graphit_core::{
    file_lock::Cancellation,
    tcp_listener::{Failure, Options, PortSelection, reserve},
};

fn options(ports: PortSelection) -> Options {
    Options {
        address: IpAddr::V4(Ipv4Addr::LOCALHOST),
        ports,
        timeout: Duration::from_secs(5),
    }
}

fn range(first: u16, last: u16) -> PortSelection {
    PortSelection::Range {
        first: NonZeroU16::new(first).unwrap(),
        last: NonZeroU16::new(last).unwrap(),
    }
}

fn preferred_fixture() -> (TcpListener, u16) {
    for _ in 0..16 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        if port <= 65435 {
            return (listener, port);
        }
    }
    panic!("could not allocate a fixture below the port range boundary");
}

#[test]
fn ephemeral_reservation_reports_actual_address_and_releases_on_drop() {
    let bound = reserve(options(PortSelection::Ephemeral), &Cancellation::default()).unwrap();
    let address = bound.address();
    assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_ne!(address.port(), 0);
    assert_eq!(bound.attempts(), 1);
    assert_eq!(
        TcpListener::bind(address).unwrap_err().kind(),
        std::io::ErrorKind::AddrInUse
    );
    assert!(!format!("{bound:?}").contains(&address.to_string()));
    drop(bound);
    let rebound = TcpListener::bind(address).expect("owner Drop releases its socket");
    drop(rebound);
}

#[test]
fn transferred_socket_accepts_exact_bytes_without_rebind() {
    let bound = reserve(options(PortSelection::Ephemeral), &Cancellation::default()).unwrap();
    let address = bound.address();
    let listener = bound.into_listener();
    assert_eq!(listener.local_addr().unwrap(), address);
    assert!(TcpListener::bind(address).is_err());
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    listener.set_nonblocking(false).unwrap();
    let mut client = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client.write_all(b"tcp\0\xff\n").unwrap();
    let mut bytes = [0; 6];
    server.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"tcp\0\xff\n");
}

#[test]
fn occupied_preferred_port_advances_and_remains_reserved() {
    let (_occupied, preferred) = preferred_fixture();
    let bound = reserve(
        options(range(preferred, preferred + 99)),
        &Cancellation::default(),
    )
    .unwrap();
    assert!(bound.address().port() > preferred);
    assert!(bound.address().port() <= preferred + 99);
    assert_eq!(
        bound.attempts(),
        u32::from(bound.address().port() - preferred) + 1
    );
    assert!(TcpListener::bind(bound.address()).is_err());
}

#[test]
fn exact_occupied_range_is_exhausted_without_stealing_owner() {
    let (occupied, preferred) = preferred_fixture();
    let error = reserve(
        options(range(preferred, preferred)),
        &Cancellation::default(),
    )
    .unwrap_err();
    assert_eq!(error.failure, Failure::Exhausted);
    assert_eq!(error.attempts, 1);
    assert_eq!(occupied.local_addr().unwrap().port(), preferred);
    drop(occupied);
    let bound = reserve(
        options(range(preferred, preferred)),
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(bound.address().port(), preferred);
    assert_eq!(bound.attempts(), 1);
}

#[test]
fn concurrent_contenders_hold_distinct_live_reservations() {
    let (_occupied, preferred) = preferred_fixture();
    let barrier = Arc::new(Barrier::new(2));
    let contenders: Vec<_> = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                reserve(
                    options(range(preferred, preferred + 99)),
                    &Cancellation::default(),
                )
                .unwrap()
            })
        })
        .collect();
    let bound: Vec<_> = contenders
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_ne!(bound[0].address(), bound[1].address());
    for listener in &bound {
        assert!(listener.address().port() > preferred);
        assert!(listener.address().port() <= preferred + 99);
        assert!(TcpListener::bind(listener.address()).is_err());
    }
}

#[test]
fn invalid_options_and_cancellation_are_typed_and_payload_free() {
    for (request, expected) in [
        (options(range(2, 1)), Failure::InvalidRange),
        (
            Options {
                timeout: Duration::ZERO,
                ..options(PortSelection::Ephemeral)
            },
            Failure::InvalidTimeout,
        ),
        (
            Options {
                timeout: Duration::MAX,
                ..options(PortSelection::Ephemeral)
            },
            Failure::InvalidTimeout,
        ),
    ] {
        let error = reserve(request, &Cancellation::default()).unwrap_err();
        assert_eq!(error.failure, expected);
        assert_eq!(error.attempts, 0);
        assert!(!format!("{request:?} {error:?} {error}").contains("127.0.0.1"));
    }
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let error = reserve(options(PortSelection::Ephemeral), &cancellation).unwrap_err();
    assert_eq!(error.failure, Failure::Cancelled);
    assert_eq!(error.attempts, 0);
}

#[test]
fn ipv6_loopback_reservation_when_the_host_supports_it() {
    let probe = match TcpListener::bind((Ipv6Addr::LOCALHOST, 0)) {
        Ok(probe) => probe,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::AddrNotAvailable | std::io::ErrorKind::Unsupported
            ) =>
        {
            eprintln!("IPv6 loopback branch unavailable: {:?}", error.kind());
            return;
        }
        Err(error) => panic!("IPv6 fixture failed: {error}"),
    };
    drop(probe);
    let bound = reserve(
        Options {
            address: IpAddr::V6(Ipv6Addr::LOCALHOST),
            ..options(PortSelection::Ephemeral)
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(bound.address().ip(), IpAddr::V6(Ipv6Addr::LOCALHOST));
    assert_ne!(bound.address().port(), 0);
    assert!(TcpListener::bind(bound.address()).is_err());
    eprintln!("IPv6 loopback branch exercised=true");
}

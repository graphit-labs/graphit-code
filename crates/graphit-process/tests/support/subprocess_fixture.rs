//! Real, self-bounded child processes for supervisor tests; never a product CLI.
use std::{
    env, fs,
    io::{self, Read, Write},
    net::TcpStream,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args[0].to_str().unwrap() {
        "probe" => {
            println!("cwd={}", env::current_dir().unwrap().display());
            println!("explicit={}", env::var("GRAPHIT_PROCESS_EXPLICIT").unwrap());
            println!(
                "ambient={}",
                env::var("HOME").unwrap_or_else(|_| "absent".into())
            );
            println!("arg={}", args[1].to_string_lossy());
        }
        "bytes" => {
            io::stdout().write_all(&[0, 255, 128, b'x']).unwrap();
            io::stderr().write_all(&[254, 0, b'e']).unwrap();
            std::process::exit(7);
        }
        "echo" => {
            let mut input = Vec::new();
            io::stdin().read_to_end(&mut input).unwrap();
            io::stdout().write_all(&input).unwrap();
        }
        "close-input" => {
            println!("closed");
        }
        "flood" => {
            let count: usize = args[1].to_str().unwrap().parse().unwrap();
            let err = thread::spawn(move || write_bytes(io::stderr(), count, b'e'));
            write_bytes(io::stdout(), count, b'o');
            err.join().unwrap();
        }
        "duplex" => {
            let count = args
                .get(1)
                .map_or(256 * 1024, |v| v.to_str().unwrap().parse().unwrap());
            let err = thread::spawn(move || write_bytes(io::stderr(), count, b'e'));
            write_bytes(io::stdout(), count, b'o');
            let mut input = Vec::new();
            io::stdin().read_to_end(&mut input).unwrap();
            assert_eq!(input.len(), 2 * count);
            err.join().unwrap();
        }
        "incremental" | "exit-output" | "read-ready" => {
            let mut peer = TcpStream::connect(args[1].to_str().unwrap()).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            peer.set_write_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            peer.write_all(b"L").unwrap();
            if args[0] == "read-ready" {
                let mut ack = [0];
                peer.read_exact(&mut ack).unwrap();
            } else {
                io::stdout().write_all(&[0, 255, 128, b'1']).unwrap();
                io::stdout().flush().unwrap();
                if args[0] == "incremental" {
                    let mut ack = [0];
                    peer.read_exact(&mut ack).unwrap();
                    assert_eq!(ack, [b'A']);
                }
                io::stdout().write_all(&[254, 0, b'2', 129]).unwrap();
                io::stdout().flush().unwrap();
            }
            // No shutdown/early-drop: TCP EOF is actual native process exit.
            std::process::exit(7);
        }
        "sequence" => {
            let count: usize = args[1].to_str().unwrap().parse().unwrap();
            let err = thread::spawn(move || write_sequence(io::stderr(), count));
            write_sequence(io::stdout(), count);
            err.join().unwrap();
        }
        "ready" => {
            fs::write(&args[1], b"ready").unwrap();
            thread::sleep(Duration::from_secs(15));
        }
        "term" => {
            #[cfg(unix)]
            {
                let signals = block_term();
                let path = args[1].clone();
                thread::spawn(|| {
                    thread::sleep(Duration::from_secs(15));
                    std::process::exit(99);
                });
                fs::write(&path, b"ready").unwrap();
                signals.wait().unwrap();
                fs::write(std::path::Path::new(&path).with_extension("term"), b"term").unwrap();
            }
            #[cfg(windows)]
            thread::sleep(Duration::from_secs(15));
        }
        "tree" | "tree-exit" => {
            let mut descendant = Command::new(env::current_exe().unwrap())
                .arg("heartbeat")
                .arg(&args[1])
                .stdin(Stdio::null())
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !std::path::Path::new(&args[1]).exists() {
                assert!(start.elapsed() < Duration::from_secs(5));
                thread::sleep(Duration::from_millis(5));
            }
            println!("descendant={}", descendant.id());
            // Reap while this parent lives. tree-exit intentionally terminates
            // the parent first to exercise a descendant adopted by the OS.
            thread::spawn(move || {
                let _ = descendant.wait();
            });
            if args[0] == "tree-exit" {
                std::process::exit(7);
            }
            // Intentionally keep the parent alive without waiting for the child.
            thread::sleep(Duration::from_secs(15));
        }
        "heartbeat" => {
            #[cfg(unix)]
            let _ = block_term();
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(15) {
                fs::write(&args[1], start.elapsed().as_nanos().to_string()).unwrap();
                thread::sleep(Duration::from_millis(10));
            }
        }
        _ => panic!("unknown fixture"),
    }
}

fn write_bytes(mut writer: impl Write, count: usize, byte: u8) {
    let chunk = [byte; 8192];
    let mut remaining = count;
    while remaining > 0 {
        let n = remaining.min(chunk.len());
        if writer.write_all(&chunk[..n]).is_err() {
            return;
        }
        remaining -= n;
    }
}

#[cfg(unix)]
fn block_term() -> nix::sys::signal::SigSet {
    use nix::sys::signal::{SigSet, SigmaskHow, Signal, pthread_sigmask};
    let mut signals = SigSet::empty();
    signals.add(Signal::SIGTERM);
    pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&signals), None).unwrap();
    signals
}

fn write_sequence(mut writer: impl Write, count: usize) {
    let mut chunk = [0; 8192];
    let mut offset = 0;
    while offset < count {
        let bytes = (count - offset).min(chunk.len());
        for (index, byte) in chunk[..bytes].iter_mut().enumerate() {
            *byte = ((offset + index) % 251) as u8;
        }
        if writer.write_all(&chunk[..bytes]).is_err() {
            return;
        }
        offset += bytes;
    }
}

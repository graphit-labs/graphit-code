use std::{
    ffi::OsString,
    io::{self, Read, Write},
    net::TcpStream,
    path::PathBuf,
};

fn invalid() -> io::Error {
    io::ErrorKind::InvalidInput.into()
}

fn decode_hex(value: &str) -> io::Result<Vec<u8>> {
    if value.len() > 16_384 || !value.len().is_multiple_of(2) {
        return Err(invalid());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let pair = std::str::from_utf8(pair).map_err(|_| invalid())?;
            u8::from_str_radix(pair, 16).map_err(|_| invalid())
        })
        .collect()
}

fn emit(stream: &str, bytes: &[u8]) -> io::Result<()> {
    match stream {
        "out" => {
            let mut output = io::stdout().lock();
            output.write_all(bytes)?;
            output.flush()
        }
        "err" => {
            let mut output = io::stderr().lock();
            output.write_all(bytes)?;
            output.flush()
        }
        _ => Err(invalid()),
    }
}

fn arg(args: &[OsString], index: usize) -> io::Result<&str> {
    args.get(index).and_then(|s| s.to_str()).ok_or_else(invalid)
}

fn run() -> io::Result<i32> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mode = arg(&args, 0)?;
    let address = arg(&args, 1)?;
    let mut peer = TcpStream::connect(address)?;
    peer.set_nodelay(true)?;
    match mode {
        "sequence" => {
            let exit = arg(&args, 2)?.parse::<i32>().map_err(|_| invalid())?;
            if args.len() > 67 || !(args.len() - 3).is_multiple_of(2) {
                return Err(invalid());
            }
            for i in (3..args.len()).step_by(2) {
                let stream = arg(&args, i)?;
                let bytes = decode_hex(arg(&args, i + 1)?)?;
                peer.write_all(b"L")?;
                emit(stream, &bytes)?;
                let mut ack = [0];
                peer.read_exact(&mut ack)?;
                if ack != *b"A" {
                    return Err(invalid());
                }
            }
            Ok(exit)
        }
        "scalar" => {
            peer.write_all(b"L")?;
            emit("out", &decode_hex(arg(&args, 2)?)?)?;
            let mut ack = [0];
            peer.read_exact(&mut ack)?;
            if ack != *b"A" {
                return Err(invalid());
            }
            // Only the first ASCII prefix needs an ACK. Native pipe reads may
            // split the final scalar and suffix into multiple text callbacks.
            emit("out", &decode_hex(arg(&args, 3)?)?)?;
            Ok(0)
        }
        "raw" => {
            peer.write_all(b"L")?;
            emit(arg(&args, 2)?, &decode_hex(arg(&args, 3)?)?)?;
            Ok(0)
        }
        "hold" => {
            peer.write_all(b"L")?;
            emit("out", b"A")?;
            let mut ack = [0];
            // The parent deliberately keeps this connection alive until cleanup.
            peer.read_exact(&mut ack)?;
            Err(invalid())
        }
        "capture" => {
            let expected_cwd = PathBuf::from(args.get(2).ok_or_else(invalid)?);
            if std::env::current_dir()? != expected_cwd
                || args.get(3).map(OsString::as_os_str)
                    != Some(std::ffi::OsStr::new("literal;$(fixture)"))
                || std::env::var_os("AI_PLAIN_FIXTURE_VALUE")
                    != Some(OsString::from("CANARY-CLI-PLAIN-PRIVATE"))
                || std::env::var_os("AI_PLAIN_AMBIENT_FORBIDDEN").is_some()
                || std::env::var_os("HOME").is_some()
            {
                return Err(invalid());
            }
            let mut input = Vec::new();
            io::stdin().lock().take(257).read_to_end(&mut input)?;
            if input != b"CANARY-CLI-PLAIN-PRIVATE" {
                return Err(invalid());
            }
            peer.write_all(b"L")?;
            emit("out", b"A")?;
            let mut ack = [0];
            peer.read_exact(&mut ack)?;
            if ack != *b"A" {
                return Err(invalid());
            }
            Ok(0)
        }
        _ => Err(invalid()),
    }
}

fn main() {
    match run() {
        Ok(exit) => std::process::exit(exit),
        Err(error) => {
            eprintln!("plain fixture I/O kind {:?}", error.kind());
            std::process::exit(2);
        }
    }
}

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const HELLO: &str = "quickdpi-show";
const ACK: &str = "quickdpi-ok";

fn port_file() -> std::path::PathBuf {
    crate::config::dir().join("instance.port")
}

pub fn signal_existing() -> bool {
    let Some(port) = std::fs::read_to_string(port_file()).ok().and_then(|s| s.trim().parse::<u16>().ok()) else { return false };
    let Ok(mut s) = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(400)) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    if writeln!(s, "{HELLO}").is_err() {
        return false;
    }
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).is_ok() && line.trim() == ACK
}

pub fn listen(on_show: impl Fn() + Send + 'static) {
    let Ok(l) = TcpListener::bind(("127.0.0.1", 0)) else { return };
    let Ok(addr) = l.local_addr() else { return };
    let _ = std::fs::create_dir_all(crate::config::dir());
    let _ = std::fs::write(port_file(), addr.port().to_string());
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let mut line = String::new();
            let mut r = BufReader::new(&s);
            if r.read_line(&mut line).is_ok() && line.trim() == HELLO {
                let _ = writeln!(&s, "{ACK}");
                on_show();
            }
        }
    });
}

use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::time::Duration;

/// A real listening socket whose accept queue is full. Keep the returned
/// streams alive so subsequent connect attempts must wait for their timeout.
pub fn saturated_listener() -> (TcpListener, Vec<TcpStream>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    // SAFETY: the live listener owns this socket; listen only changes its backlog.
    assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 1) }, 0);
    let address = listener.local_addr().expect("listener address");
    let mut clients = Vec::new();
    for _ in 0..16 {
        match TcpStream::connect_timeout(&address, Duration::from_millis(30)) {
            Ok(stream) => clients.push(stream),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return (listener, clients)
            }
            Err(error) => panic!("could not fill accept queue: {error}"),
        }
    }
    panic!("listen backlog did not fill");
}

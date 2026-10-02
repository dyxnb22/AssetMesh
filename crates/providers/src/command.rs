//! Bounded process output and lifetime, shared by discovery sources.
use crate::{CommandError, CommandRunner};
use std::{
    io::{self, Read},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

const STREAM_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct SystemCommandRunner {
    timeout: Duration,
}

impl Default for SystemCommandRunner {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15),
        }
    }
}

impl SystemCommandRunner {
    /// Interactive directory selection needs time for the user's decision.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, CommandError> {
        run_limited(program, args, self.timeout, STREAM_LIMIT).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                CommandError::NotFound
            } else {
                CommandError::Failed(error)
            }
        })
    }
}

fn run_limited(
    program: &str,
    args: &[&str],
    timeout: Duration,
    limit: usize,
) -> io::Result<Output> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let (sender, receiver) = mpsc::channel();
    let deadline = Instant::now() + timeout;
    let stop = Arc::new(AtomicBool::new(false));
    let readers = [
        read_stream(
            child.stdout.take().unwrap(),
            sender.clone(),
            0,
            limit,
            deadline,
            stop.clone(),
        ),
        read_stream(
            child.stderr.take().unwrap(),
            sender,
            1,
            limit,
            deadline,
            stop.clone(),
        ),
    ];
    let result = (|| {
        let mut streams = [None, None];
        let mut status = None;
        loop {
            if status.is_none() {
                status = child.try_wait()?;
            }
            if let Some(status) = status.filter(|_| streams.iter().all(Option::is_some)) {
                return Ok(Output {
                    status,
                    stdout: streams[0].take().unwrap(),
                    stderr: streams[1].take().unwrap(),
                });
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "command exceeded its time budget",
                ));
            }
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok((index, bytes)) => {
                    let bytes = bytes?;
                    if bytes.len() > limit {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "command exceeded its output budget",
                        ));
                    }
                    streams[index] = Some(bytes);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(Duration::from_millis(50))
                }
            }
        }
    })();
    if result.is_err() {
        stop.store(true, Ordering::Release);
        terminate(&mut child);
    }
    drop(receiver);
    // The Unix process group also closes pipes held by grandchildren.
    #[cfg(unix)]
    for reader in readers {
        let _ = reader.join();
    }
    #[cfg(not(unix))]
    drop(readers);
    result
}

#[cfg(unix)]
fn read_stream(
    mut stream: impl Read + std::os::fd::AsRawFd + Send + 'static,
    sender: mpsc::Sender<(usize, io::Result<Vec<u8>>)>,
    index: usize,
    limit: usize,
    deadline: Instant,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let result = (|| {
            // Nonblocking reads also bound inherited pipes held by a detached
            // descendant outside the original process group.
            let descriptor = stream.as_raw_fd();
            let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
            if flags < 0
                || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(io::Error::last_os_error());
            }
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 8192];
            loop {
                if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "command exceeded its time budget",
                    ));
                }
                let remaining = (limit + 1 - bytes.len()).min(buffer.len());
                match stream.read(&mut buffer[..remaining]) {
                    Ok(0) => return Ok(bytes),
                    Ok(count) => {
                        bytes.extend_from_slice(&buffer[..count]);
                        if bytes.len() > limit {
                            return Ok(bytes);
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        let mut descriptor = libc::pollfd {
                            fd: descriptor,
                            events: libc::POLLIN,
                            revents: 0,
                        };
                        let timeout = deadline
                            .saturating_duration_since(Instant::now())
                            .as_millis()
                            .min(200) as i32;
                        let result = unsafe { libc::poll(&mut descriptor, 1, timeout) };
                        if result < 0
                            && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
                        {
                            return Err(io::Error::last_os_error());
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
        })();
        let _ = sender.send((index, result));
    })
}

#[cfg(not(unix))]
fn read_stream(
    stream: impl Read + Send + 'static,
    sender: mpsc::Sender<(usize, io::Result<Vec<u8>>)>,
    index: usize,
    limit: usize,
    _deadline: Instant,
    _stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stream
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send((index, result));
    })
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    // Only our newly created process group is signalled.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn captures_both_streams_without_deadlocking() {
        let output = run_limited(
            "/bin/sh",
            &["-c", "printf hello; printf problem >&2"],
            Duration::from_secs(2),
            1024,
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"hello");
        assert_eq!(output.stderr, b"problem");
    }

    #[test]
    fn kills_hanging_processes_and_inherited_pipes() {
        let started = Instant::now();
        let error = run_limited(
            "/bin/sh",
            &["-c", "sleep 30 & wait"],
            Duration::from_millis(100),
            1024,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
        let error = run_limited(
            "/bin/sh",
            &["-c", "sleep 30 & exit 0"],
            Duration::from_millis(100),
            1024,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn aborts_unbounded_stdout_or_stderr() {
        for command in ["yes", "yes >&2"] {
            let error =
                run_limited("/bin/sh", &["-c", command], Duration::from_secs(2), 4096).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        }
    }
}

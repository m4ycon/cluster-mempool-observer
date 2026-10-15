use std::io;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

/// A TCP forwarder that sits between an RPC client and the node, counting the
/// connections it accepts.
///
/// With a budget it mimics a port pool that runs dry: after `budget`
/// connections it closes its listening socket, so the kernel answers the next
/// connects with RST (a real `ECONNREFUSED`), and it listens again on the same
/// port once `cooldown` has passed.
///
/// Plain threads rather than tokio tasks, so it runs whatever runtime the test
/// uses, if any.
pub struct Forwarder {
    addr: SocketAddr,
    accepted: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
}

impl Forwarder {
    pub fn start(upstream: SocketAddr) -> Self {
        Self::spawn(upstream, None)
    }

    pub fn with_budget(upstream: SocketAddr, budget: usize, cooldown: Duration) -> Self {
        assert!(budget > 0, "a forwarder with no budget never accepts");
        Self::spawn(upstream, Some((budget, cooldown)))
    }

    fn spawn(upstream: SocketAddr, budget: Option<(usize, Duration)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind forwarder");
        let addr = listener.local_addr().expect("forwarder addr");
        let accepted = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicBool::new(false));

        let (count, stop) = (Arc::clone(&accepted), Arc::clone(&stopped));
        thread::spawn(move || accept_loop(listener, addr, upstream, budget, count, stop));

        Self {
            addr,
            accepted,
            stopped,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Connections accepted so far, each one a TCP handshake the client paid for.
    pub fn connections(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        // wakes the blocking accept so the thread sees the flag; refused during a
        // cooldown, where the thread checks the flag on its own before relistening
        let _ = TcpStream::connect(self.addr);
    }
}

fn accept_loop(
    mut listener: TcpListener,
    addr: SocketAddr,
    upstream: SocketAddr,
    budget: Option<(usize, Duration)>,
    accepted: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
) {
    loop {
        let Ok((client, _)) = listener.accept() else {
            continue;
        };
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        let n = accepted.fetch_add(1, Ordering::SeqCst) + 1;
        thread::spawn(move || {
            let _ = pipe(client, upstream);
        });

        if let Some((budget, cooldown)) = budget
            && n.is_multiple_of(budget)
        {
            drop(listener);
            thread::sleep(cooldown);
            if stopped.load(Ordering::SeqCst) {
                return;
            }
            listener = TcpListener::bind(addr).expect("relisten on the forwarder port");
        }
    }
}

fn pipe(client: TcpStream, upstream: SocketAddr) -> io::Result<()> {
    let server = TcpStream::connect(upstream)?;
    let (mut client_rx, mut server_tx) = (client.try_clone()?, server.try_clone()?);
    let (mut server_rx, mut client_tx) = (server, client);

    let up = thread::spawn(move || {
        let _ = io::copy(&mut client_rx, &mut server_tx);
        let _ = server_tx.shutdown(Shutdown::Write);
    });
    let _ = io::copy(&mut server_rx, &mut client_tx);
    let _ = client_tx.shutdown(Shutdown::Write);
    let _ = up.join();
    Ok(())
}

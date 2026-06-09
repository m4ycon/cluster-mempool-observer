use async_nats::Subscriber;
use shared::nats::{self, NatsConfig};
use shared::subjects::Subject;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::sleep;

/// Spawns a real `nats-server` process for integration tests. Requires the
/// `nats-server` binary on PATH. The process is killed when the struct is
/// dropped.
pub struct NatsServerForTesting {
    pub port: u16,
    child: Child,
    ports_dir: std::path::PathBuf,
    log_path: std::path::PathBuf,
}

impl NatsServerForTesting {
    pub async fn start_nats() -> Self {
        let server = Self::new(&[]).await;
        nats::init(&NatsConfig {
            address: server.address(),
            username: None,
            password: None,
        })
        .await
        .expect("init nats client");
        server
    }

    pub async fn subscribe(&self, subject: &Subject) -> Subscriber {
        nats::get()
            .subscribe(subject.as_str())
            .await
            .expect("subscribe to subject")
    }

    /// Starts a `nats-server` on a random free port. `extra_args` are passed
    /// through, e.g. `["--user", "michael", "--pass", "scott"]`.
    pub async fn new(extra_args: &[&str]) -> Self {
        // Unique dir for the ports file so parallel tests don't collide. Letting
        // nats-server pick the port (`-p -1`) avoids racing on an ephemeral port
        // we'd otherwise have to reserve and release ourselves.
        let ports_dir = std::env::temp_dir().join(format!("nats-test-{}", unique_id()));
        std::fs::create_dir_all(&ports_dir).expect("create ports dir");

        // nats-server logs to stdout/stderr, which the test process would
        // otherwise inherit and print even on success. Capture it to a file so
        // it stays quiet on success but can be dumped if the test fails.
        let log_path = ports_dir.join("nats-server.log");
        let log = std::fs::File::create(&log_path).expect("create nats log file");

        let child = Command::new("nats-server")
            .arg("-p")
            .arg("-1")
            .arg("--ports_file_dir")
            .arg(&ports_dir)
            .args(extra_args)
            .stdout(Stdio::from(log.try_clone().expect("clone log handle")))
            .stderr(Stdio::from(log))
            .spawn()
            .expect("failed to spawn nats-server (is it on PATH?)");

        let port = read_port(&ports_dir, child.id()).await;
        let server = Self {
            port,
            child,
            ports_dir,
            log_path,
        };
        server.wait_until_ready().await;
        server
    }

    async fn wait_until_ready(&self) {
        let address = self.address();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect(&address).await.is_ok() {
                return;
            }
            sleep(Duration::from_millis(50)).await;
        }
        panic!("nats-server on {address} did not become ready in time");
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
}

impl Drop for NatsServerForTesting {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Dump the captured server log only when the test is failing, so
        // success stays quiet but failures show what nats-server did.
        if std::thread::panicking()
            && let Ok(log) = std::fs::read_to_string(&self.log_path)
        {
            eprintln!("--- nats-server log ---\n{log}\n--- end nats-server log ---");
        }
        let _ = std::fs::remove_dir_all(&self.ports_dir);
    }
}

/// Waits for the `nats-server_<pid>.ports` JSON file and returns the client port.
async fn read_port(ports_dir: &std::path::Path, pid: u32) -> u16 {
    let ports_file = ports_dir.join(format!("nats-server_{pid}.ports"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(&ports_file)
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents)
            && let Some(addr) = json["nats"].get(0).and_then(|v| v.as_str())
        {
            return addr
                .rsplit(':')
                .next()
                .and_then(|p| p.parse().ok())
                .expect("parse port from ports file");
        }
        sleep(Duration::from_millis(50)).await;
    }
    panic!("nats-server did not write a ports file in time");
}

/// Process- and call-unique id for naming temp dirs.
fn unique_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

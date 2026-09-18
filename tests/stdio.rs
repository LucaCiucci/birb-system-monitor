use birb_monitor::{
    message::{Command, Message, SystemId},
    transport::Remote,
};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command as Process,
};

fn backend() -> Process {
    let mut process = Process::new(env!("CARGO_BIN_EXE_birb-monitor"));
    process.args(["backend", "--stdio"]);
    process
}

#[test]
fn local_subprocess_uses_remote_transport() {
    let (mut remote, commands, mut messages) = Remote::spawn(backend()).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        commands
            .send(Command::SetInterval {
                target: SystemId::Components,
                interval: Duration::from_millis(50),
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut acknowledged = false;
            loop {
                match messages.recv().await.expect("transport closed") {
                    Message::IntervalChanged {
                        target: SystemId::Components,
                        interval,
                    } => {
                        assert_eq!(interval, Duration::from_millis(50));
                        acknowledged = true;
                    }
                    Message::Sysinfo(
                        birb_monitor::backend::sysinfo::SysinfoMessage::Components(_),
                    ) if acknowledged => break,
                    Message::CommandError(error) => panic!("{error}"),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
    });
    drop(messages);
    let start = std::time::Instant::now();
    remote.shutdown();
    assert!(start.elapsed() < Duration::from_secs(6));
}

#[tokio::test]
async fn backend_rejects_wrong_version_and_exits_on_eof() {
    for version in [999, 1] {
        let mut child = backend()
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut hello = String::new();
        tokio::time::timeout(Duration::from_secs(5), output.read_line(&mut hello))
            .await
            .unwrap()
            .unwrap();
        let hello: serde_json::Value = serde_json::from_str(&hello).unwrap();
        assert_eq!(hello["protocol"], "birb-monitor");
        assert_eq!(hello["version"], 1);
        input
            .write_all(
                format!("{{\"protocol\":\"birb-monitor\",\"version\":{version}}}\n").as_bytes(),
            )
            .await
            .unwrap();
        drop(input);
        let status = tokio::time::timeout(Duration::from_secs(6), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.success(), version == 1);
    }
}

#[test]
fn launch_failure_is_reported_to_frontend() {
    let (mut remote, _commands, mut messages) =
        Remote::spawn(Process::new("/no-such-birb-monitor-backend")).unwrap();
    let message = messages.blocking_recv().unwrap();
    assert!(matches!(message, Message::CommandError(error) if error.contains("Failed to launch")));
    assert!(messages.blocking_recv().is_none());
    remote.shutdown();
}

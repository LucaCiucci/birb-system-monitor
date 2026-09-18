//! Versioned JSON Lines over stdio. Stdout is exclusively protocol traffic.
use crate::{
    backend::Systems,
    message::{Command, Message},
};
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{process::Stdio, thread::JoinHandle, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
};

const VERSION: u32 = 1;
const MAX_FRAME: usize = 32 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Hello {
    protocol: String,
    version: u32,
}

async fn read_frame<T: DeserializeOwned>(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> anyhow::Result<Option<T>> {
    let mut frame = Vec::new();
    loop {
        let bytes = reader.fill_buf().await?;
        if bytes.is_empty() {
            if frame.is_empty() {
                return Ok(None);
            }
            bail!("Truncated protocol frame");
        }
        let end = bytes.iter().position(|&b| b == b'\n');
        let count = end.map_or(bytes.len(), |i| i + 1);
        if frame.len() + count > MAX_FRAME {
            bail!("Protocol frame exceeds 32 MiB");
        }
        frame.extend_from_slice(&bytes[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(Some(
                serde_json::from_slice(&frame).context("Invalid JSON protocol frame")?,
            ));
        }
    }
}

async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
) -> anyhow::Result<()> {
    let mut frame = serde_json::to_vec(value)?;
    if frame.len() + 1 > MAX_FRAME {
        bail!("Protocol frame exceeds 32 MiB");
    }
    frame.push(b'\n');
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

async fn handshake(
    reader: &mut (impl AsyncBufRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(30), async {
        write_frame(
            writer,
            &Hello {
                protocol: "birb-monitor".into(),
                version: VERSION,
            },
        )
        .await?;
        let hello: Hello = read_frame(reader)
            .await?
            .context("Backend closed before protocol handshake")?;
        if hello.protocol != "birb-monitor" || hello.version != VERSION {
            bail!(
                "Incompatible backend protocol (expected birb-monitor v{VERSION}, got {} v{})",
                hello.protocol,
                hello.version
            );
        }
        Ok(())
    })
    .await
    .context("Timed out waiting for backend handshake")?
}

/// Attached backend entry point. EOF or malformed input stops the session.
pub fn serve_stdio() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = rt.block_on(async {
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut writer = tokio::io::stdout();
        handshake(&mut reader, &mut writer).await?;
        let (mut systems, commands, mut messages) = Systems::new()?;
        let input = async {
            while let Some(command) = read_frame::<Command>(&mut reader).await? {
                commands.send(command).await.context("Backend stopped")?;
            }
            Ok::<_, anyhow::Error>(())
        };
        let output = async {
            while let Some(message) = messages.recv().await {
                write_frame(&mut writer, &message).await?;
            }
            Ok::<_, anyhow::Error>(())
        };
        let result = tokio::select! { r = input => r, r = output => r };
        drop(messages);
        systems.shutdown();
        result
    });
    // Tokio stdin may still have an OS read pending if stdout failed first.
    rt.shutdown_timeout(Duration::from_millis(100));
    result
}

/// Owns one subprocess session. There is deliberately no automatic reconnect:
/// a new session must start with fresh frontend histories.
pub struct Remote {
    stop: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl Remote {
    pub fn ssh(
        host: &str,
        binary: &str,
    ) -> anyhow::Result<(Self, mpsc::Sender<Command>, mpsc::Receiver<Message>)> {
        if host.is_empty() || host.starts_with('-') {
            bail!("Invalid SSH destination");
        }
        let command = remote_command(binary)?;
        let mut process = tokio::process::Command::new("ssh");
        // Authentication uses the SSH agent/configuration. Never read passwords
        // from the protocol pipe; errors and host-key diagnostics go to stderr.
        process.args(["-T", "-o", "BatchMode=yes", "--", host, &command]);
        Self::spawn(process)
    }

    /// Also permits exercising the exact transport with a local backend process.
    pub fn spawn(
        mut process: tokio::process::Command,
    ) -> anyhow::Result<(Self, mpsc::Sender<Command>, mpsc::Receiver<Message>)> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (commands, mut rx) = mpsc::channel(32);
        let (tx, messages) = mpsc::channel(64);
        let (stop, stopped) = oneshot::channel();
        process
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        let worker = std::thread::Builder::new().name("remote-backend".into()).spawn(move || {
            rt.block_on(async {
                let session = async {
                    let mut child = process.spawn().context("Failed to launch backend transport")?;
                    let mut reader = BufReader::new(child.stdout.take().context("Missing backend stdout")?);
                    let mut writer = child.stdin.take().context("Missing backend stdin")?;
                    let exchange = async {
                        handshake(&mut reader, &mut writer).await?;
                        let input = async {
                            while let Some(message) = read_frame::<Message>(&mut reader).await? {
                                tx.send(message).await.context("Frontend closed")?;
                            }
                            bail!("Backend disconnected; last readings are stale (see stderr for SSH diagnostics)")
                        };
                        let output = async {
                            while let Some(command) = rx.recv().await { write_frame(&mut writer, &command).await?; }
                            Ok::<_, anyhow::Error>(())
                        };
                        tokio::select! { r = input => r, r = output => r }
                    };
                    let result = tokio::select! { r = exchange => r, _ = stopped => Ok(()), _ = tx.closed() => Ok(()) };
                    drop(writer);
                    // Closing stdin lets the server shut down. Kill and reap a
                    // stuck SSH process after a bounded grace period.
                    if tokio::time::timeout(Duration::from_secs(3), child.wait()).await.is_err() {
                        let _ = child.kill().await;
                    }
                    result
                };
                if let Err(error) = session.await {
                    let _ = tokio::time::timeout(Duration::from_millis(100), tx.send(Message::CommandError(format!("Remote connection: {error:#}")))).await;
                }
            });
        })?;
        Ok((
            Self {
                stop: Some(stop),
                worker: Some(worker),
            },
            commands,
            messages,
        ))
    }

    pub fn shutdown(&mut self) {
        self.stop.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn remote_command(binary: &str) -> anyhow::Result<String> {
    if binary.is_empty() || binary.contains(['\0', '\n', '\r']) {
        bail!("Remote executable path must not be empty or contain a line break");
    }
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\"'\"'"));
    let executable = if let Some(path) = binary.strip_prefix("~/") {
        if path.is_empty() {
            bail!("Remote executable path after ~/ must not be empty");
        }
        // Expand only the home directory; quote the remainder so the option
        // cannot add shell syntax or alter the protocol command.
        format!("\"$HOME\"/{}", quote(path))
    } else {
        quote(binary)
    };
    Ok(format!("exec {executable} backend --stdio"))
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frames_preserve_embedded_newlines_and_detect_truncation() {
        let value = Message::CommandError("line one\nline two".into());
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &value).await.unwrap();
        assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), 1);
        let mut reader = BufReader::new(bytes.as_slice());
        assert!(
            matches!(read_frame::<Message>(&mut reader).await.unwrap(), Some(Message::CommandError(text)) if text == "line one\nline two")
        );
        assert!(read_frame::<Message>(&mut reader).await.unwrap().is_none());
        bytes.pop();
        assert!(
            read_frame::<Message>(&mut BufReader::new(bytes.as_slice()))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn rejects_oversized_and_invalid_frames() {
        let bytes = vec![b'x'; MAX_FRAME + 1];
        let error = read_frame::<Message>(&mut BufReader::new(bytes.as_slice()))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("exceeds"));
        assert!(
            read_frame::<Message>(&mut BufReader::new(&b"banner from shell\n"[..]))
                .await
                .is_err()
        );
    }

    #[test]
    fn remote_executable_path_is_shell_safe() {
        assert_eq!(
            remote_command("birb-monitor").unwrap(),
            "exec 'birb-monitor' backend --stdio"
        );
        assert_eq!(
            remote_command("~/bin/birb-monitor").unwrap(),
            "exec \"$HOME\"/'bin/birb-monitor' backend --stdio"
        );
        assert_eq!(
            remote_command("/opt/birb monitor/bin").unwrap(),
            "exec '/opt/birb monitor/bin' backend --stdio"
        );
        let command = remote_command("$(bad); echo nope").unwrap();
        assert_eq!(command, "exec '$(bad); echo nope' backend --stdio");
        assert!(remote_command("\n").is_err());
    }
}

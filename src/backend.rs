use self::{
    docker::{DockerCommand, DockerHandler},
    sysinfo::{ComponentsHandler, SysinfoCommand, SystemHandler},
};
use crate::{
    message::{Command, Message, SystemId},
    utils::TimedTask,
};
use std::{thread::JoinHandle, time::Duration};
use tokio::sync::{mpsc, oneshot};

pub mod docker;
pub mod sysinfo;

/// Local backend owner. The runtime and collectors live on a dedicated thread.
/// Dropping or shutting down Systems stops routing and bounds shutdown waiting.
/// No frontend state or GUI dependencies cross this boundary.
pub struct Systems {
    stop: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl Systems {
    pub fn new() -> std::io::Result<(Self, mpsc::Sender<Command>, mpsc::Receiver<Message>)> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let (commands, mut rx) = mpsc::channel(32);
        let (tx, messages) = mpsc::channel(64);
        let (stop, stopped) = oneshot::channel();
        let worker = std::thread::Builder::new()
            .name("systems".into())
            .spawn(move || {
                rt.block_on(async {
                    let system = TimedTask::with_handler(
                        &rt,
                        Duration::from_secs(1),
                        SystemHandler::new(tx.clone()),
                    );
                    let components = TimedTask::with_handler(
                        &rt,
                        Duration::from_secs(5),
                        ComponentsHandler::new(tx.clone()),
                    );
                    let docker = TimedTask::with_handler(
                        &rt,
                        Duration::from_secs(2),
                        DockerHandler::new("unix:///var/run/docker.sock".into(), tx.clone()),
                    );
                    let route = async {
                        while let Some(command) = rx.recv().await {
                            let result: Result<(), String> = match command {
                                Command::SetInterval { target, interval } => {
                                    if interval < Duration::from_millis(50)
                                        || interval > Duration::from_secs(86400)
                                    {
                                        Err("Read interval must be between 50 ms and 24 hours"
                                            .into())
                                    } else {
                                        let result = match target {
                                            SystemId::System => system.set_interval(interval),
                                            SystemId::Components => {
                                                components.set_interval(interval)
                                            }
                                            SystemId::Docker => docker.set_interval(interval),
                                        }
                                        .map_err(|_| "Collector stopped".to_string());
                                        if result.is_ok() {
                                            if tx
                                                .send(Message::IntervalChanged { target, interval })
                                                .await
                                                .is_err()
                                            {
                                                break;
                                            }
                                        }
                                        result
                                    }
                                }
                                Command::Refresh(SystemId::System)
                                | Command::Sysinfo(SysinfoCommand::Refresh) => system
                                    .send_async(SysinfoCommand::Refresh)
                                    .await
                                    .map_err(|e| e.to_string()),
                                Command::Sysinfo(
                                    command @ SysinfoCommand::SetProcessDetailSelection { .. },
                                ) => system.send_async(command).await.map_err(|e| e.to_string()),
                                Command::Refresh(SystemId::Components) => components
                                    .send_async(SysinfoCommand::Refresh)
                                    .await
                                    .map_err(|e| e.to_string()),
                                Command::Refresh(SystemId::Docker) => docker
                                    .send_async(DockerCommand::Refresh)
                                    .await
                                    .map_err(|e| e.to_string()),
                                Command::Docker(command) => {
                                    docker.send_async(command).await.map_err(|e| e.to_string())
                                }
                            };
                            if let Err(error) = result {
                                if tx.send(Message::CommandError(error)).await.is_err() {
                                    break;
                                }
                            }
                        }
                    };
                    tokio::select! {
                        _ = stopped => {},
                        _ = tx.closed() => {},
                        _ = route => {},
                    }
                    // Bounded wait also handles a full output queue or unresponsive daemon.
                    let _ = tokio::time::timeout(Duration::from_secs(1), async {
                        let _ = tokio::join!(
                            system.shutdown(),
                            components.shutdown(),
                            docker.shutdown()
                        );
                    })
                    .await;
                });
                // Running spawn_blocking reads cannot be cancelled. Do not make app
                // shutdown wait indefinitely for an OS read.
                rt.shutdown_timeout(Duration::from_secs(1));
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

impl Drop for Systems {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_intervals_and_rejects_invalid_values() {
        let (mut systems, commands, mut messages) = Systems::new().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            commands.send(Command::SetInterval { target: SystemId::Components, interval: Duration::ZERO }).await.unwrap();
            let error = tokio::time::timeout(Duration::from_secs(5), messages.recv()).await.unwrap().unwrap();
            assert!(matches!(error, Message::CommandError(_)));
            commands.send(Command::SetInterval { target: SystemId::Components, interval: Duration::from_millis(50) }).await.unwrap();
            let ack = tokio::time::timeout(Duration::from_secs(5), messages.recv()).await.unwrap().unwrap();
            assert!(matches!(ack, Message::IntervalChanged { target: SystemId::Components, interval } if interval == Duration::from_millis(50)));
            tokio::time::timeout(Duration::from_secs(3), async {
                let mut samples = 0;
                while samples < 2 {
                    if let Some(Message::Sysinfo(sysinfo::SysinfoMessage::Components(_))) = messages.recv().await { samples += 1; }
                }
            }).await.unwrap();
        });
        drop(messages);
        systems.shutdown();
    }

    #[test]
    fn shutdown_does_not_deadlock_on_full_output() {
        let (mut systems, commands, _messages) = Systems::new().unwrap();
        for _ in 0..80 {
            commands
                .blocking_send(Command::SetInterval {
                    target: SystemId::Components,
                    interval: Duration::from_secs(5),
                })
                .unwrap();
        }
        let start = std::time::Instant::now();
        systems.shutdown();
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}

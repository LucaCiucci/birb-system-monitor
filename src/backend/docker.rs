use std::time::SystemTime;

use bollard::{API_DEFAULT_VERSION, Docker};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    message::Message,
    utils::{TimedTaskEvent, TimedTaskHandler},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DockerCommand {
    Refresh,
    /// Changes the daemon endpoint and immediately samples it.
    SetSocketPath(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DockerMessage {
    Snapshot(DockerSnapshot),
}

/// Results from one sampling cycle. Failed reads never masquerade as empty lists.
/// The frontend decides whether to retain older data after a failed read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockerSnapshot {
    pub captured_at: SystemTime,
    pub socket_path: String,
    pub containers: Result<Vec<SimpleContainer>, String>,
    pub images: Result<Vec<SimpleImage>, String>,
}

/// Container and image reads share one cadence, supplied by TimedTask.
/// Owns the client but no UI state or history.
pub struct DockerHandler {
    socket_path: String,
    docker: Option<Docker>,
    tx: mpsc::Sender<Message>,
}

impl DockerHandler {
    pub fn new(socket_path: String, tx: mpsc::Sender<Message>) -> Self {
        Self {
            socket_path,
            docker: None,
            tx,
        }
    }

    async fn sample(&mut self) -> DockerSnapshot {
        // Creating a client does not establish daemon connectivity. Only the
        // request results tell us whether the daemon actually responded.
        let connection = match self.docker.as_ref() {
            Some(docker) => Ok(docker.clone()),
            None => Docker::connect_with_socket(&self.socket_path, 120, API_DEFAULT_VERSION),
        };
        let (containers, images) = match connection {
            Ok(docker) => {
                self.docker = Some(docker.clone());
                let (containers, images) =
                    tokio::join!(list_containers(&docker), list_images(&docker),);
                (
                    containers.map_err(|error| format!("Failed to list containers: {error}")),
                    images.map_err(|error| format!("Failed to list images: {error}")),
                )
            }
            Err(error) => {
                let error = format!("Failed to create Docker client: {error}");
                (Err(error.clone()), Err(error))
            }
        };
        DockerSnapshot {
            captured_at: SystemTime::now(),
            socket_path: self.socket_path.clone(),
            containers,
            images,
        }
    }
}

impl TimedTaskHandler<DockerCommand> for DockerHandler {
    async fn handle(&mut self, event: TimedTaskEvent<DockerCommand>) {
        match event {
            TimedTaskEvent::Tick | TimedTaskEvent::Message(DockerCommand::Refresh) => {}
            TimedTaskEvent::Message(DockerCommand::SetSocketPath(socket_path)) => {
                self.socket_path = socket_path;
                self.docker = None;
            }
        }
        if self.tx.is_closed() {
            return;
        }
        let snapshot = self.sample().await;
        // Drain or close the receiver before awaiting shutdown: bounded output
        // deliberately applies backpressure rather than silently dropping data.
        if self
            .tx
            .send(Message::Docker(DockerMessage::Snapshot(snapshot)))
            .await
            .is_err()
        {
            tracing::debug!("Docker output receiver closed");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created: i64,
    pub ports: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleImage {
    pub id: String,
    pub repo_tags: Vec<String>,
    pub created: i64,
    pub size: i64,
}

async fn list_containers(docker: &Docker) -> anyhow::Result<Vec<SimpleContainer>> {
    use bollard::query_parameters::ListContainersOptionsBuilder;

    let options = ListContainersOptionsBuilder::default().all(true).build();

    let containers = docker.list_containers(Some(options)).await?;

    Ok(containers
        .into_iter()
        .map(|c| {
            let names = c.names.unwrap_or_default();
            let name = names
                .first()
                .cloned()
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string();

            let ports_str = c
                .ports
                .unwrap_or_default()
                .iter()
                .map(|p| {
                    let typ = p.typ.map(|t| t.to_string()).unwrap_or_else(|| "tcp".into());
                    match p.public_port {
                        Some(pub_port) => format!(
                            "{}:{}->{}/{}",
                            p.ip.as_deref().unwrap_or("0.0.0.0"),
                            pub_port,
                            p.private_port,
                            typ
                        ),
                        None => format!("{}/{}", p.private_port, typ),
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");

            SimpleContainer {
                id: c.id.unwrap_or_default().to_string(),
                name,
                image: c.image.unwrap_or_default(),
                status: c.status.unwrap_or_default(),
                state: c.state.map(|s| s.to_string()).unwrap_or_default(),
                created: c.created.unwrap_or(0),
                ports: ports_str,
            }
        })
        .collect())
}

async fn list_images(docker: &Docker) -> anyhow::Result<Vec<SimpleImage>> {
    use bollard::query_parameters::ListImagesOptionsBuilder;

    let options = ListImagesOptionsBuilder::default().all(true).build();

    let images = docker.list_images(Some(options)).await?;

    Ok(images
        .into_iter()
        .map(|i| SimpleImage {
            id: i.id,
            repo_tags: i.repo_tags,
            created: i.created,
            size: i.size,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::TimedTask;
    use std::time::Duration;

    #[test]
    fn partial_results_round_trip_without_losing_successful_data() {
        let message = Message::Docker(DockerMessage::Snapshot(DockerSnapshot {
            captured_at: SystemTime::now(),
            socket_path: "unix:///var/run/docker.sock".into(),
            containers: Ok(vec![SimpleContainer {
                id: "container-id".into(),
                name: "example".into(),
                image: "example:latest".into(),
                status: "Up".into(),
                state: "running".into(),
                created: 42,
                ports: "80/tcp".into(),
            }]),
            images: Err("image request failed".into()),
        }));
        let encoded = serde_json::to_vec(&message).unwrap();
        let decoded: Message = serde_json::from_slice(&encoded).unwrap();
        let Message::Docker(DockerMessage::Snapshot(snapshot)) = decoded else {
            panic!("expected Docker snapshot");
        };
        assert_eq!(snapshot.containers.unwrap()[0].name, "example");
        assert_eq!(snapshot.images.unwrap_err(), "image request failed");
    }

    #[cfg(unix)]
    #[test]
    fn timed_handler_reports_missing_daemon_and_endpoint_changes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (tx, mut rx) = mpsc::channel(2);
            let first = format!("unix:///tmp/birb-docker-{}.sock", uuid::Uuid::new_v4());
            let second = format!("unix:///tmp/birb-docker-{}.sock", uuid::Uuid::new_v4());
            let task = TimedTask::with_handler(
                &rt,
                Duration::from_secs(3600),
                DockerHandler::new(first.clone(), tx),
            );
            for (command, endpoint) in [
                (DockerCommand::Refresh, first),
                (DockerCommand::SetSocketPath(second.clone()), second),
            ] {
                task.send_async(command).await.unwrap();
                let message = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                    .await
                    .unwrap()
                    .unwrap();
                let Message::Docker(DockerMessage::Snapshot(snapshot)) = message else {
                    panic!("expected Docker snapshot");
                };
                assert_eq!(snapshot.socket_path, endpoint);
                assert!(snapshot.containers.is_err());
                assert!(snapshot.images.is_err());
            }
            drop(rx);
            task.shutdown().await.unwrap();
        });
    }

    #[tokio::test]
    async fn closed_output_does_not_create_client() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let mut handler = DockerHandler::new("unused".into(), tx);
        handler.handle(TimedTaskEvent::Tick).await;
        assert!(handler.docker.is_none());
    }
}

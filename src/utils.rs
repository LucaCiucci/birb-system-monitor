use std::{future::Future, time::Duration};

use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot, watch},
    task::{JoinError, JoinHandle},
    time::{Instant, sleep},
};

/// A stateful handler whose future may borrow the handler until it completes.
pub trait TimedTaskHandler<M>: Send + 'static {
    fn handle(&mut self, event: TimedTaskEvent<M>) -> impl Future<Output = ()> + Send;
}

impl<M, F, H> TimedTaskHandler<M> for H
where
    H: FnMut(TimedTaskEvent<M>) -> F + Send + 'static,
    F: Future<Output = ()> + Send,
{
    fn handle(&mut self, event: TimedTaskEvent<M>) -> impl Future<Output = ()> + Send {
        self(event)
    }
}

/// Runs commands and ticks sequentially on a runtime that must outlive this task.
///
/// The first tick waits one interval. Subsequent ticks wait one interval after
/// the previous tick's handler finishes. Commands do not reset the timer.
/// Interval changes restart the delay when processed, without emitting a tick.
/// Changes and shutdown are processed only after the active handler completes.
pub struct TimedTask<M> {
    interval_tx: watch::Sender<Duration>,
    tx: mpsc::Sender<M>,
    stop_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl<M: Send + 'static> TimedTask<M> {
    /// Creates a task from a closure. Panics if `interval` is zero.
    pub fn new<F>(
        rt: &Runtime,
        interval: Duration,
        handler: impl FnMut(TimedTaskEvent<M>) -> F + Send + 'static,
    ) -> Self
    where
        F: Future<Output = ()> + Send,
    {
        Self::with_handler(rt, interval, handler)
    }

    /// Creates a task with an owned handler, allowing async access to its state.
    ///
    /// # Panics
    ///
    /// Panics if `interval` is zero.
    pub fn with_handler(
        rt: &Runtime,
        interval: Duration,
        mut handler: impl TimedTaskHandler<M>,
    ) -> Self {
        assert!(!interval.is_zero(), "timed task interval must be nonzero");
        let (interval_tx, mut interval_rx) = watch::channel(interval);
        let (tx, mut rx) = mpsc::channel(32);
        let (stop_tx, mut stop_rx) = oneshot::channel();
        let handle = rt.spawn(async move {
            let timer = sleep(interval);
            tokio::pin!(timer);
            loop {
                tokio::select! {
                    // Shutdown and configuration take precedence over queued work.
                    biased;
                    _ = &mut stop_rx => break,
                    changed = interval_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        let interval = *interval_rx.borrow_and_update();
                        timer.as_mut().reset(Instant::now() + interval);
                    }
                    _ = &mut timer => {
                        handler.handle(TimedTaskEvent::Tick).await;
                        let interval = *interval_rx.borrow();
                        timer.as_mut().reset(Instant::now() + interval);
                    }
                    msg = rx.recv() => {
                        let Some(msg) = msg else { break };
                        handler.handle(TimedTaskEvent::Message(msg)).await;
                    }
                }
            }
        });
        Self {
            interval_tx,
            tx,
            stop_tx: Some(stop_tx),
            handle: Some(handle),
        }
    }

    /// Attempts to enqueue a command; reports a full queue or stopped task.
    pub fn send(&self, msg: M) -> Result<(), mpsc::error::TrySendError<M>> {
        self.tx.try_send(msg)
    }

    /// Waits for queue capacity; reports a stopped task.
    pub async fn send_async(&self, msg: M) -> Result<(), mpsc::error::SendError<M>> {
        self.tx.send(msg).await
    }

    /// Updates the delay. Panics if `interval` is zero.
    pub fn set_interval(
        &self,
        interval: Duration,
    ) -> Result<(), watch::error::SendError<Duration>> {
        assert!(!interval.is_zero(), "timed task interval must be nonzero");
        self.interval_tx.send(interval)
    }

    /// Finishes the active handler, discards queued commands, and waits for exit.
    /// Also reports handler panics. A handler that never finishes blocks shutdown.
    pub async fn shutdown(mut self) -> Result<(), JoinError> {
        self.stop_tx.take();
        self.handle.take().expect("task handle missing").await
    }
}

impl<M> Drop for TimedTask<M> {
    fn drop(&mut self) {
        // Closing the channel requests graceful shutdown, including if a
        // shutdown future is cancelled. Drop itself cannot wait for completion.
        self.stop_tx.take();
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TimedTaskEvent<M> {
    Tick,
    Message(M),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{runtime::Builder, task::yield_now, time::advance};

    fn runtime() -> Runtime {
        Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap()
    }

    #[test]
    fn interval_changes_restart_deadline_without_extra_ticks() {
        let rt = runtime();
        rt.block_on(async {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let task = TimedTask::<()>::new(&rt, Duration::from_secs(60), move |event| {
                tx.send(event).unwrap();
                async {}
            });
            yield_now().await;
            advance(Duration::from_secs(5)).await;
            task.set_interval(Duration::from_secs(1)).unwrap();
            yield_now().await;
            assert!(rx.try_recv().is_err());
            advance(Duration::from_secs(1)).await;
            yield_now().await;
            assert_eq!(rx.try_recv().unwrap(), TimedTaskEvent::Tick);
            task.set_interval(Duration::from_secs(60)).unwrap();
            yield_now().await;
            advance(Duration::from_secs(2)).await;
            yield_now().await;
            assert!(rx.try_recv().is_err());
            task.shutdown().await.unwrap();
        });
    }

    struct SlowHandler {
        tx: mpsc::UnboundedSender<usize>,
        count: usize,
    }

    impl TimedTaskHandler<()> for SlowHandler {
        async fn handle(&mut self, _: TimedTaskEvent<()>) {
            self.count += 1;
            self.tx.send(self.count).unwrap();
            sleep(Duration::from_secs(10)).await;
            self.tx.send(self.count).unwrap();
        }
    }

    #[test]
    fn slow_handler_gets_full_delay_and_shutdown_waits_for_completion() {
        let rt = runtime();
        rt.block_on(async {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let task =
                TimedTask::with_handler(&rt, Duration::from_secs(1), SlowHandler { tx, count: 0 });
            assert_eq!(rx.recv().await, Some(1));
            assert_eq!(rx.recv().await, Some(1));
            advance(Duration::from_millis(500)).await;
            yield_now().await;
            assert!(rx.try_recv().is_err());
            assert_eq!(rx.recv().await, Some(2));
            task.shutdown().await.unwrap();
            assert_eq!(rx.recv().await, Some(2));
            assert_eq!(rx.recv().await, None);
        });
    }

    #[test]
    fn drop_stops_task_and_full_queue_is_reported() {
        let rt = runtime();
        rt.block_on(async {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let task = TimedTask::new(&rt, Duration::from_secs(1), move |event| {
                tx.send(event).unwrap();
                async {}
            });
            for i in 0..32 {
                task.send(i).unwrap();
            }
            assert!(matches!(
                task.send(32),
                Err(mpsc::error::TrySendError::Full(32))
            ));
            drop(task);
            yield_now().await;
            assert_eq!(rx.recv().await, None);
        });
    }

    #[test]
    fn handler_failure_is_observable() {
        let rt = runtime();
        rt.block_on(async {
            let task = TimedTask::new(&rt, Duration::from_secs(1), |_: TimedTaskEvent<()>| async {
                panic!("handler failed")
            });
            task.send_async(()).await.unwrap();
            yield_now().await;
            assert!(matches!(
                task.send(()),
                Err(mpsc::error::TrySendError::Closed(()))
            ));
            assert!(task.set_interval(Duration::from_secs(2)).is_err());
            assert!(task.shutdown().await.unwrap_err().is_panic());
        });
    }

    #[test]
    #[should_panic(expected = "interval must be nonzero")]
    fn zero_initial_interval_is_rejected() {
        let rt = runtime();
        let _task = TimedTask::<()>::new(&rt, Duration::ZERO, |_| async {});
    }

    #[test]
    #[should_panic(expected = "interval must be nonzero")]
    fn zero_updated_interval_is_rejected() {
        let rt = runtime();
        let task = TimedTask::<()>::new(&rt, Duration::from_secs(1), |_| async {});
        let _ = task.set_interval(Duration::ZERO);
    }
}

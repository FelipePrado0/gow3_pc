//! Background work for the Elm loop.

use std::future::Future;

use iced::futures::channel::oneshot;
use iced::Task;

/// Runs `work` on its own thread and delivers the result as one message. If the thread cannot
/// start or panics, no message arrives.
pub fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Task<T> {
    let (sender, receiver) = oneshot::channel();
    let _ = std::thread::Builder::new().spawn(move || {
        let _ = sender.send(work());
    });
    // `and_then` skips the `None` of a dropped sender.
    Task::future(async move { receiver.await.ok() }).and_then(Task::done)
}

/// `blocking` for work whose end the window waits for (a copy, a download): a thread that cannot
/// start or panics delivers `lost` instead of nothing, so the window does not wait forever.
pub fn blocking_or<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    lost: T,
) -> Task<T> {
    Task::future(finished(work, lost))
}

/// Starts `work` on its own thread; the future of its result, or of `lost` when the thread
/// cannot start or panics (its sender is then dropped).
pub fn finished<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    lost: T,
) -> impl Future<Output = T> + Send + 'static {
    let (sender, receiver) = oneshot::channel();
    let _ = std::thread::Builder::new().spawn(move || {
        let _ = sender.send(work());
    });
    async move { receiver.await.unwrap_or(lost) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::futures::executor::block_on;

    #[test]
    fn a_worker_that_panics_delivers_the_lost_value() {
        assert_eq!(block_on(finished(|| "done", "lost")), "done");
        let panics = || -> &'static str { panic!("the worker failed") };
        assert_eq!(block_on(finished(panics, "lost")), "lost");
    }
}

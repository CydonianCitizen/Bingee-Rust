//! A fixed pool of worker threads for blocking network and credential-store
//! calls, and the way back to the UI thread, so the Slint event loop never
//! waits on I/O (ADR-0008).
//!
//! Created once at startup and owned by the app. When the last `Network`
//! handle is dropped the channel closes and each worker exits after its
//! current job; at process exit, requests still in flight are abandoned.

use std::sync::{Arc, Mutex, mpsc};

pub type Job = Box<dyn FnOnce() + Send>;

/// Runs a closure on the UI thread, called from any thread.
pub type Post = Arc<dyn Fn(Job) + Send + Sync>;

#[derive(Clone)]
pub struct Network {
    jobs: mpsc::Sender<Job>,
    post: Post,
}

impl Network {
    /// Workers whose results go back through the Slint event loop.
    pub fn start(workers: usize) -> Self {
        // Fails only when the event loop is gone: the app is closing.
        Self::with_post(
            workers,
            Arc::new(|job| drop(slint::invoke_from_event_loop(job))),
        )
    }

    /// Workers whose results go back through `post` (tests pump their own
    /// queue on the test thread).
    pub fn with_post(workers: usize, post: Post) -> Self {
        let (jobs, queue) = mpsc::channel::<Job>();
        let queue = Arc::new(Mutex::new(queue));
        for n in 0..workers {
            let queue = queue.clone();
            std::thread::Builder::new()
                .name(format!("bingee-network-{n}"))
                .spawn(move || {
                    loop {
                        // The guard is dropped before the job runs.
                        let job = queue
                            .lock()
                            .map_err(drop)
                            .and_then(|q| q.recv().map_err(drop));
                        match job {
                            Ok(job) => job(),
                            Err(()) => break,
                        }
                    }
                })
                // Bootstrap: without threads there is no remote feature; the
                // panic hook records why.
                .expect("the OS refused to start a network worker thread");
        }
        Self { jobs, post }
    }

    /// Runs `job` on a worker.
    pub fn run(&self, job: impl FnOnce() + Send + 'static) {
        // Fails only once every worker has exited, when nothing can run.
        let _ = self.jobs.send(Box::new(job));
    }

    /// Runs `ui` on the UI thread.
    pub fn post(&self, ui: impl FnOnce() + Send + 'static) {
        (self.post)(Box::new(ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn jobs_run_concurrently_off_the_calling_thread_and_post_back() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let posted: Arc<Mutex<Vec<Job>>> = Arc::default();
        let queue = posted.clone();
        let network = Network::with_post(2, Arc::new(move |job| queue.lock().unwrap().push(job)));
        let caller = std::thread::current().id();
        let (running, most) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let (done, results) = mpsc::channel();
        for _ in 0..2 {
            let (done, running, most) = (done.clone(), running.clone(), most.clone());
            let back = network.clone();
            network.run(move || {
                most.fetch_max(running.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(300));
                running.fetch_sub(1, Ordering::SeqCst);
                let worker = std::thread::current().id();
                back.post(move || done.send((worker, std::thread::current().id())).unwrap());
            });
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while posted.lock().unwrap().len() < 2 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::mem::take(&mut *posted.lock().unwrap())
            .into_iter()
            .for_each(|job| job());
        let threads: Vec<_> = results.try_iter().collect();
        assert_eq!(most.load(Ordering::SeqCst), 2, "the two jobs overlapped");
        assert_eq!(threads.len(), 2);
        assert!(
            threads
                .iter()
                .all(|(worker, ui)| *worker != caller && *ui == caller)
        );
    }
}

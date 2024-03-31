//! Thread pool that joins all thread when dropped.

// NOTE: Crossbeam channels are MPMC, which means that you don't need to wrap the receiver in
// Arc<Mutex<..>>. Just clone the receiver and give it to each worker thread.
use crossbeam_channel::{unbounded, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::thread::{JoinHandle, sleep};
use std::time::Duration;

struct Job(Box<dyn FnOnce() + Send + 'static>);

#[derive(Debug)]
struct Worker {
    _id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for Worker {
    /// When dropped, the thread's `JoinHandle` must be `join`ed.  If the worker panics, then this
    /// function should panic too.
    ///
    /// NOTE: The thread is detached if not `join`ed explicitly.
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

/// Internal data structure for tracking the current job status. This is shared by worker closures
/// via `Arc` so that the workers can report to the pool that it started/finished a job.
#[derive(Debug, Default)]
struct ThreadPoolInner {
    job_count: Mutex<usize>,
    empty_condvar: Condvar,
}

impl ThreadPoolInner {
    /// Increment the job count.
    fn start_job(&self) {
        let mut res = self.job_count.lock().unwrap();
        *res += 1;
        self.empty_condvar.notify_one();
    }

    /// Decrement the job count.
    fn finish_job(&self) {
        let mut res = self.job_count.lock().unwrap();
        *res -= 1;
        self.empty_condvar.notify_one();
    }

    /// Wait until the job count becomes 0.
    ///
    /// NOTE: We can optimize this function by adding another field to `ThreadPoolInner`, but let's
    /// not care about that in this homework.
    fn wait_empty(&self) {
        let mut count  = self.job_count.lock().unwrap();
        while *count != 0 {
            count = self.empty_condvar.wait(count).unwrap();
        }
    }
}

/// Thread pool.
#[derive(Debug)]
pub struct ThreadPool {
    _workers: Vec<Worker>,
    job_sender: Option<Sender<Job>>,
    pool_inner: Arc<ThreadPoolInner>,
}

impl ThreadPool {
    /// Create a new ThreadPool with `size` threads.
    ///
    /// # Panics
    ///
    /// Panics if `size` is 0.
    pub fn new(size: usize) -> Self {
        assert!(size > 0);
        let mut workers = Vec::with_capacity(size);

        let (sender, receiver) = unbounded::<Job>();
        let arc_receiver = Arc::new(receiver);
        // let pair = Arc::new((Mutex::new(0usize), Condvar::new()));
        // let cloned: Arc<(Mutex<usize>, Condvar)> = pair.clone();
        let pool_inner = Arc::new(ThreadPoolInner{
            job_count: Mutex::new(0usize),
            empty_condvar: Condvar::new(),
        });

        for idx in 0..size {
            let receiver = arc_receiver.clone();
            let inner = pool_inner.clone();
            let thread = thread::spawn(move || loop {
                let message = receiver.recv();

                match message {
                    Ok(job) => {
                        println!("Worker {idx} got a job; executing.");
                        inner.start_job();
                        job.0();
                        inner.finish_job();
                        println!("Worker {idx} finish a job; shutting down.");
                    }
                    Err(_) => {
                        println!("Worker {idx} disconnected; shutting down.");
                        break;
                    }
                }
            });

            workers.push(
                Worker {
                    _id : idx,
                    thread: Some(thread)
                }
            )
        }

        ThreadPool{
            _workers: workers,
            job_sender: Some(sender),
            pool_inner,
        }
    }

    /// Execute a new job in the thread pool.
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let sender = self.job_sender.as_ref().unwrap();
        let job = Job(Box::new(f));

        sender.send(job).unwrap()
    }

    /// Block the current thread until all jobs in the pool have been executed.
    ///
    /// NOTE: This method has nothing to do with `JoinHandle::join`.
    pub fn join(&self) {
        self.pool_inner.wait_empty();
    }
}

impl Drop for ThreadPool {
    /// When dropped, all worker threads' `JoinHandle` must be `join`ed. If the thread panicked,
    /// then this function should panic too.
    fn drop(&mut self) {
        drop(self.job_sender.take());

        for worker in &mut self._workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
            println!("Shutting down worker {}", worker._id);
        }
    }
}

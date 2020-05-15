use std::sync::{mpsc, Arc, Mutex};
use std::thread;

/// Message type to communicate with workers. A JobMsg is either a FnOnce closure or None, which
/// signals the worker to shut down.
// type JobMsg = Option<Box<dyn FnOnce() + Send + 'static>>;
type JobMsg = Option<Box<dyn FnBox + Send + 'static>>;

trait FnBox {
    fn call_box(self: Box<Self>);
}

impl<F: FnOnce()> FnBox for F {
    fn call_box(self: Box<F>) {
        (*self)()
    }
}

/// A ThreadPool should have a sending-end of a mpsc channel (`mpsc::Sender`) and a vector of
/// `JoinHandle`s for the worker threads.
pub struct ThreadPool {
    sender: mpsc::Sender<JobMsg>,
    pub workers: Vec<thread::JoinHandle<()>>,
}

impl ThreadPool {
    /// Spin up a thread pool with `num_workers` threads. Workers should all share the same
    /// receiving end of an mpsc channel (`mpsc::Receiver`) with appropriate synchronization. Each
    /// thread should loop and (1) listen for new jobs on the channel, (2) execute received jobs,
    /// and (3) quit the loop if it receives None.
    pub fn new(num_workers: usize) -> Self {
        let (sender, receiver): (mpsc::Sender<JobMsg>, mpsc::Receiver<JobMsg>) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(num_workers);
        for _ in 0..num_workers {
            let receiver = Arc::clone(&receiver);
            let thread = thread::spawn(move || loop {
                let message = receiver.lock().unwrap().recv().unwrap();
                match message {
                    Some(job) => job.call_box(),
                    None => break,
                }
            });
            workers.push(thread);
        }
        ThreadPool { workers, sender }
    }

    /// Push a new job into the thread pool.
    pub fn execute<F>(&mut self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(job);
        self.sender.send(Some(job)).unwrap();
    }
}

impl Drop for ThreadPool {
    /// Clean up the thread pool. Send a kill message (None) to each worker, and join each worker.
    /// This function should only return when all workers have finished.
    fn drop(&mut self) {
        for _ in &mut self.workers {
            self.sender.send(None).unwrap();
        }
        for _ in 0..self.workers.len() {
            let worker = self.workers.pop().unwrap();
            worker.join().unwrap();
        }
    }
}

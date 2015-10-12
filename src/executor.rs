use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::Condvar;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicUsize;
use std::thread;

pub struct ExecutorTask<F: Send + 'static> {
    body: Box<F>,
}

impl<F: FnOnce() + Send + 'static> ExecutorTask<F> {
    pub fn new(body: Box<F>) -> ExecutorTask<F> {
        ExecutorTask {
            body: body
        }
    }
}


pub struct ForkJoinWorker<T: Send + 'static> {
    id: u64,
    work: Mutex<VecDeque<ExecutorTask<T>>>,
    pool: Arc<ForkJoinPool<T>>
}

pub struct ForkJoinExecCtx<T: Send + 'static> {
    threads: Vec<ForkJoinWorker<T>>,
}

pub struct ForkJoinPool<T: Send + 'static> {
    workers: RwLock<Vec<Arc<ForkJoinWorker<T>>>>,
    supervisor: Arc<(Mutex<bool>, Condvar)>,
    workerThread: RwLock<Option<thread::Thread>>,
    dispatchIdx: AtomicUsize
}

impl<T: FnOnce() + Send + 'static> ForkJoinWorker<T> {
    fn new(id: u64, pool: Arc<ForkJoinPool<T>>) -> ForkJoinWorker<T> {
        let work = VecDeque::with_capacity(128);
        ForkJoinWorker {
            id: id,
            work: Mutex::new(work),
            pool: pool
        }
    }
    
    // TODO - return result
    fn start(worker: Arc<ForkJoinWorker<T>>) -> () {
        let w = worker.clone();
        let s = worker.pool.supervisor.clone();
        thread::spawn(move || {
            let &(ref lock, ref cvar) = &*s;
            let mut running = lock.lock().unwrap();
            while *running {
                println!("le waiting!");
                running = cvar.wait(running).unwrap();
                println!("le run!");
            }
        });
    }
}

impl<T: FnOnce() + Send + 'static> ForkJoinPool<T> {
    pub fn new(workerCount: u64) -> Arc<ForkJoinPool<T>> {
        // in order to facilitate workSteal, workers need access to the pool
        // ForkJoinPool needs strong reference to workers
        // Workers need a strong reference to ForkJoinPool
        // Let's just have the whole thing shut down 

        let mut pool = Arc::new(ForkJoinPool {
            workerThread: RwLock::new(None),
            workers: RwLock::new(vec![]), // workers can dynamically change size / replace themselves
            supervisor: Arc::new((Mutex::new(true), Condvar::new())),
            dispatchIdx: AtomicUsize::new(0)
        });

        {
            let mut workers = pool.workers.write().unwrap();
            for id in 0..workerCount {
                let worker = Arc::new(ForkJoinWorker::new(id, pool.clone()));
                ForkJoinWorker::start(worker.clone());
                workers.push(worker);
            }
        }
        pool
    }
}

pub trait Executor<T: Send + 'static> {
    fn execute(&self, task: ExecutorTask<T>) -> ();
}

impl<T: FnOnce() + Send + 'static> Executor<T> for Arc<ForkJoinPool<T>> {
    fn execute(&self, task: ExecutorTask<T>) -> () {
        let s = self.supervisor.clone();
        let &(ref num, ref cvar) = &*s;
        cvar.notify_one();
        let mut f = task.body;
        f();
    }
}

#[cfg(test)]
mod tests {
    use Promise;
    use ExecutorTask;
    use ForkJoinPool;
    use Executor;
    use std::thread;
    #[test]
    fn test_executor() {
        let (pt,pr) = Promise::new();
        let ec = ForkJoinPool::new(4);
        let task = ExecutorTask::new(Box::new(move || {
            pt.deliver(true);
        }));
        ec.execute(task);
        
        pr.wait().unwrap();
        thread::sleep_ms(3000);
        println!("get = {:?}", pr.get());
    }
}

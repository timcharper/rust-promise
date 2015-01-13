extern crate alloc;

use alloc::arc::strong_count;
use std::sync::{Arc, Mutex, Condvar};
use latch::Latch;
 

pub struct Promise<T> {
    pub data: Arc<(Mutex<Option<T>>, Condvar)>,
    pub latch: Latch,
}

impl<T: Sync+Send> Promise<T> {
    pub fn new () -> Promise<T> {
        Promise {data: Arc::new((Mutex::new(None), Condvar::new())), latch: Latch::new()}
    }
    pub fn deliver (&self, d:T) -> bool {
        if self.latch.close() {
            let &(ref lock, ref cond) = &*self.data; 
            let mut data = lock.lock().unwrap();
             
            *data = Some(d);
            cond.notify_all(); //wake up others

            return true
        }
        
        return false
    }
 
    //potentially blocking for other readers if fn applied is cpu/disk intensive
    pub fn apply<F> (&self, f: F) -> Result<T,String> 
        where F: Fn(&T) -> T {
        let &(ref lock, ref cond) = &*self.data;
        let mut v = lock.lock().unwrap();
            
        if !self.latch.latched() { 
            if strong_count(&self.data) < 2 { return Err("safety hatch, promise not capable".to_string()) }
            v = cond.wait(v).unwrap();
        }
        
        
        match *v {
            Some(ref r) => Ok(f(r)),
            None => Err("promise signaled early, value not present!".to_string()),
        }
    }
 
    pub fn clone (&self) -> Promise<T> {
        Promise {data: self.data.clone(),
                 latch: self.latch.clone()}
    }
 
    pub fn destroy (&self) -> Result<String,String> {
        if self.latch.close() {
            let &(ref lock, ref cond) = &*self.data;
            let mut data = lock.lock().unwrap();
            *data = None;
            cond.notify_all(); //wake up others

            Ok("Promise signaled early".to_string())
        }
        else { Err("promise already delivered".to_string()) }
    }
}

#[unsafe_destructor]
/// Special Drop for Promise
/// we don't want to hang readers on a local panic
impl<T: Sync+Send> Drop for Promise<T> {
    fn drop (&mut self) {
        if strong_count(&self.data) < 3 { self.destroy(); }
    }
}

#[cfg(test)]
mod tests {
    extern crate test;
    use Promise;
    use std::sync::mpsc::channel;
    use std::thread::Thread;

    #[test]
    fn test_promise_linear() {
        let p: Promise<u8> = Promise::new();
        assert_eq!(p.deliver(1),true);
        assert_eq!(p.deliver(2),false);
        assert_eq!(p.apply(|x| *x).unwrap(),1);
    }

    #[test]
    fn test_promise_threaded() {
        let p: Promise<u8> = Promise::new();
        let p2 = p.clone();
        Thread::spawn(move || {
            assert_eq!(p2.deliver(1),true);
        });
        assert_eq!(p.apply(|x| *x).unwrap(),1); //waits on spawned thread
    }

    #[test]
    #[should_fail]
    fn test_promise_threaded_destroyed() {
        let p: Promise<u8> = Promise::new();
        let p2 = p.clone();
        Thread::spawn(move || {
            p2.destroy();
        });
        p.apply(|x| *x).unwrap();
    }

    #[test]
    #[should_fail]
    fn test_promise_threaded_panic_safely() {
        let p: Promise<u8> = Promise::new();
        let p2 = p.clone();

        Thread::spawn (move || {
            panic!("proc dead"); //destroys promise, triggers wake on main proc
            p2.deliver(1);
        });
        
        p.apply(|x| *x).unwrap();
    }


    #[bench]
    fn bench_promise_linear(b: &mut test::Bencher) {
        b.iter(|| {
            let p: Promise<u8> = Promise::new();
            p.deliver(1);
            p.apply(|x| *x).unwrap();
        });
    }

    #[bench]
    fn bench_channel_linear(b: &mut test::Bencher) {
        b.iter(|| {
            let (cs,cr) = channel::<u8>();
            cs.send(1);
            cr.recv();
        });
    }
}



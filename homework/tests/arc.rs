use cs431_homework::test::loom::sync::atomic::AtomicUsize;
use cs431_homework::test::loom::sync::atomic::Ordering::Relaxed;

/// Used for testing if `T` of `Arc<T>` is dropped exactly once.
struct Canary(*const AtomicUsize);

unsafe impl Send for Canary {}
unsafe impl Sync for Canary {}

impl Drop for Canary {
    fn drop(&mut self) {
        let _ = unsafe { (*self.0).fetch_add(1, Relaxed) };
    }
}

#[cfg(not(feature = "check-loom"))]
mod basic {
    use cs431_homework::test::loom::sync::atomic::AtomicUsize;
    use cs431_homework::test::loom::sync::atomic::Ordering::Relaxed;
    use cs431_homework::test::loom::sync::mpsc::channel;
    use cs431_homework::test::loom::thread;
    use cs431_homework::Arc;

    use super::Canary;

    #[test]
    fn manually_share_arc() {
        let v = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let arc_v = Arc::new(v);

        let (tx, rx) = channel();

        let _t = thread::spawn(move || {
            let arc_v: Arc<Vec<i32>> = rx.recv().unwrap();
            assert_eq!((*arc_v)[3], 4);
        });

        tx.send(arc_v.clone()).unwrap();

        assert_eq!((*arc_v)[2], 3);
        assert_eq!((*arc_v)[4], 5);
    }

    #[test]
    fn test_cowarc_clone_make_mut() {
        let mut cow0 = Arc::new(75);
        let mut cow1 = cow0.clone();
        let mut cow2 = cow1.clone();

        assert!(75 == *Arc::make_mut(&mut cow0));
        assert!(75 == *Arc::make_mut(&mut cow1));
        assert!(75 == *Arc::make_mut(&mut cow2));

        *Arc::make_mut(&mut cow0) += 1;
        *Arc::make_mut(&mut cow1) += 2;
        *Arc::make_mut(&mut cow2) += 3;

        assert!(76 == *cow0);
        assert!(77 == *cow1);
        assert!(78 == *cow2);

        // none should point to the same backing memory
        assert!(*cow0 != *cow1);
        assert!(*cow0 != *cow2);
        assert!(*cow1 != *cow2);
    }

    #[test]
    fn test_cowarc_clone_unique2() {
        let mut cow0 = Arc::new(75);
        let cow1 = cow0.clone();
        let cow2 = cow1.clone();

        assert!(75 == *cow0);
        assert!(75 == *cow1);
        assert!(75 == *cow2);

        *Arc::make_mut(&mut cow0) += 1;
        assert!(76 == *cow0);
        assert!(75 == *cow1);
        assert!(75 == *cow2);

        // cow1 and cow2 should share the same contents
        // cow0 should have a unique reference
        assert!(*cow0 != *cow1);
        assert!(*cow0 != *cow2);
        assert!(*cow1 == *cow2);
    }

    #[test]
    fn drop_arc() {
        let canary = AtomicUsize::new(0);
        let x = Arc::new(Canary(&canary));
        let y = x.clone();
        drop(x);
        drop(y);
        assert!(canary.load(Relaxed) == 1);
    }

    #[test]
    fn test_count() {
        let a = Arc::new(0);
        assert!(Arc::count(&a) == 1);
        let b = a.clone();
        assert!(Arc::count(&a) == 2);
        assert!(Arc::count(&b) == 2);
    }

    #[test]
    fn test_ptr_eq() {
        let five = Arc::new(5);
        let same_five = five.clone();
        let other_five = Arc::new(5);

        assert!(Arc::ptr_eq(&five, &same_five));
        assert!(!Arc::ptr_eq(&five, &other_five));
    }

    #[test]
    fn test_try_unwrap_drop_once() {
        let canary = AtomicUsize::new(0);
        let x = Arc::new(Canary(&canary));
        drop(Arc::try_unwrap(x));
        assert!(canary.load(Relaxed) == 1);
    }

    #[test]
    fn test_try_make_mut_count() {
        let mut data1 = Arc::new(5);
        let mut data2 = Arc::clone(&data1); // Won't clone inner data
        let mut data3 = Arc::clone(&data1);
        assert_eq!(Arc::count(&data1), 3);
        assert_eq!(Arc::count(&data2), 3);
        assert_eq!(Arc::count(&data3), 3);
        *Arc::make_mut(&mut data1) += 1;
        assert_eq!(Arc::count(&data1), 1);
        assert_eq!(Arc::count(&data2), 2);
        assert_eq!(Arc::count(&data3), 2);
        *Arc::make_mut(&mut data2) *= 2; // clone
        assert_eq!(Arc::count(&data1), 1);
        assert_eq!(Arc::count(&data2), 1);
        assert_eq!(Arc::count(&data3), 1);
        *Arc::make_mut(&mut data3) += 1; // clone
        assert_eq!(Arc::count(&data1), 1);
        assert_eq!(Arc::count(&data2), 1);
        assert_eq!(Arc::count(&data3), 1);
    }

    #[test]
    fn test_stress() {
        let count = Arc::new(AtomicUsize::new(0));
        let handles = (0..8)
            .map(|_| {
                let count = count.clone();
                thread::spawn(move || {
                    for _ in 0..128 {
                        let _ = count.fetch_add(1, Relaxed);
                    }
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(count.load(Relaxed), 8 * 128);
    }
}

mod correctness {
    use cs431_homework::test::loom::sync::atomic::AtomicUsize;
    use cs431_homework::test::loom::sync::atomic::Ordering::Relaxed;
    use cs431_homework::test::loom::{model, thread};
    use cs431_homework::Arc;

    use super::Canary;

    #[test]
    /// data:=123 → flag.count:=1 → flag.count==1 → data==123
    fn count_sync() {
        model(|| {
            let flag = Arc::new(());
            let data = Arc::new(AtomicUsize::new(0));
            {
                let flag = flag.clone();
                let data = data.clone();
                let _ = thread::spawn(move || {
                    data.store(123, Relaxed);
                    drop(flag)
                });
            }
            if Arc::count(&flag) == 1 {
                assert_eq!(data.load(Relaxed), 123)
            }
        })
    }

    #[test]
    /// value:=123 → count:=1 → get_mut success
    fn get_mut_sync() {
        model(|| {
            let mut value = Arc::new(AtomicUsize::new(0));
            {
                let value = value.clone();
                let _ = thread::spawn(move || {
                    value.store(123, Relaxed);
                });
            }
            if let Some(val) = Arc::get_mut(&mut value) {
                assert_eq!(val.load(Relaxed), 123);
            }
        })
    }

    #[test]
    /// value:=123 → count:=1 → try_unwrap success
    fn try_unwrap_sync() {
        model(|| {
            let value = Arc::new(AtomicUsize::new(0));
            {
                let value = value.clone();
                let _ = thread::spawn(move || {
                    value.store(123, Relaxed);
                });
            }
            if let Ok(val) = Arc::try_unwrap(value) {
                assert_eq!(val.load(Relaxed), 123);
            }
        })
    }

    #[test]
    /// accesses → last drop → data drop/dealloc
    fn drop_sync() {
        struct Counter(AtomicUsize);

        impl Drop for Counter {
            fn drop(&mut self) {
                assert_eq!(self.0.load(Relaxed), 2);
            }
        }

        model(|| {
            let arc1 = Arc::new(Counter(AtomicUsize::new(0)));
            let arc2 = arc1.clone();
            let _ = thread::spawn(move || {
                let _ = arc1.0.fetch_add(1, Relaxed);
            });
            let _ = arc2.0.fetch_add(1, Relaxed);
        })
    }

    #[test]
    /// Resistance against arbitrary interleaving of instructions in `clone` and `drop`.
    fn clone_drop_atomic() {
        model(|| {
            let canary = AtomicUsize::new(0);
            let arc1 = Arc::new(Canary(&canary));
            let arc2 = arc1.clone();
            let handle = thread::spawn(move || {
                drop(arc1.clone());
                drop(arc1);
            });
            drop(arc2.clone());
            drop(arc2);
            handle.join().unwrap();
            assert_eq!(canary.load(Relaxed), 1);
        })
    }
}

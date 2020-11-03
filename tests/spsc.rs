use std::sync::atomic::{AtomicUsize, Ordering};

use crossbeam_utils::thread::scope;
use rand::{thread_rng, Rng};

use rtrb::{ChunkError, RingBuffer};

#[test]
fn smoke() {
    let (mut p, mut c) = RingBuffer::new(1).split();

    p.push(7).unwrap();
    assert_eq!(c.pop(), Ok(7));

    p.push(8).unwrap();
    assert_eq!(c.pop(), Ok(8));
    assert!(c.pop().is_err());
}

#[test]
fn capacity() {
    for i in 1..10 {
        let (p, c) = RingBuffer::<i32>::new(i).split();
        assert_eq!(p.capacity(), i);
        assert_eq!(c.capacity(), i);
    }
}

#[test]
fn zero_capacity() {
    let (mut p, mut c) = RingBuffer::<i32>::new(0).split();

    assert_eq!(p.slots(), 0);
    assert_eq!(c.slots(), 0);

    assert!(p.is_full());
    assert!(c.is_empty());

    assert!(p.push(10).is_err());
    assert!(c.pop().is_err());

    assert_eq!(p.write_chunk(1).unwrap_err(), ChunkError::TooFewSlots(0));
    assert_eq!(c.read_chunk(1).unwrap_err(), ChunkError::TooFewSlots(0));

    if let Ok(mut chunk) = p.write_chunk(0) {
        let (first, second) = chunk.as_mut_slices();
        assert!(first.is_empty());
        assert!(second.is_empty());
        chunk.commit_all();
    } else {
        unreachable!();
    }

    if let Ok(chunk) = c.read_chunk(0) {
        let (first, second) = chunk.as_slices();
        assert!(first.is_empty());
        assert!(second.is_empty());
        chunk.commit_all();
    } else {
        unreachable!();
    }
}

#[test]
fn parallel() {
    const COUNT: usize = 100_000;

    let (mut p, mut c) = RingBuffer::new(3).split();

    scope(|s| {
        s.spawn(move |_| {
            for i in 0..COUNT {
                loop {
                    if let Ok(x) = c.pop() {
                        assert_eq!(x, i);
                        break;
                    }
                }
            }
            assert!(c.pop().is_err());
        });

        s.spawn(move |_| {
            for i in 0..COUNT {
                while p.push(i).is_err() {}
            }
        });
    })
    .unwrap();
}

#[test]
fn drops() {
    const RUNS: usize = 100;

    static DROPS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug, PartialEq)]
    struct DropCounter;

    impl Drop for DropCounter {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    let mut rng = thread_rng();

    for _ in 0..RUNS {
        let steps = rng.gen_range(0, 10_000);
        let additional = rng.gen_range(0, 50);

        DROPS.store(0, Ordering::SeqCst);
        let (mut p, mut c) = RingBuffer::new(50).split();

        let mut p = scope(|s| {
            s.spawn(move |_| {
                for _ in 0..steps {
                    while c.pop().is_err() {}
                }
            });

            s.spawn(move |_| {
                for _ in 0..steps {
                    while p.push(DropCounter).is_err() {
                        DROPS.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                p
            })
            .join()
            .unwrap()
        })
        .unwrap();

        for _ in 0..additional {
            p.push(DropCounter).unwrap();
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), steps);
        drop(p);
        assert_eq!(DROPS.load(Ordering::SeqCst), steps + additional);
    }
}

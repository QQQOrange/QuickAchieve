//! 后台任务调度（v2.23.115）。
//!
//! 统一两类任务：
//!   · `spawn`：短任务立刻进工作池，不再每次 `std::thread::spawn` 起一条线程；
//!   · `spawn_later`：延迟任务交给**一条**定时线程，到期后再进工作池。
//!
//! 只接"回调很短"的任务（show/focus/emit/log 之类）。会阻塞几百毫秒以上的
//! 网络 / 磁盘 / 等待任务仍走各自的专用线程 —— 塞进 2 个工作池会被互相拖死。

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::{mpsc, Condvar, Mutex};
use std::time::{Duration, Instant};

type Job = Box<dyn FnOnce() + Send + 'static>;

struct TimedJob {
    at: Instant,
    seq: u64,
    job: Job,
}

impl PartialEq for TimedJob {
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at && self.seq == other.seq
    }
}
impl Eq for TimedJob {}
impl PartialOrd for TimedJob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TimedJob {
    fn cmp(&self, other: &Self) -> Ordering {
        // 反向：BinaryHeap 是大顶堆，这里要最近到期的先出
        other
            .at
            .cmp(&self.at)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

struct Timer {
    heap: Mutex<BinaryHeap<TimedJob>>,
    cv: Condvar,
    tx: mpsc::Sender<Job>,
}

/// 工作池队列。首次使用时启动 2 条命名线程 + 1 条定时线程。
fn pool() -> &'static mpsc::Sender<Job> {
    static POOL: std::sync::OnceLock<mpsc::Sender<Job>> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = std::sync::Arc::new(Mutex::new(rx));
        for i in 0..2 {
            let rx = rx.clone();
            let _ = std::thread::Builder::new()
                .name(format!("qs-task-{i}"))
                .spawn(move || loop {
                    let job = match rx.lock() {
                        Ok(g) => g.recv(),
                        Err(_) => return,
                    };
                    match job {
                        Ok(job) => {
                            // 单个任务 panic 不许把工作线程带走
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                        }
                        Err(_) => return,
                    }
                });
        }
        tx
    })
}

/// 立刻把一个短任务丢进工作池。
pub(crate) fn spawn<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    let _ = pool().send(Box::new(f));
}

/// 延迟执行：一条定时线程负责所有延迟任务，到期后交给工作池。
pub(crate) fn spawn_later<F>(delay: Duration, f: F)
where
    F: FnOnce() + Send + 'static,
{
    static TIMER: std::sync::OnceLock<Timer> = std::sync::OnceLock::new();
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let timer = TIMER.get_or_init(|| Timer {
        heap: Mutex::new(BinaryHeap::new()),
        cv: Condvar::new(),
        tx: pool().clone(),
    });
    // ⚠ 线程必须在 TIMER 初始化**之后**再起：否则它可能在
    //   `get_or_init` 还没返回时就去读 static，panic。
    if !STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let _ = std::thread::Builder::new()
            .name("qs-timer".into())
            .spawn(|| timer_loop(TIMER.get().expect("timer 未初始化")));
    }
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut heap) = timer.heap.lock() {
        heap.push(TimedJob {
            at: Instant::now() + delay,
            seq,
            job: Box::new(f),
        });
        timer.cv.notify_one();
    }
}

/// 定时线程主体：只做"等 → 取 → 投递"，不执行任务本身。
fn timer_loop(timer: &'static Timer) {
    loop {
        let next = {
            let Ok(mut heap) = timer.heap.lock() else { return };
            loop {
                let now = Instant::now();
                match heap.peek() {
                    None => {
                        let Ok(g) = timer.cv.wait(heap) else { return };
                        heap = g;
                    }
                    Some(j) if j.at <= now => break heap.pop(),
                    Some(j) => {
                        let wait = j.at.saturating_duration_since(now);
                        let Ok(g) = timer.cv.wait_timeout(heap, wait) else {
                            return;
                        };
                        heap = g.0;
                    }
                }
            }
        };
        if let Some(j) = next {
            let _ = timer.tx.send(j.job);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn spawn_later_runs_after_delay() {
        let (tx, rx) = mpsc::channel();
        let t0 = Instant::now();
        spawn_later(Duration::from_millis(40), move || {
            let _ = tx.send(t0.elapsed());
        });
        let elapsed = rx.recv_timeout(Duration::from_secs(2)).expect("任务没跑");
        assert!(elapsed >= Duration::from_millis(30), "跑得太早：{elapsed:?}");
    }

    #[test]
    fn spawn_runs_job() {
        let (tx, rx) = mpsc::channel();
        spawn(move || {
            let _ = tx.send(7);
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 7);
    }
}

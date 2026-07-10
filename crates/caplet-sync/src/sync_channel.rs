//! Synchronous (bounded) channel implementation

use crate::spin::SpinLock;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// A bounded synchronous channel
pub struct SyncChannel<T> {
    inner: Arc<SyncChannelInner<T>>,
}

struct SyncChannelInner<T> {
    queue: SpinLock<VecDeque<T>>,
    capacity: usize,
    sender_closed: AtomicBool,
    receiver_closed: AtomicBool,
    pending_senders: AtomicUsize,
}

impl<T> SyncChannel<T> {
    /// Create a new sync channel with the given capacity
    pub fn new(capacity: usize) -> (SyncSender<T>, SyncReceiver<T>) {
        let channel = Arc::new(SyncChannelInner {
            queue: SpinLock::new(VecDeque::with_capacity(capacity)),
            capacity,
            sender_closed: AtomicBool::new(false),
            receiver_closed: AtomicBool::new(false),
            pending_senders: AtomicUsize::new(0),
        });

        (
            SyncSender { inner: Arc::clone(&channel) },
            SyncReceiver { inner: channel },
        )
    }
}

/// Sender side of a sync channel
pub struct SyncSender<T> {
    inner: Arc<SyncChannelInner<T>>,
}

/// Receiver side of a sync channel
pub struct SyncReceiver<T> {
    inner: Arc<SyncChannelInner<T>>,
}

/// Send error
pub struct SendError<T>(pub T);

/// Try send error
#[derive(Debug)]
pub enum TrySendError<T> {
    /// Channel is full
    Full(T),
    /// Receiver was dropped
    Disconnected(T),
}

/// Recv error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvError {
    /// All senders have been dropped
    Disconnected,
}

/// Try recv error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryRecvError {
    /// Channel is empty
    Empty,
    /// All senders have been dropped
    Disconnected,
}

/// Create a new sync channel
pub fn sync_channel<T>(capacity: usize) -> (SyncSender<T>, SyncReceiver<T>) {
    SyncChannel::new(capacity)
}

impl<T> SyncSender<T> {
    /// Send a value (blocks until space is available)
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        if self.inner.receiver_closed.load(Ordering::Acquire) {
            return Err(SendError(value));
        }

        self.inner.pending_senders.fetch_add(1, Ordering::Acquire);

        loop {
            {
                let mut queue = self.inner.queue.lock();
                if queue.len() < self.inner.capacity {
                    queue.push_back(value);
                    self.inner.pending_senders.fetch_sub(1, Ordering::Release);
                    return Ok(());
                }
            }
            core::hint::spin_loop();
        }
    }

    /// Try to send without blocking
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        if self.inner.receiver_closed.load(Ordering::Acquire) {
            return Err(TrySendError::Disconnected(value));
        }

        let mut queue = self.inner.queue.lock();
        if queue.len() < self.inner.capacity {
            queue.push_back(value);
            Ok(())
        } else {
            Err(TrySendError::Full(value))
        }
    }
}

impl<T> Clone for SyncSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Drop for SyncSender<T> {
    fn drop(&mut self) {
        self.inner.sender_closed.store(true, Ordering::Release);
    }
}

impl<T> SyncReceiver<T> {
    /// Receive a value (blocks until one is available)
    pub fn recv(&self) -> Result<T, RecvError> {
        loop {
            {
                let mut queue = self.inner.queue.lock();
                if let Some(value) = queue.pop_front() {
                    return Ok(value);
                }
                if self.inner.sender_closed.load(Ordering::Acquire)
                    && self.inner.pending_senders.load(Ordering::Acquire) == 0
                {
                    return Err(RecvError::Disconnected);
                }
            }
            core::hint::spin_loop();
        }
    }

    /// Try to receive without blocking
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut queue = self.inner.queue.lock();
        match queue.pop_front() {
            Some(value) => Ok(value),
            None => {
                if self.inner.sender_closed.load(Ordering::Acquire) {
                    Err(TryRecvError::Disconnected)
                } else {
                    Err(TryRecvError::Empty)
                }
            }
        }
    }
}

impl<T> Drop for SyncReceiver<T> {
    fn drop(&mut self) {
        self.inner.receiver_closed.store(true, Ordering::Release);
    }
}
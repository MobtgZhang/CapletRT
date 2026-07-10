//! Async channel implementation

use crate::spin::SpinLock;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

/// Async channel (multiple producer, single consumer)
pub struct Channel<T> {
    inner: Arc<ChannelInner<T>>,
}

/// Inner state of a channel
struct ChannelInner<T> {
    queue: SpinLock<VecDeque<T>>,
    sender_closed: AtomicBool,
    receiver_closed: AtomicBool,
}

impl<T> Channel<T> {
    /// Create a new bounded channel with the given capacity
    pub fn bounded(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let channel = Arc::new(ChannelInner {
            queue: SpinLock::new(VecDeque::with_capacity(capacity)),
            sender_closed: AtomicBool::new(false),
            receiver_closed: AtomicBool::new(false),
        });

        let sender = Sender {
            inner: Arc::clone(&channel),
        };
        let receiver = Receiver {
            inner: channel,
        };
        (sender, receiver)
    }

    /// Create an unbounded channel
    pub fn unbounded() -> (Sender<T>, Receiver<T>) {
        Self::bounded(usize::MAX)
    }
}

/// Sender side of a channel
pub struct Sender<T> {
    inner: Arc<ChannelInner<T>>,
}

impl<T> Sender<T> {
    /// Send a value through the channel
    ///
    /// Returns Err if the receiver has been dropped
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        if self.inner.receiver_closed.load(Ordering::Acquire) {
            return Err(SendError(value));
        }

        let mut queue = self.inner.queue.lock();
        queue.push_back(value);
        Ok(())
    }

    /// Try to send without blocking
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        if self.inner.receiver_closed.load(Ordering::Acquire) {
            return Err(TrySendError::Disconnected(value));
        }
        let mut queue = self.inner.queue.lock();
        queue.push_back(value);
        Ok(())
    }

    /// Check if the receiver is still alive
    pub fn is_connected(&self) -> bool {
        !self.inner.receiver_closed.load(Ordering::Acquire)
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.inner.sender_closed.store(true, Ordering::Release);
    }
}

/// Receiver side of a channel
pub struct Receiver<T> {
    inner: Arc<ChannelInner<T>>,
}

impl<T> Receiver<T> {
    /// Receive a value from the channel
    pub fn recv(&self) -> Result<T, RecvError> {
        loop {
            {
                let mut queue = self.inner.queue.lock();
                if let Some(value) = queue.pop_front() {
                    return Ok(value);
                }
                if self.inner.sender_closed.load(Ordering::Acquire) {
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

    /// Check if any value is available
    pub fn can_recv(&self) -> bool {
        !self.inner.queue.lock().is_empty()
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.inner.receiver_closed.store(true, Ordering::Release);
    }
}

/// Error returned when send fails because receiver was dropped
pub struct SendError<T>(pub T);

impl<T> core::fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SendError").finish()
    }
}

/// Error returned when try_send fails
#[derive(Debug)]
pub enum TrySendError<T> {
    /// Channel is full
    Full(T),
    /// Receiver was dropped
    Disconnected(T),
}

/// Error returned when recv fails
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvError {
    /// All senders have been dropped
    Disconnected,
}

/// Error returned when try_recv fails
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryRecvError {
    /// Channel is empty
    Empty,
    /// All senders have been dropped
    Disconnected,
}

/// Create a new unbounded channel
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    Channel::unbounded()
}
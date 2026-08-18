use std::collections::VecDeque;
use std::sync::Mutex;

pub struct ExecutionQueue<T> {
    queue: Mutex<VecDeque<T>>,
}

impl<T> ExecutionQueue<T> {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push(&self, item: T) {
        crate::lock::lock_ok(self.queue.lock()).push_back(item);
    }

    pub fn pop(&self) -> Option<T> {
        crate::lock::lock_ok(self.queue.lock()).pop_front()
    }

    pub fn peek(&self) -> Option<T>
    where
        T: Clone,
    {
        crate::lock::lock_ok(self.queue.lock()).front().cloned()
    }

    pub fn len(&self) -> usize {
        crate::lock::lock_ok(self.queue.lock()).len()
    }

    pub fn is_empty(&self) -> bool {
        crate::lock::lock_ok(self.queue.lock()).is_empty()
    }

    pub fn clear(&self) {
        crate::lock::lock_ok(self.queue.lock()).clear();
    }

    pub fn drain(&self) -> Vec<T> {
        crate::lock::lock_ok(self.queue.lock()).drain(..).collect()
    }
}

impl<T> Default for ExecutionQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_pop() {
        let queue = ExecutionQueue::new();
        assert!(queue.is_empty());

        queue.push(1);
        queue.push(2);
        queue.push(3);
        assert_eq!(queue.len(), 3);

        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), Some(3));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_pop_empty() {
        let queue: ExecutionQueue<i32> = ExecutionQueue::new();
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn test_peek() {
        let queue = ExecutionQueue::new();
        assert!(queue.peek().is_none());

        queue.push(42);
        assert_eq!(queue.peek(), Some(42));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_clear() {
        let queue = ExecutionQueue::new();
        queue.push(1);
        queue.push(2);
        queue.push(3);
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_drain() {
        let queue = ExecutionQueue::new();
        queue.push(10);
        queue.push(20);
        queue.push(30);

        let items = queue.drain();
        assert_eq!(items, vec![10, 20, 30]);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_fifo_order() {
        let queue = ExecutionQueue::new();
        for i in 0..100 {
            queue.push(i);
        }
        for i in 0..100 {
            assert_eq!(queue.pop(), Some(i));
        }
    }
}

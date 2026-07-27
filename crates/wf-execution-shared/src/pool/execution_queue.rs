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
        self.queue.lock().unwrap().push_back(item);
    }

    pub fn pop(&self) -> Option<T> {
        self.queue.lock().unwrap().pop_front()
    }

    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().unwrap().is_empty()
    }
}

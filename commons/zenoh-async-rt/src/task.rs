use std::fmt;

pub fn current() -> Task {
    let task = async_std::task::current();
    Task { inner: task }
}

#[derive(Debug, Clone)]
pub struct Task {
    inner: async_std::task::Task,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId {
    inner: async_std::task::TaskId,
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl Task {
    pub fn id(&self) -> TaskId {
        TaskId {
            inner: self.inner.id(),
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.inner.name()
    }
}

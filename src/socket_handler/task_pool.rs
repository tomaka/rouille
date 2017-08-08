// Copyright 2015 The tiny-http Contributors
// Copyright (c) 2017 The Rouille developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{Ordering, AtomicUsize};
use std::collections::VecDeque;
use std::time::Duration;
use std::thread;

/// Manages a collection of threads.
///
/// A new thread is created every time all the existing threads are full.
/// Any idle thread will automatically die after a few seconds.
#[derive(Clone)]
pub struct TaskPool {
    sharing: Arc<Sharing>,
}

struct Sharing {
    // list of the tasks to be done by worker threads
    todo: Mutex<VecDeque<Box<FnMut() + Send>>>,

    // condvar that will be notified whenever a task is added to `todo`
    condvar: Condvar,

    // number of total worker threads running
    active_tasks: AtomicUsize,

    // number of idle worker threads
    waiting_tasks: AtomicUsize,
}

/// Minimum number of active threads.
static MIN_THREADS: usize = 4;

struct Registration<'a> {
    nb: &'a AtomicUsize
}

impl<'a> Registration<'a> {
    fn new(nb: &'a AtomicUsize) -> Registration<'a> {
        nb.fetch_add(1, Ordering::Release);
        Registration { nb: nb }
    }
}

impl<'a> Drop for Registration<'a> {
    fn drop(&mut self) {
        self.nb.fetch_sub(1, Ordering::Release);
    }
}

impl TaskPool {
    pub fn new() -> TaskPool {
        let pool = TaskPool {
            sharing: Arc::new(Sharing {
                todo: Mutex::new(VecDeque::new()),
                condvar: Condvar::new(),
                active_tasks: AtomicUsize::new(0),
                waiting_tasks: AtomicUsize::new(0),
            }),
        };

        for _ in 0..MIN_THREADS {
            pool.add_thread(None)
        }

        pool
    }

    /// Executes a function in a thread.
    /// If no thread is available, spawns a new one.
    pub fn spawn(&self, code: Box<FnMut() + Send>) {
        let mut queue = self.sharing.todo.lock().unwrap();

        if self.sharing.waiting_tasks.load(Ordering::Acquire) == 0 {
            self.add_thread(Some(code));

        } else {
            queue.push_back(code);
            self.sharing.condvar.notify_one();
        }
    }

    fn add_thread(&self, initial_fn: Option<Box<FnMut() + Send>>) {
        let sharing = self.sharing.clone();

        thread::spawn(move || {
            let sharing = sharing;
            let _active_guard = Registration::new(&sharing.active_tasks);

            if initial_fn.is_some() {
                let mut f = initial_fn.unwrap();
                f();
            }

            loop {
                let mut task: Box<FnMut() + Send> = {
                    let mut todo = sharing.todo.lock().unwrap();

                    let task;
                    loop {
                        if let Some(poped_task) = todo.pop_front() {
                            task = poped_task;
                            break;
                        }
                        let _waiting_guard = Registration::new(&sharing.waiting_tasks);

                        let received = if sharing.active_tasks.load(Ordering::Acquire)
                                                <= MIN_THREADS
                        {
                            todo = sharing.condvar.wait(todo).unwrap();
                            true

                        } else {
                            let (new_lock, waitres) = sharing.condvar
                                                             .wait_timeout(todo, Duration::from_millis(5000))
                                                             .unwrap();
                            todo = new_lock;
                            !waitres.timed_out()
                        };

                        if !received && todo.is_empty() {
                            return;
                        }
                    }

                    task
                };

                task();
            }
        });
    }
}

impl Drop for TaskPool {
    fn drop(&mut self) {
        self.sharing.active_tasks.store(999999999, Ordering::Release);
        self.sharing.condvar.notify_all();
    }
}

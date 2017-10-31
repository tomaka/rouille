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

use std::sync::Arc;
use std::thread;
use crossbeam::sync::MsQueue;
use num_cpus;

/// Manages a collection of threads.
#[derive(Clone)]
pub struct TaskPool {
    sharing: Arc<Sharing>,
}

struct Sharing {
    // List of the tasks to be done by worker threads.
    //
    // If the task returns `true` then the worker thread must continue. Otherwise it must stop.
    // This feature is necessary in order to be able to stop worker threads.
    todo: MsQueue<Box<FnMut() -> bool + Send>>,
}

impl TaskPool {
    /// Initializes a new task pool.
    pub fn new() -> TaskPool {
        let pool = TaskPool {
            sharing: Arc::new(Sharing {
                todo: MsQueue::new(),
            }),
        };

        for _ in 0..num_cpus::get() {
            let sharing = pool.sharing.clone();
            thread::spawn(move || {
                loop {
                    let mut task = sharing.todo.pop();
                    if !task() {
                        break;
                    }
                }
            });
        }

        pool
    }

    /// Executes a function in a worker thread.
    #[inline]
    pub fn spawn<F>(&self, code: F)
        where F: FnOnce() + Send + 'static
    {
        let mut code = Some(code);
        self.sharing.todo.push(Box::new(move || {
            let code = code.take().unwrap();
            code();
            true
        }));
    }
}

impl Drop for Sharing {
    fn drop(&mut self) {
        for _ in 0 .. num_cpus::get() {
            self.todo.push(Box::new(|| false));
        }
    }
}

//! In-place terminal view: a small state machine with a background repaint.
//!
//! Redirected output skips the repaint loop and keeps only the state, so the
//! same model backs both the live screen and the final report.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::style::{self, Screen};

const TICK: Duration = Duration::from_millis(90);

pub struct Live<T: Send + 'static> {
    inner: Arc<Inner<T>>,
    thread: Option<JoinHandle<()>>,
}

type Render<T> = dyn Fn(&T, Screen) -> Vec<String> + Send + Sync;

struct Inner<T> {
    state: Mutex<T>,
    render: Box<Render<T>>,
    stop: AtomicBool,
    drawn: AtomicUsize,
}

pub struct Handle<T> {
    inner: Arc<Inner<T>>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Send + 'static> Handle<T> {
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        change(&mut state);
    }

    pub fn view(&self) -> T
    where
        T: Clone,
    {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
}

impl<T: Send + 'static> Live<T> {
    pub fn start(
        initial: T,
        render: impl Fn(&T, Screen) -> Vec<String> + Send + Sync + 'static,
    ) -> Self {
        let inner = Arc::new(Inner {
            state: Mutex::new(initial),
            render: Box::new(render),
            stop: AtomicBool::new(false),
            drawn: AtomicUsize::new(0),
        });

        let thread = if style::interactive() {
            let inner = Arc::clone(&inner);
            Some(std::thread::spawn(move || {
                let mut drawn = 0;
                while !inner.stop.load(Ordering::Relaxed) {
                    drawn = paint(&inner, drawn);
                    inner.drawn.store(drawn, Ordering::Relaxed);
                    std::thread::sleep(TICK);
                }
            }))
        } else {
            None
        };

        Self { inner, thread }
    }

    pub fn handle(&self) -> Handle<T> {
        Handle {
            inner: Arc::clone(&self.inner),
        }
    }

    pub fn update(&self, change: impl FnOnce(&mut T)) {
        self.handle().update(change);
    }

    pub fn view(&self) -> T
    where
        T: Clone,
    {
        self.handle().view()
    }

    pub fn finish(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.inner.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
            erase(self.inner.drawn.swap(0, Ordering::Relaxed));
        }
    }
}

impl<T: Send + 'static> Drop for Live<T> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn paint<T>(inner: &Inner<T>, previous: usize) -> usize {
    let screen = style::screen();
    let lines = {
        let state = inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (inner.render)(&state, screen)
    };

    let (out, drawn) = frame(lines, previous, screen);
    write_out(&out);
    drawn
}

fn frame(mut lines: Vec<String>, previous: usize, screen: Screen) -> (String, usize) {
    let wrap = screen.width + 1;
    let cap = screen.height.saturating_sub(1).max(1);

    let mut kept = 0;
    let mut rows = 0;
    for line in &lines {
        let line_rows = style::rows(line, wrap);
        if kept > 0 && rows + line_rows > cap {
            break;
        }
        rows += line_rows;
        kept += 1;
    }
    lines.truncate(kept);

    let height = rows.max(previous);
    let mut out = String::new();
    if previous > 0 {
        out.push_str(&format!("\x1b[{previous}A"));
    }
    for line in &lines {
        out.push_str("\x1b[2K");
        out.push_str(line);
        out.push_str("\x1b[K");
        out.push_str("\r\n");
    }
    for _ in rows..height {
        out.push_str("\x1b[2K\r\n");
    }
    (out, height)
}

fn erase(lines: usize) {
    if lines == 0 {
        return;
    }
    let mut out = format!("\x1b[{lines}A");
    for _ in 0..lines {
        out.push_str("\x1b[2K\r\n");
    }
    out.push_str(&format!("\x1b[{lines}A"));
    write_out(&out);
}

fn write_out(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_keep_state_without_drawing() {
        let live = Live::start(0usize, |value, _| vec![value.to_string()]);
        live.update(|value| *value += 2);
        assert_eq!(live.view(), 2);
        live.finish();
    }

    fn screen(width: usize, height: usize) -> Screen {
        Screen { width, height }
    }

    #[test]
    fn wrapped_lines_take_multiple_rows() {
        let lines = vec!["x".repeat(45)];
        let (out, rows) = frame(lines, 0, screen(20, 40));
        assert_eq!(rows, 3);
        assert!(out.contains(&"x".repeat(45)));
    }

    #[test]
    fn a_shorter_frame_clears_rows_left_by_a_taller_one() {
        let lines = vec!["x".to_string()];
        let (out, rows) = frame(lines, 3, screen(20, 40));
        assert_eq!(rows, 3);
        assert!(out.starts_with("\x1b[3A"));
        assert_eq!(out.matches("\x1b[2K").count(), 3);
    }
}

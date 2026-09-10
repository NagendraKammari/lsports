use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

fn wrap(text: &str, code: &str) -> String {
    if ENABLED.load(Ordering::Relaxed) {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    wrap(text, "1")
}

pub fn dim(text: &str) -> String {
    wrap(text, "2")
}

pub fn cyan(text: &str) -> String {
    wrap(text, "36")
}

pub fn yellow(text: &str) -> String {
    wrap(text, "33")
}

pub fn green(text: &str) -> String {
    wrap(text, "32")
}

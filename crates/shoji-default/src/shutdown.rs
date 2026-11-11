//! Leaving cleanly when the shell is stopped from outside: `kill`, Ctrl+C, the session closing,
//! or a panic. Without it the keys would stay bound to their command forms and Omarchy's socket
//! would stay moved aside until the next start repaired it. (`msg quit` does the same by itself.)
//!
//! A signal handler may only do very little, so it writes one byte to a pipe and a thread does the
//! work. A kill that cannot be caught (`SIGKILL`) leaves it to whoever killed: `alter-bar` reloads
//! Hyprland's configuration, which puts the keys back.

use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::OnceLock;

static WAKE: OnceLock<i32> = OnceLock::new();

/// Call once, from the root.
pub fn install() {
    let Ok((mut reader, writer)) = UnixStream::pair() else { return };
    let _ = WAKE.set(writer.as_raw_fd());
    // The write end lives as long as the process.
    std::mem::forget(writer);
    for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        // SAFETY: the handler only calls `write`, which is async-signal-safe.
        let _ = unsafe {
            signal_hook_registry::register(signal, || {
                if let Some(fd) = WAKE.get() {
                    let _ = libc::write(*fd, b"x".as_ptr().cast(), 1);
                }
            })
        };
    }
    let _ = std::thread::Builder::new().name("shoji-bar-4 shutdown".into()).spawn(move || {
        let mut byte = [0u8; 1];
        if reader.read(&mut byte).is_ok_and(|n| n > 0) {
            crate::ipc::cleanup();
            std::process::exit(0);
        }
    });
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        crate::ipc::cleanup();
        previous(info);
    }));
}

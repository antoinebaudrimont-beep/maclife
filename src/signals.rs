use crate::DynError;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicI32, Ordering};

static SIGNAL_WRITE_FD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn signal_handler(signal: libc::c_int) {
    let fd = SIGNAL_WRITE_FD.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = signal as u8;
        // SAFETY: write is async-signal-safe; the descriptor is nonblocking and
        // remains open while the handlers are installed.
        unsafe {
            libc::write(fd, (&byte as *const u8).cast(), 1);
        }
    }
}

pub struct ShutdownSignals {
    read_fd: OwnedFd,
    write_fd: OwnedFd,
    old_term: libc::sigaction,
    old_int: libc::sigaction,
}

impl ShutdownSignals {
    pub fn install() -> Result<Self, DynError> {
        if SIGNAL_WRITE_FD.load(Ordering::SeqCst) >= 0 {
            return Err("shutdown signal handlers are already installed".into());
        }

        let mut pipe = [-1; 2];
        // SAFETY: pipe points to two writable integers.
        if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: pipe2 returned two newly owned descriptors.
        let read_fd = unsafe { OwnedFd::from_raw_fd(pipe[0]) };
        // SAFETY: pipe2 returned two newly owned descriptors.
        let write_fd = unsafe { OwnedFd::from_raw_fd(pipe[1]) };

        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = signal_handler as usize;
        // Do not use SA_RESTART: interruption should promptly wake any syscall.
        action.sa_flags = 0;
        // SAFETY: action contains a valid empty signal set after sigemptyset.
        if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let mut old_term = MaybeUninit::uninit();
        let mut old_int = MaybeUninit::uninit();
        SIGNAL_WRITE_FD.store(write_fd.as_raw_fd(), Ordering::SeqCst);
        // SAFETY: the signal numbers and sigaction pointers are valid.
        if unsafe { libc::sigaction(libc::SIGTERM, &action, old_term.as_mut_ptr()) } != 0 {
            SIGNAL_WRITE_FD.store(-1, Ordering::SeqCst);
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: same as above.
        if unsafe { libc::sigaction(libc::SIGINT, &action, old_int.as_mut_ptr()) } != 0 {
            // SAFETY: old_term was initialized by the successful call above.
            unsafe {
                libc::sigaction(libc::SIGTERM, old_term.as_ptr(), std::ptr::null_mut());
            }
            SIGNAL_WRITE_FD.store(-1, Ordering::SeqCst);
            return Err(std::io::Error::last_os_error().into());
        }

        // SAFETY: both values were initialized by successful sigaction calls.
        Ok(Self {
            read_fd,
            write_fd,
            old_term: unsafe { old_term.assume_init() },
            old_int: unsafe { old_int.assume_init() },
        })
    }

    pub fn read_fd(&self) -> RawFd {
        self.read_fd.as_raw_fd()
    }

    pub fn consume(&self) -> Result<bool, DynError> {
        let mut buffer = [0_u8; 32];
        // SAFETY: buffer is writable and read_fd is valid.
        let count = unsafe {
            libc::read(
                self.read_fd.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if count > 0 {
            return Ok(true);
        }
        if count == 0 {
            return Ok(false);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            Ok(false)
        } else {
            Err(error.into())
        }
    }
}

impl Drop for ShutdownSignals {
    fn drop(&mut self) {
        // Stop the handler from writing before restoring dispositions and closing FDs.
        SIGNAL_WRITE_FD.store(-1, Ordering::SeqCst);
        // SAFETY: saved actions were returned by sigaction for these signals.
        unsafe {
            libc::sigaction(libc::SIGTERM, &self.old_term, std::ptr::null_mut());
            libc::sigaction(libc::SIGINT, &self.old_int, std::ptr::null_mut());
        }
        let _ = self.write_fd.as_raw_fd();
    }
}

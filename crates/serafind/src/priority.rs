//! Lower priority for background work (the index), so that it does not slow the session down.

/// Lowers the CPU and disk priority of the calling thread. On Linux, threads it starts afterwards
/// (the parallel walk) get the same priorities.
pub fn lower_this_thread() {
    #[cfg(target_os = "linux")]
    {
        /// `IOPRIO_WHO_PROCESS`: a thread id.
        const WHO_THREAD: libc::c_long = 1;
        /// Best-effort class (2), lowest level (7).
        const LOWEST_BEST_EFFORT: libc::c_long = (2 << 13) | 7;
        // SAFETY: system calls that only change the priorities of the calling thread.
        #[allow(unsafe_code)]
        unsafe {
            let thread = libc::syscall(libc::SYS_gettid);
            let id = libc::id_t::try_from(thread).unwrap_or(0);
            libc::setpriority(libc::PRIO_PROCESS, id, 10);
            libc::syscall(libc::SYS_ioprio_set, WHO_THREAD, thread, LOWEST_BEST_EFFORT);
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        // SAFETY: changes the priority of the calling thread only.
        #[allow(unsafe_code)]
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn the_thread_and_its_children_are_lowered() {
        std::thread::spawn(|| {
            super::lower_this_thread();
            let nice = || {
                // SAFETY: reads the priority of the calling thread.
                #[allow(unsafe_code)]
                unsafe {
                    let thread = libc::syscall(libc::SYS_gettid);
                    libc::getpriority(libc::PRIO_PROCESS, libc::id_t::try_from(thread).unwrap())
                }
            };
            assert!(nice() >= 10);
            assert!(std::thread::spawn(nice).join().unwrap() >= 10, "inherited");
        })
        .join()
        .unwrap();
    }
}

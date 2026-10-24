//! Return freed heap pages to the OS at quiescent points.
//!
//! glibc keeps memory freed after a peak (a turn's request, tool outputs) in
//! its arenas and only trims an arena's top. Long-lived processes then hold
//! the high-water mark of every past turn. A trim after the work madvises the
//! free pages away; live allocations and their addresses are untouched.

/// Release free heap pages. Cheap relative to a turn; never call per frame.
pub fn release_free_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only inspects allocator state and has no pointer
    // arguments; it is thread-safe in glibc.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Owner-process allocator policy, measured by the memory soak: two arenas
/// (two Tokio workers plus short blocking lanes) instead of one per thread,
/// and a fixed mmap threshold so request-sized buffers (journals, request
/// bodies, tool outputs) return to the OS on free instead of raising glibc's
/// dynamic threshold and fragmenting the arenas. Must run before threads.
pub fn tune_owner_allocator() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: mallopt takes plain integers and runs before other threads.
        unsafe { libc::mallopt(libc::M_ARENA_MAX, 2) };
        // SAFETY: as above; a fixed threshold also disables dynamic raising.
        unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, 256 * 1024) };
    }
}

/// Releases free pages when a unit of work ends, including on error/cancel.
pub(crate) struct ReleaseOnDrop;

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        release_free_memory();
    }
}

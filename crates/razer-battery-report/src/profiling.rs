//! Tracy instrumentation, built in only with the `profile-with-tracy` features.

/// Call stack depth recorded for every allocation, deep enough to get past std collections into
/// app code.
#[cfg(feature = "profile-with-tracy-allocations")]
const ALLOCATION_CALLSTACK_DEPTH: u16 = 100;

#[cfg(feature = "profile-with-tracy-allocations")]
#[global_allocator]
static ALLOCATOR: tracy_client::ProfiledAllocator<std::alloc::System> =
    tracy_client::ProfiledAllocator::new(std::alloc::System, ALLOCATION_CALLSTACK_DEPTH);

/// Opens a Tracy zone named `$name` that lasts until the end of the enclosing block; expands to
/// nothing without profiling.
macro_rules! span {
    ($name:literal) => {
        #[cfg(feature = "profile-with-tracy")]
        let _span = tracy_client::span!($name);
    };
}
pub(crate) use span;

/// Starts the Tracy client. Call it first in `main`: a zone opened before it panics.
pub fn start() {
    #[cfg(feature = "profile-with-tracy")]
    tracy_client::Client::start();
}

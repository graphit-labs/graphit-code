#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "task_protocol/comments.rs"]
mod comments;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "task_protocol/native.rs"]
mod native;
#[path = "task_protocol/wire.rs"]
mod wire;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "task_protocol/session_get.rs"]
mod session_get;

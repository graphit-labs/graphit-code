//! Observe the retained parent Job; process-wrap 10.0.1 keeps its Job private.
use crate::{Cleanup, ControlFailures, GroupFailure, LeaderCleanup, UnconfirmedCleanup};
use process_wrap::tokio::{ChildWrapper, CommandWrap, CommandWrapper, JobObject};
use std::{fmt, io, os::windows::io::AsRawHandle, time::Duration};
use win32job::{ExtendedLimitInfo, Job};

#[derive(Default)]
pub(crate) struct JobObservation {
    assigned: Option<AssignedJob>,
}
struct AssignedJob(Job);
impl fmt::Debug for JobObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JobObservation")
    }
}
impl CommandWrapper for JobObservation {
    fn wrap_child(
        &mut self,
        child: Box<dyn ChildWrapper>,
        command: &CommandWrap,
    ) -> io::Result<Box<dyn ChildWrapper>> {
        // Registered before JobObject: its pre_spawn has already suspended the
        // native child, but its wrap_child has not yet assigned/resumed it.
        if !command.has_wrap::<JobObject>() || self.assigned.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let process = child
            .process_handle()
            .or_else(|| {
                child
                    .try_inner_child()
                    .and_then(ChildWrapper::process_handle)
            })
            .ok_or(io::ErrorKind::Unsupported)?;
        let mut limits = ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        // No breakaway flags or inheritable handle: descendants remain in this
        // parent Job, including the SDK's subsequently nested Job.
        let job = Job::create_with_limit_info(&limits)?;
        job.assign_process(process.as_raw_handle() as isize)?;
        // Keep the real capability in CommandWrap through later wrapper errors.
        self.assigned = Some(AssignedJob(job));
        Ok(child)
    }
}
impl AssignedJob {
    async fn empty(&self) -> io::Result<()> {
        loop {
            // Query accounting for this Job and every nested Job without
            // allocating or copying a capacity-limited list of native PIDs.
            if active_processes(&self.0)? == 0 {
                return Ok(());
            }
            // Only the existing host driver waits. Its observer retains the
            // original cleanup endpoint and can return the same RemainingProcess.
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

pub(crate) async fn cleanup(
    command: Option<&CommandWrap>,
    leader: LeaderCleanup,
    mut control: ControlFailures,
) -> Cleanup {
    let assigned = command
        .and_then(CommandWrap::get_wrap::<JobObservation>)
        .and_then(|observer| observer.assigned.as_ref());
    let Some(assigned) = assigned else {
        return Cleanup::Unconfirmed(UnconfirmedCleanup::TreeUnknownWindows { leader, control });
    };
    if let Err(error) = assigned.empty().await {
        control.group = Some(GroupFailure::Io(error.into()));
        return Cleanup::Unconfirmed(UnconfirmedCleanup::TreeUnknownWindows { leader, control });
    }
    if control != ControlFailures::default() {
        return Cleanup::ControlFailed { leader, control };
    }
    match leader {
        LeaderCleanup::Reaped => Cleanup::Complete,
        LeaderCleanup::Failed(error) => Cleanup::Failed(error),
        LeaderCleanup::TimedOut => unreachable!("native wait is not timed out"),
    }
}

#[allow(unsafe_code)]
fn active_processes(job: &Job) -> io::Result<u32> {
    use windows_sys::Win32::System::JobObjects::{
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
        QueryInformationJobObject,
    };
    let mut information = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    // SAFETY: Job owns this valid, non-inherited handle for the complete call.
    // The SDK writes exactly the selected accounting structure into its aligned
    // stack allocation; the pointer and byte count describe that same allocation.
    let result = unsafe {
        QueryInformationJobObject(
            job.handle() as _,
            JobObjectBasicAccountingInformation,
            (&mut information as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            std::mem::size_of_val(&information) as u32,
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(information.ActiveProcesses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpStream,
        process::{Command, Stdio},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn fixture_command(command: &mut Command, address: &str, member: bool) {
        command
            .args([
                "--exact",
                "windows_job::tests::native_member_fixture",
                "--nocapture",
            ])
            .env_clear()
            .envs(
                std::env::vars_os()
                    .filter(|(key, _)| key.as_encoded_bytes().eq_ignore_ascii_case(b"SYSTEMROOT")),
            )
            .env("GRAPHIT_JOB_FIXTURE_ADDRESS", address)
            .env(
                "GRAPHIT_JOB_FIXTURE_MEMBER",
                if member { "child" } else { "leader" },
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }

    #[test]
    fn native_member_fixture() {
        let Ok(address) = std::env::var("GRAPHIT_JOB_FIXTURE_ADDRESS") else {
            return;
        };
        let member = std::env::var("GRAPHIT_JOB_FIXTURE_MEMBER").unwrap() == "child";
        let mut peer = TcpStream::connect(&address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
        peer.write_all(if member { b"D" } else { b"L" }).unwrap();
        let _descendant = if member {
            None
        } else {
            let mut command = Command::new(std::env::current_exe().unwrap());
            fixture_command(&mut command, &address, true);
            Some(command.spawn().unwrap())
        };
        let mut acknowledgement = [0];
        peer.read_exact(&mut acknowledgement).unwrap();
        assert_eq!(acknowledgement, *b"A");
        std::process::exit(7);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn absent_job_capability_cannot_become_success_from_leader_status() {
        let observed = cleanup(None, LeaderCleanup::Reaped, ControlFailures::default()).await;
        assert!(matches!(
            observed,
            Cleanup::Unconfirmed(UnconfirmedCleanup::TreeUnknownWindows {
                leader: LeaderCleanup::Reaped,
                ..
            })
        ));
        assert_eq!(
            crate::supervisor::cleanup_failure(observed),
            Some(crate::Reason::Cleanup)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn retained_job_observes_descendant_after_leader_exit_then_native_zero() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let mut native = Command::new(std::env::current_exe().unwrap());
        fixture_command(&mut native, &address, false);
        let mut command: CommandWrap = tokio::process::Command::from(native).into();
        command
            .wrap(process_wrap::tokio::KillOnDrop)
            .wrap(JobObservation::default())
            .wrap(JobObject);
        let mut child = command.spawn().unwrap();
        let observer = command.get_wrap::<JobObservation>().unwrap();
        let job = observer.assigned.as_ref().unwrap();
        let (status, active, _member) = tokio::time::timeout(Duration::from_secs(5), async {
            let (mut leader, _) = listener.accept().await.unwrap();
            let mut byte = [0];
            leader.read_exact(&mut byte).await.unwrap();
            assert_eq!(byte, *b"L");
            let (mut member, _) = listener.accept().await.unwrap();
            member.read_exact(&mut byte).await.unwrap();
            assert_eq!(byte, *b"D");
            leader.write_all(b"A").await.unwrap();
            // Only the native leader is reaped; its descendant remains held by
            // the socket. Query the retained parent Job before requesting kill.
            let status = child.inner_mut().wait().await;
            (status, active_processes(&job.0), member)
        })
        .await
        .unwrap();
        let kill = child.start_kill();
        let waited = child.wait().await;
        let observed = cleanup(
            Some(&command),
            LeaderCleanup::Reaped,
            ControlFailures::default(),
        )
        .await;
        drop(child);
        drop(command);
        assert_eq!(status.unwrap().code(), Some(7));
        assert!(
            active.unwrap() > 0,
            "leader status must not prove an empty Job"
        );
        kill.unwrap();
        assert_eq!(waited.unwrap().code(), Some(7));
        assert_eq!(observed, Cleanup::Complete);
    }
}

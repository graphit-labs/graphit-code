use graphit_process::Cleanup;

#[cfg(unix)]
pub const REAPED: Cleanup = Cleanup::Complete;

#[cfg(windows)]
pub const REAPED: Cleanup =
    Cleanup::Unconfirmed(graphit_process::UnconfirmedCleanup::TreeUnknownWindows {
        leader: graphit_process::LeaderCleanup::Reaped,
        control: graphit_process::ControlFailures {
            group: None,
            terminate: None,
            kill: None,
        },
    });

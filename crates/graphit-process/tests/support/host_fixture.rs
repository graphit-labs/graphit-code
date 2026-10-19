use graphit_core::file_lock::Cancellation;
use graphit_process::{
    HostLimits, HostReadiness, HostReport, HostShutdown, HostShutdownObservation, ProcessHost,
};
use std::time::{Duration, Instant};

pub async fn install(slots: usize) -> ProcessHost {
    let limits = HostLimits::new(slots, 1).unwrap();
    let service = Cancellation::default();
    let host = ProcessHost::install(&service, limits).unwrap();
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    host
}

pub async fn joined(host: ProcessHost) -> HostReport {
    match host.shutdown(Instant::now() + Duration::from_secs(5)).await {
        HostShutdown::Joined(report) => report,
        HostShutdown::Pending(mut pending) => match pending
            .repair(Instant::now() + Duration::from_secs(5))
            .await
        {
            HostShutdownObservation::Joined(report) => report,
            observation => panic!("process host did not shut down: {observation:?}"),
        },
    }
}

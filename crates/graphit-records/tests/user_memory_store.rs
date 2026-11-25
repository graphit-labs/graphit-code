#![cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "user_memory_store/automatic.rs"]
mod automatic;
#[path = "user_memory_store/fenced.rs"]
mod fenced;
#[path = "user_memory_store/fixture.rs"]
mod fixture;
#[path = "user_memory_store/history.rs"]
mod history;
#[path = "user_memory_store/hybrid.rs"]
mod hybrid;
#[path = "user_memory_store/integrity.rs"]
mod integrity;
#[path = "user_memory_store/ordinary.rs"]
mod ordinary;
#[path = "user_memory_store/references.rs"]
mod references;
#[path = "user_memory_store/search.rs"]
mod search;

use fixture::*;
use graphit_records::memory_store::*;
use graphit_records::user_memory_store::*;

macro_rules! incoming {
    ($f:ident,$ctx:ident,$verifier:ident,$proof:ident,$read:ident,$write:ident) => {
        let $verifier = $f.server.verifier(true, $f.provider_revision(&$ctx));
        let raw = $f.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
        let $proof = $verifier
            .verify(
                &raw,
                graphit_auth::broker::BrokerRequestContext::new($ctx.snapshot()),
            )
            .await
            .unwrap();
        let $read = $f
            .issuer
            .admit_incoming_user_memory_read(&$proof, &$verifier, &$ctx, "service", $f.scope())
            .unwrap();
        let $write = $f
            .issuer
            .admit_incoming_user_memory_write(
                &$proof,
                &$verifier,
                &$ctx,
                "service",
                $f.scope(),
                graphit_auth::UserMemoryWriteOperation::Memory,
            )
            .unwrap();
    };
}
pub(crate) use incoming;

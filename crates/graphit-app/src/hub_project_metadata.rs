//! Register the captured, authorized PROJECT before exposing an artifact.
use futures_util::StreamExt;
use graphit_core::{
    project_identity::{ProjectId, ProjectIdentityStore},
    project_paths::ProjectPaths,
};
use graphit_hub::{
    policy::RequestContext,
    project_metadata::{HubProjectMetadata, project_metadata_key},
};
use object_store::{ObjectStore, ObjectStoreExt, PutMode, UpdateVersion};
use std::{fmt, io};
/// The real metadata effect survives a later artifact failure; it is not a rollback ledger.
#[derive(Debug)]
pub(crate) struct Failure {
    pub effect: &'static str,
    pub stage: &'static str,
    pub cause: io::Error,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PROJECT metadata {}: {}", self.effect, self.cause)
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
pub(crate) fn artifact_failure(effect: &'static str, cause: io::Error) -> io::Error {
    io::Error::new(
        cause.kind(),
        Failure {
            effect,
            stage: "artifact",
            cause,
        },
    )
}
pub(crate) async fn publish(
    store: &dyn ObjectStore,
    prefix: &str,
    project: (&ProjectPaths, ProjectId),
    max_bytes: usize,
    request: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<&'static str> {
    let (paths, id) = project;
    let mut effect = "not_written";
    let operation:io::Result<()>=async {
        check()?;
        let identity=ProjectIdentityStore::at(paths).load()?.ok_or(io::ErrorKind::NotFound)?;
        if identity.id()!=id{return Err(io::ErrorKind::PermissionDenied.into())}
        let metadata=HubProjectMetadata{id,name:identity.metadata().name.clone()};
        let bytes=metadata.encode(max_bytes)?;
        let key=crate::native_publication::key(prefix,&project_metadata_key(id))?;
        let mode=match crate::native_publication::guarded(request,check,store.get(&key)).await {
            Ok(got)=>{
                if got.meta.size>max_bytes as u64{return Err(io::ErrorKind::FileTooLarge.into())}
                let marker=UpdateVersion{e_tag:got.meta.e_tag.clone(),version:got.meta.version.clone()};
                let mut stream=got.into_stream();let mut current=Vec::new();
                while let Some(chunk)=crate::native_publication::guarded(request,check,async{stream.next().await.transpose()}).await? {
                    if chunk.len()>max_bytes-current.len(){return Err(io::ErrorKind::FileTooLarge.into())}
                    current.extend_from_slice(&chunk);
                }
                let current=HubProjectMetadata::decode(&current,id,max_bytes)?;
                if current==metadata {effect="unchanged";None}else {
                    if marker.e_tag.is_none()&&marker.version.is_none(){return Err(io::ErrorKind::Unsupported.into())}
                    Some(PutMode::Update(marker))
                }
            }
            Err(e) if e.get_ref().and_then(|e|e.downcast_ref::<object_store::Error>()).is_some_and(|e|matches!(e,object_store::Error::NotFound{..}))=>Some(PutMode::Create),
            Err(e)=>return Err(e),
        };
        if let Some(mode)=mode {
            check()?;effect="uncertain";
            let written=tokio::select!{
                biased;
                ()=request.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
                ()=tokio::time::sleep_until(request.snapshot().required_deadline()?.into())=>Err(io::ErrorKind::TimedOut.into()),
                result=store.put_opts(&key,bytes.into(),mode.into())=>result.map_err(io::Error::other),
            };
            if let Err(cause)=written {
                if cause.get_ref().and_then(|e|e.downcast_ref::<object_store::Error>()).is_some_and(|e|matches!(e,object_store::Error::AlreadyExists{..}|object_store::Error::Precondition{..}|object_store::Error::PermissionDenied{..}|object_store::Error::NotSupported{..})) {effect="rejected";}
                return Err(cause)
            }
            effect="published";
        }
        // Both unchanged and newly written metadata use this same final guard.
        check()?;
        if ProjectIdentityStore::at(paths).load()?.as_ref()!=Some(&identity){return Err(io::ErrorKind::Interrupted.into())}
        Ok(())
    }.await;
    operation.map(|()| effect).map_err(|cause| {
        io::Error::new(
            cause.kind(),
            Failure {
                effect,
                stage: "project_metadata",
                cause,
            },
        )
    })
}

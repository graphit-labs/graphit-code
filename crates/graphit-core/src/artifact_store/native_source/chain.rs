//! The one held-root relative acquisition loop, used by Source and PROJECT documents.
use super::*;
use crate::native_file_identity::{PhysicalId, handle_id};

pub(super) fn open_prefixes(
    admission: &RootReadAdmission,
    relative: &Path,
    kind: NodeKind,
) -> io::Result<Vec<PrefixHandle>> {
    open_prefixes_with(admission, relative, kind, &mut |parent, name, kind, _| {
        NativeHandle::open_child(parent, name, Some(kind), &admission.request)
    })
}

fn open_prefixes_with(
    admission: &RootReadAdmission,
    relative: &Path,
    kind: NodeKind,
    open: &mut impl FnMut(&File, &std::ffi::OsStr, NodeKind, bool) -> io::Result<Arc<NativeHandle>>,
) -> io::Result<Vec<PrefixHandle>> {
    let request = &admission.request;
    let components = relative.components().count();
    let mut prefixes = Vec::new();
    prefixes
        .try_reserve_exact(components)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    let mut parent = Arc::clone(&admission.anchor.root);
    for (index, component) in relative.components().enumerate() {
        let Component::Normal(name) = component else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        let last = index + 1 == components;
        let expected = if last { kind } else { NodeKind::Directory };
        let next = {
            let file = parent.lock()?;
            open(&file, name, expected, last)?
        };
        prefixes.push(PrefixHandle {
            observation: next.observe(request)?,
            handle: Arc::clone(&next),
        });
        parent = next;
    }
    drop(parent);
    Ok(prefixes)
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct DirectoryBinding {
    identity: PhysicalId,
    #[cfg(unix)]
    mode: u32,
    #[cfg(windows)]
    readonly: bool,
}
impl DirectoryBinding {
    pub(super) fn observe(
        handle: &NativeHandle,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        let file = handle.lock()?;
        let metadata =
            read_directory::inspect(&file, Some(NodeKind::Directory), &mut || request.check())?;
        request.check()?;
        let identity = handle_id(&file)?;
        identity.validate()?;
        Ok(Self {
            identity,
            #[cfg(unix)]
            mode: {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o7777
            },
            #[cfg(windows)]
            readonly: metadata.permissions().readonly(),
        })
    }
}

pub(crate) struct CapturedChain {
    prefixes: Vec<PrefixHandle>,
    bindings: Vec<DirectoryBinding>,
    selection: PathBuf,
    admission: Arc<RootReadAdmission>,
}
impl Drop for CapturedChain {
    fn drop(&mut self) {
        while let Some(prefix) = self.prefixes.pop() {
            drop(prefix);
        }
    }
}
impl CapturedChain {
    pub(crate) fn probe_user_memory(
        scope: &CapturedPublicationScope,
        selection: &Path,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<crate::native_document_file::DirectoryPresence> {
        scope.user_root()?;
        scope.validate_record_selection(selection)?;
        let mut relative = PathBuf::new();
        let mut retained = Self::capture_inner(scope, &relative, request)?;
        for component in selection.components() {
            request.check()?;
            retained.revalidate()?;
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            relative.push(name);
            match Self::capture_inner(scope, &relative, request) {
                Ok(next) => retained = next,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    retained.revalidate()?;
                    return Ok(crate::native_document_file::DirectoryPresence::Absent);
                }
                Err(error) => return Err(error),
            }
        }
        retained.revalidate()?;
        Ok(crate::native_document_file::DirectoryPresence::PresentDirectory)
    }
    pub(crate) fn capture_record(
        scope: &CapturedPublicationScope,
        relative: &Path,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Arc<Self>> {
        scope.validate_record_selection(relative)?;
        Self::capture_inner(scope, relative, request)
    }
    fn capture_inner(
        scope: &CapturedPublicationScope,
        relative: &Path,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Arc<Self>> {
        crate::file_io::validate_relative(relative, true)?;
        let admission = Arc::new(RootReadAdmission::new(scope, request)?);
        Self::capture_admitted(admission, relative)
    }
    fn capture_admitted(
        admission: Arc<RootReadAdmission>,
        relative: &Path,
    ) -> io::Result<Arc<Self>> {
        let request = &admission.request;
        let selection = relative.to_owned();
        let count = relative.components().count();

        let prefixes = open_prefixes(&admission, relative, NodeKind::Directory)?;
        let mut value = Self {
            prefixes,
            bindings: Vec::new(),
            selection,
            admission: Arc::clone(&admission),
        };
        value
            .bindings
            .try_reserve_exact(count)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        for prefix in &value.prefixes {
            value
                .bindings
                .push(DirectoryBinding::observe(&prefix.handle, request)?);
        }
        value.revalidate()?;
        Ok(Arc::new(value))
    }
    pub(crate) fn capture_runtime(
        scope: &CapturedPublicationScope,
        relative: &Path,
        request: &Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Arc<Self>> {
        let selection = scope.runtime_selection(relative)?;
        let admission = Arc::new(RootReadAdmission::runtime(scope, request)?);
        Self::capture_admitted(admission, &selection)
    }
    pub(crate) fn probe_runtime(
        scope: &CapturedPublicationScope,
        relative: &Path,
        request: &Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<crate::native_document_file::DirectoryPresence> {
        let selection = scope.runtime_selection(relative)?;
        let admission = Arc::new(RootReadAdmission::runtime(scope, request)?);
        let mut relative = PathBuf::new();
        let mut retained = Self::capture_admitted(Arc::clone(&admission), &relative)?;
        for component in selection.components() {
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            request.check()?;
            retained.revalidate()?;
            relative.push(name);
            match Self::capture_admitted(Arc::clone(&admission), &relative) {
                Ok(next) => retained = next,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    retained.revalidate()?;
                    return Ok(crate::native_document_file::DirectoryPresence::Absent);
                }
                Err(error) => return Err(error),
            }
        }
        retained.revalidate()?;
        Ok(crate::native_document_file::DirectoryPresence::PresentDirectory)
    }
    pub(crate) fn request(&self) -> &std::sync::Arc<crate::file_io::SnapshotContext> {
        &self.admission.request
    }
    pub(crate) fn directory(&self) -> io::Result<&NativeHandle> {
        if self.selection.as_os_str().is_empty() {
            return Ok(&self.admission.anchor.root);
        }
        self.prefixes
            .last()
            .map(|prefix| prefix.handle.as_ref())
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }
    pub(crate) fn identity(&self) -> io::Result<&PhysicalId> {
        if self.selection.as_os_str().is_empty() {
            return Ok(&self.admission.anchor.identity);
        }
        self.bindings
            .last()
            .map(|binding| &binding.identity)
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }
    pub(crate) fn revalidate(&self) -> io::Result<()> {
        let request = self.request();
        self.admission.revalidate()?;
        for (prefix, binding) in self.prefixes.iter().zip(&self.bindings) {
            if DirectoryBinding::observe(&prefix.handle, request)? != *binding {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        let mut parent = Arc::clone(&self.admission.anchor.root);
        for (component, binding) in self.selection.components().zip(&self.bindings) {
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidData.into());
            };
            let next = {
                let file = parent.lock()?;
                NativeHandle::open_child(&file, name, Some(NodeKind::Directory), request)?
            };
            if DirectoryBinding::observe(&next, request)? != *binding {
                return Err(io::ErrorKind::InvalidData.into());
            }
            parent = next;
        }
        drop(parent);
        self.admission.revalidate()
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(in crate::artifact_store) struct NativeControlChain {
    chain: CapturedChain,
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
impl NativeControlChain {
    pub(in crate::artifact_store) fn ensure_directory(&self, relative: &Path) -> io::Result<()> {
        let request = self.chain.request();
        crate::file_io::validate_relative(relative, true)?;
        self.revalidate()?;
        let admission = Arc::new(RootReadAdmission::new(
            &self.chain.admission.scope,
            request,
        )?);
        let prefixes = open_prefixes_with(
            &admission,
            relative,
            NodeKind::Directory,
            &mut |parent, name, _, _| {
                let opened = read_directory::open_relative_readonly(
                    parent,
                    name,
                    Some(NodeKind::Directory),
                    tree::BUFFER_BYTES,
                    &mut || request.check(),
                );
                let file = match opened {
                    Ok(file) => file,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        match graphit_fs::write_relative::create_private_directory_relative(
                            parent,
                            name,
                            tree::BUFFER_BYTES,
                            &mut || request.check(),
                        ) {
                            Ok(file) => file,
                            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                                read_directory::open_relative_readonly(
                                    parent,
                                    name,
                                    Some(NodeKind::Directory),
                                    tree::BUFFER_BYTES,
                                    &mut || request.check(),
                                )?
                            }
                            Err(error) => return Err(error),
                        }
                    }
                    Err(error) => return Err(error),
                };
                request.check()?;
                Ok(Arc::new(NativeHandle {
                    file: Arc::new(RwLock::new(file)),
                }))
            },
        )?;
        let bindings = prefixes
            .iter()
            .map(|prefix| DirectoryBinding::observe(&prefix.handle, request))
            .collect::<io::Result<Vec<_>>>()?;
        let parent = CapturedChain {
            prefixes,
            bindings,
            selection: relative.to_owned(),
            admission,
        };
        parent.revalidate()?;
        self.revalidate()
    }
    pub(in crate::artifact_store) fn capture(
        scope: &CapturedPublicationScope,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        scope.revalidate_native(request)?;
        let relative = scope
            .runtime_root()
            .strip_prefix(scope.runtime_anchor_root())
            .map_err(|_| io::ErrorKind::InvalidInput)?;
        crate::file_io::validate_relative(relative, false)?;
        // This exact control destination comes from the opaque scope factory.
        // The publication allowed-roots policy deliberately does not admit it.
        scope.admit_native_control_layout(request)?;
        let admission = Arc::new(RootReadAdmission::runtime(scope, request)?);
        let selection = relative.to_owned();
        let count = relative.components().count();

        let prefixes = open_prefixes_with(
            &admission,
            relative,
            NodeKind::Directory,
            &mut |parent, name, kind, last| {
                if kind != NodeKind::Directory {
                    return Err(io::ErrorKind::InvalidInput.into());
                }

                let opened = if last {
                    graphit_fs::write_relative::open_private_directory_relative(
                        parent,
                        name,
                        64 * 1024,
                        &mut || request.check(),
                    )
                } else {
                    read_directory::open_relative_readonly(
                        parent,
                        name,
                        Some(NodeKind::Directory),
                        64 * 1024,
                        &mut || request.check(),
                    )
                };
                let file = match opened {
                    Ok(file) => file,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        graphit_fs::write_relative::create_private_directory_relative(
                            parent,
                            name,
                            64 * 1024,
                            &mut || request.check(),
                        )?
                    }
                    Err(error) => return Err(error),
                };
                request.check()?;
                Ok(Arc::new(NativeHandle {
                    file: Arc::new(RwLock::new(file)),
                }))
            },
        )?;
        let mut chain = CapturedChain {
            prefixes,
            bindings: Vec::new(),
            selection,
            admission,
        };
        chain
            .bindings
            .try_reserve_exact(count)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        for prefix in &chain.prefixes {
            chain
                .bindings
                .push(DirectoryBinding::observe(&prefix.handle, request)?);
        }
        chain.revalidate()?;
        Ok(Self { chain })
    }
    pub(in crate::artifact_store) fn create_record_directory(
        &self,
        kind: crate::artifact_store::ProjectRecordDirectory,
        limits: crate::native_document_file::NativeDocumentFileLimits,
    ) -> io::Result<crate::native_document_file::CapturedProjectDocumentDirectory> {
        use crate::native_document_file::CapturedProjectDocumentDirectory;
        let request = self.chain.request();
        let directory_name = kind.directory_name();
        let selection = self.chain.selection.join(directory_name);
        let path_bytes = CapturedProjectDocumentDirectory::validate_runtime_capture_path(
            &self.chain.admission.scope,
            &selection,
            limits,
            request,
        )?;
        self.chain.revalidate()?;
        let count = self
            .chain
            .prefixes
            .len()
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        let mut prefixes = Vec::new();
        prefixes
            .try_reserve_exact(count)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        for prefix in &self.chain.prefixes {
            prefixes.push(PrefixHandle {
                handle: Arc::clone(&prefix.handle),
                observation: prefix.observation.clone(),
            });
        }
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(count)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        bindings.extend(self.chain.bindings.iter().cloned());
        let file = {
            let parent = self.file()?;
            graphit_fs::write_relative::create_private_directory_relative(
                &parent,
                std::ffi::OsStr::new(directory_name),
                64 * 1024,
                &mut || request.check(),
            )?
        };
        #[cfg(test)]
        crate::artifact_store::native_writer_acquisition_hooks::after_record_birth();
        let handle = Arc::new(NativeHandle {
            file: Arc::new(RwLock::new(file)),
        });
        bindings.push(DirectoryBinding::observe(&handle, request)?);
        prefixes.push(PrefixHandle {
            observation: handle.observe(request)?,
            handle,
        });
        let chain = CapturedChain {
            prefixes,
            bindings,
            selection,
            admission: Arc::clone(&self.chain.admission),
        };
        chain.revalidate()?;
        Ok(CapturedProjectDocumentDirectory::from_captured_chain(
            Arc::new(chain),
            limits,
            path_bytes,
        ))
    }
    pub(in crate::artifact_store) fn file(&self) -> io::Result<RwLockWriteGuard<'_, File>> {
        self.chain.directory()?.lock()
    }
    pub(in crate::artifact_store) fn revalidate(&self) -> io::Result<()> {
        self.chain.revalidate()
    }
}

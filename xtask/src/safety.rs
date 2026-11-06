//! Keep native filesystem, identity and Job exceptions within exact private SDK functions.
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use syn::visit::{self, Visit};

// Entire qualified bindgen projection: ABI, types, visibility and scope.
const MAC_SDK_SHA256: [u8; 32] = [
    0x48, 0x34, 0x26, 0x37, 0x40, 0x8e, 0x55, 0xc6, 0x70, 0xe8, 0x11, 0xfa, 0x93, 0xd9, 0x78, 0x9f,
    0x48, 0xb8, 0xbf, 0x47, 0x35, 0x1b, 0xea, 0xbb, 0xd3, 0x2a, 0xb5, 0x5c, 0xa3, 0x75, 0x8d, 0x00,
];
fn verify_generated_sdk(text: &str) -> Result<(), String> {
    if Sha256::digest(text.as_bytes()).as_slice() != MAC_SDK_SHA256 {
        return Err(
            "native safety boundary: generated macOS SDK differs from qualified projection".into(),
        );
    }
    Ok(())
}

pub(super) fn run(root: &Path) -> Result<(), String> {
    let source = root.join("crates/graphit-fs/src");
    let mut totals = (0, 0);
    let boundary = fs::read_to_string(source.join("lib.rs")).map_err(|e| e.to_string())?;
    let file = syn::parse_file(&boundary).map_err(|e| e.to_string())?;
    verify_generated_sdk(
        &fs::read_to_string(source.join("macos_private_sdk.rs")).map_err(|e| e.to_string())?,
    )?;
    if !file.items.iter().any(|item| matches!(item, syn::Item::Mod(module) if module.ident == "macos_private_sdk" && matches!(module.vis, syn::Visibility::Inherited) && module.content.is_none())) {
        return Err("native safety boundary: generated SDK module must remain private".into());
    }
    verify_crate_safety(&file)?;
    inspect_directory(&source, &source, SourceDomain::Filesystem, &mut totals)?;
    let core = root.join("crates/graphit-core/src");
    let core_boundary = fs::read_to_string(core.join("lib.rs")).map_err(|e| e.to_string())?;
    let core_file = syn::parse_file(&core_boundary).map_err(|e| e.to_string())?;
    verify_crate_safety(&core_file)?;
    inspect_directory(&core, &core, SourceDomain::Core, &mut totals)?;
    let process = root.join("crates/graphit-process/src");
    let process_boundary = fs::read_to_string(process.join("lib.rs")).map_err(|e| e.to_string())?;
    let process_file = syn::parse_file(&process_boundary).map_err(|e| e.to_string())?;
    verify_crate_safety(&process_file)?;
    inspect_directory(&process, &process, SourceDomain::Process, &mut totals)?;
    println!("Native safety boundary: scoped SDK functions and exact unsafe calls verified");
    Ok(())
}
fn verify_crate_safety(file: &syn::File) -> Result<(), String> {
    for (level, name) in [
        ("deny", "unsafe_code"),
        ("forbid", "unsafe_op_in_unsafe_fn"),
    ] {
        if !file.attrs.iter().any(|attribute| matches!(&attribute.meta, syn::Meta::List(list) if attribute.path().is_ident(level) && list.tokens.to_string() == name)) {
            return Err(format!("native safety boundary: missing crate {level}({name})"));
        }
    }
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceDomain {
    Filesystem,
    Core,
    Process,
}
impl SourceDomain {
    fn namespace(self) -> &'static Path {
        match self {
            Self::Filesystem => Path::new(""),
            Self::Core => Path::new("graphit-core"),
            Self::Process => Path::new("graphit-process"),
        }
    }
}
fn inspect_directory(
    root: &Path,
    dir: &Path,
    domain: SourceDomain,
    totals: &mut (usize, usize),
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("native safety boundary: symlink in source".into());
        }
        if kind.is_dir() {
            inspect_directory(root, &path, domain, totals)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
            if domain == SourceDomain::Filesystem && relative == Path::new("macos_private_sdk.rs") {
                let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                verify_generated_sdk(&text)?;
                continue;
            }
            inspect_file(&domain.namespace().join(relative), &path, domain, totals)?;
        }
    }
    Ok(())
}
fn inspect_file(
    source: &Path,
    path: &Path,
    domain: SourceDomain,
    totals: &mut (usize, usize),
) -> Result<(), String> {
    if fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("native safety boundary: symlink in source".into());
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file = syn::parse_file(&text).map_err(|e| e.to_string())?;
    let mut audit = Audit {
        source: source.to_path_buf(),
        domain,
        function: None,
        block_base: 0,
        allowed: false,
        attribute_allowed: false,
        exception_scope: false,
        depth: 0,
        exceptions: 0,
        blocks: 0,
        errors: Vec::new(),
    };
    audit.visit_file(&file);
    if !audit.errors.is_empty() {
        return Err(format!(
            "native safety boundary {}: {}",
            path.display(),
            audit.errors.join(", ")
        ));
    }
    totals.0 += audit.exceptions;
    totals.1 += audit.blocks;
    Ok(())
}
struct Audit {
    source: std::path::PathBuf,
    domain: SourceDomain,
    function: Option<String>,
    block_base: usize,
    allowed: bool,
    attribute_allowed: bool,
    exception_scope: bool,
    depth: usize,
    exceptions: usize,
    blocks: usize,
    errors: Vec<String>,
}
impl<'ast> Visit<'ast> for Audit {
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        let before = self.allowed;
        let scope_before = self.exception_scope;
        let function_before = self.function.clone();
        let counts_before = (self.exceptions, self.blocks);
        let base_before = self.block_base;
        self.block_base = self.blocks;
        let name = function.sig.ident.to_string();
        let listed = (self.source == Path::new("windows.rs") && name == "set_junction")
            || (self.source == Path::new("windows_document_access.rs") && name == "snapshot")
            || (self.source == Path::new("windows_private_file.rs")
                && matches!(
                    name.as_str(),
                    "create_directory"
                        | "create_file"
                        | "replace_file"
                        | "descriptor_control"
                        | "descriptor_ace_header"
                ))
            || (self.source == Path::new("macos_access.rs")
                && matches!(
                    name.as_str(),
                    "extended_security_support"
                        | "read_attributes"
                        | "read_attributes_relative"
                        | "write_attributes"
                ))
            || (self.source == Path::new("macos_private_file.rs")
                && matches!(
                    name.as_str(),
                    "allocate_acl"
                        | "configure_acl"
                        | "free_acl"
                        | "allocate_filesec"
                        | "configure_filesec"
                        | "free_filesec"
                        | "create_file"
                        | "create_directory"
                ));
        let relative_sdk = self.source == Path::new("windows_relative_sdk.rs")
            && matches!(
                name.as_str(),
                "open_relative" | "query_directory" | "directory_buffer_bytes"
            );
        let core_identity = self.domain == SourceDomain::Core
            && self.source == Path::new("graphit-core/native_file_identity.rs")
            && name == "query_file_id128";
        let process_job = self.domain == SourceDomain::Process
            && self.source == Path::new("graphit-process/windows_job.rs")
            && name == "active_processes";
        let exact_visibility = if relative_sdk && name != "open_relative" {
            matches!(&function.vis, syn::Visibility::Restricted(visibility) if visibility.in_token.is_none() && visibility.path.is_ident("super"))
        } else {
            matches!(function.vis, syn::Visibility::Inherited)
        };
        self.allowed = ((self.domain == SourceDomain::Filesystem && (listed || relative_sdk))
            || core_identity
            || process_job)
            && self.depth == 0
            && exact_visibility
            && function.sig.unsafety.is_none();
        self.function = self.allowed.then_some(name);
        self.exception_scope |= self.allowed;
        self.depth += 1;
        for attribute in &function.attrs {
            self.attribute_allowed = self.allowed;
            self.visit_attribute(attribute);
            self.attribute_allowed = false;
        }
        self.visit_signature(&function.sig);
        self.visit_block(&function.block);
        self.depth -= 1;
        if self.allowed {
            let expected = match (self.source.to_str(), self.function.as_deref()) {
                (Some("windows_document_access.rs"), Some("snapshot")) => 4,
                (Some("macos_private_file.rs"), Some("configure_filesec"))
                | (Some("windows_relative_sdk.rs"), Some("open_relative")) => 3,
                (Some("macos_private_file.rs"), Some("configure_acl"))
                | (_, Some("create_file"))
                | (Some("windows_private_file.rs"), Some("descriptor_ace_header")) => 2,
                _ => 1,
            };
            if (
                self.exceptions - counts_before.0,
                self.blocks - counts_before.1,
            ) != (1, expected)
            {
                self.errors
                    .push("native function requires exactly its listed exception/calls".into());
            }
        }
        self.block_base = base_before;
        self.function = function_before;
        self.allowed = before;
        self.exception_scope = scope_before;
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        self.depth += 1;
        visit::visit_item_mod(self, module);
        self.depth -= 1;
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        fn exposes_sdk(tree: &syn::UseTree) -> bool {
            match tree {
                syn::UseTree::Path(path) => {
                    path.ident == "macos_private_sdk" || exposes_sdk(&path.tree)
                }
                syn::UseTree::Name(name) => name.ident == "macos_private_sdk",
                syn::UseTree::Rename(name) => name.ident == "macos_private_sdk",
                syn::UseTree::Group(group) => group.items.iter().any(exposes_sdk),
                syn::UseTree::Glob(_) => false,
            }
        }
        if !matches!(item.vis, syn::Visibility::Inherited) && exposes_sdk(&item.tree) {
            self.errors
                .push("generated SDK declarations cannot be reexported".into());
        }
        if self.source == Path::new("lib.rs") && !matches!(item.vis, syn::Visibility::Inherited) {
            let allowed = matches!(&item.tree, syn::UseTree::Path(path) if matches!(path.ident.to_string().as_str(), "native_principal" | "exclusive_rename" | "private_file" | "private_file_metadata" | "windows_private_file" | "macos_access" | "macos_access_buffer" | "linux_access" | "windows_document_access"));
            if !allowed {
                self.errors
                    .push("native public export outside qualified facade modules".into());
            }
        }
        visit::visit_item_use(self, item);
    }
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if let syn::Meta::List(list) = &attribute.meta
            && contains_identifier(list.tokens.clone(), "unsafe_code")
            && !attribute.path().is_ident("deny")
            && !attribute.path().is_ident("forbid")
        {
            if self.attribute_allowed
                && attribute.path().is_ident("allow")
                && list.tokens.to_string() == "unsafe_code"
            {
                self.exceptions += 1;
            } else {
                self.errors
                    .push("unsafe exception outside listed private SDK function".into());
            }
        }
        visit::visit_attribute(self, attribute);
    }
    fn visit_expr_unsafe(&mut self, block: &'ast syn::ExprUnsafe) {
        if self.allowed {
            self.blocks += 1;
            let bounded_call = matches!(block.block.stmts.as_slice(), [syn::Stmt::Expr(syn::Expr::Call(call), None)] if expected_arguments(&self.source, self.function.as_deref().unwrap_or(""), self.blocks - self.block_base, call));
            if !bounded_call {
                self.errors.push(
                    "unsafe block must contain only its listed SDK call and arguments".into(),
                );
            }
        } else {
            self.errors
                .push("unsafe block outside listed private SDK function".into());
        }
        visit::visit_expr_unsafe(self, block);
    }
    fn visit_signature(&mut self, signature: &'ast syn::Signature) {
        if signature.unsafety.is_some() {
            self.errors.push("unsafe function is not permitted".into());
        }
        visit::visit_signature(self, signature);
    }
    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        if self.exception_scope || invocation.path.is_ident("include") {
            self.errors.push(
                "macros are not permitted in the native exception or as source includes".into(),
            );
        }
        if contains_identifier(invocation.tokens.clone(), "unsafe")
            || contains_identifier(invocation.tokens.clone(), "unsafe_code")
        {
            self.errors
                .push("unsafe tokens in macros are not permitted".into());
        }
        visit::visit_macro(self, invocation);
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if item.unsafety.is_some() {
            self.errors.push("unsafe impl is not permitted".into());
        }
        visit::visit_item_impl(self, item);
    }
    fn visit_impl_item_fn(&mut self, method: &'ast syn::ImplItemFn) {
        let before = self.allowed;
        self.allowed = false;
        self.depth += 1;
        if self.exception_scope {
            self.errors
                .push("methods nested in a native exception are not permitted".into());
        }
        visit::visit_impl_item_fn(self, method);
        self.depth -= 1;
        self.allowed = before;
    }
    fn visit_trait_item_fn(&mut self, method: &'ast syn::TraitItemFn) {
        let before = self.allowed;
        self.allowed = false;
        self.depth += 1;
        if self.exception_scope {
            self.errors
                .push("methods nested in a native exception are not permitted".into());
        }
        visit::visit_trait_item_fn(self, method);
        self.depth -= 1;
        self.allowed = before;
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        if item.unsafety.is_some() {
            self.errors.push("unsafe trait is not permitted".into());
        }
        visit::visit_item_trait(self, item);
    }
    fn visit_item_foreign_mod(&mut self, item: &'ast syn::ItemForeignMod) {
        self.errors
            .push("manual foreign blocks are not permitted outside the pinned SDK".into());
        visit::visit_item_foreign_mod(self, item);
    }
}
fn expected_arguments(source: &Path, function: &str, ordinal: usize, call: &syn::ExprCall) -> bool {
    let (name, expected): (&str, &[&str]) = match (source.to_str(), function, ordinal) {
        (Some("graphit-process/windows_job.rs"), "active_processes", 1) => (
            "QueryInformationJobObject",
            &[
                "job.handle() as _",
                "JobObjectBasicAccountingInformation",
                "(&mut information as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast()",
                "std::mem::size_of_val(&information) as u32",
                "std::ptr::null_mut()",
            ],
        ),
        (Some("windows_document_access.rs"), "snapshot", 1) => (
            "GetKernelObjectSecurity",
            &[
                "file.as_raw_handle()",
                "information",
                "ptr::null_mut()",
                "0",
                "&mut needed",
            ],
        ),
        (Some("windows_document_access.rs"), "snapshot", 2) => (
            "GetKernelObjectSecurity",
            &[
                "file.as_raw_handle()",
                "information",
                "words.as_mut_ptr().cast()",
                "needed",
                "&mut actual",
            ],
        ),
        (Some("windows_document_access.rs"), "snapshot", 3) => {
            ("IsValidSecurityDescriptor", &["words.as_mut_ptr().cast()"])
        }
        (Some("windows_document_access.rs"), "snapshot", 4) => (
            "GetSecurityDescriptorControl",
            &["words.as_mut_ptr().cast()", "&mut control", "&mut revision"],
        ),
        (Some("windows_relative_sdk.rs"), "open_relative", 1) => (
            "NtCreateFile",
            &[
                "&mut raw",
                "access",
                "&attributes",
                "&mut status_block",
                "ptr::null()",
                "0",
                "sharing",
                "disposition",
                "options",
                "ptr::null()",
                "0",
            ],
        ),
        (Some("windows_relative_sdk.rs"), "open_relative", 2) => {
            ("RtlNtStatusToDosError", &["status"])
        }
        (Some("windows_relative_sdk.rs"), "open_relative", 3) => {
            ("OwnedHandle::from_raw_handle", &["raw"])
        }
        (Some("windows_relative_sdk.rs"), "query_directory", 1) => (
            "GetFileInformationByHandleEx",
            &[
                "file.as_raw_handle()",
                "FileIdBothDirectoryInfo",
                "buffer.as_mut_ptr().cast()",
                "bytes",
            ],
        ),
        (Some("windows_relative_sdk.rs"), "directory_buffer_bytes", 1) => (
            "std::slice::from_raw_parts",
            &["buffer.as_ptr().cast()", "length"],
        ),
        (_, "query_file_id128", 1)
            if source == Path::new("graphit-core/native_file_identity.rs") =>
        {
            (
                "GetFileInformationByHandleEx",
                &[
                    "file.as_raw_handle()",
                    "FileIdInfo",
                    "(&mut information as *mut FILE_ID_INFO).cast()",
                    "bytes",
                ],
            )
        }
        (Some("macos_private_file.rs"), "allocate_acl", 1) => ("sdk::acl_init", &["0"]),
        (Some("macos_private_file.rs"), "configure_acl", 1) => (
            "sdk::acl_get_flagset_np",
            &["acl.pointer.cast()", "&mut flags"],
        ),
        (Some("macos_private_file.rs"), "configure_acl", 2) => (
            "sdk::acl_add_flag_np",
            &["flags", "sdk::acl_flag_t_ACL_FLAG_NO_INHERIT"],
        ),
        (Some("macos_private_file.rs"), "free_acl", 1) => ("sdk::acl_free", &["pointer.cast()"]),
        (Some("macos_private_file.rs"), "allocate_filesec", 1) => ("sdk::filesec_init", &[]),
        (Some("macos_private_file.rs"), "configure_filesec", 1) => (
            "sdk::filesec_set_property",
            &[
                "security.pointer",
                "sdk::filesec_property_t_FILESEC_OWNER",
                "(&owner as *const libc::uid_t).cast()",
            ],
        ),
        (Some("macos_private_file.rs"), "configure_filesec", 2) => (
            "sdk::filesec_set_property",
            &[
                "security.pointer",
                "sdk::filesec_property_t_FILESEC_MODE",
                "(&mode as *const libc::mode_t).cast()",
            ],
        ),
        (Some("macos_private_file.rs"), "configure_filesec", 3) => (
            "sdk::filesec_set_property",
            &[
                "security.pointer",
                "sdk::filesec_property_t_FILESEC_ACL",
                "(&acl.pointer as *const sdk::acl_t).cast()",
            ],
        ),
        (Some("macos_private_file.rs"), "free_filesec", 1) => ("sdk::filesec_free", &["pointer"]),
        (Some("macos_private_file.rs"), "create_file", 1) => (
            "sdk::openx_np",
            &[
                "path.as_ptr()",
                "libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_RDWR",
                "security.pointer",
            ],
        ),
        (Some("macos_private_file.rs"), "create_file", 2) => {
            ("OwnedFd::from_raw_fd", &["descriptor"])
        }
        (Some("macos_private_file.rs"), "create_directory", 1) => {
            ("sdk::mkdirx_np", &["path.as_ptr()", "security.pointer"])
        }
        (Some("windows.rs"), "set_junction", 1) => (
            "DeviceIoControl",
            &[
                "file.as_raw_handle()",
                "FSCTL_SET_REPARSE_POINT",
                "buffer.as_ptr().cast()",
                "buffer.len() as u32",
                "std::ptr::null_mut()",
                "0",
                "&mut returned",
                "std::ptr::null_mut()",
            ],
        ),
        (Some("windows_private_file.rs"), "descriptor_control", 1) => (
            "GetSecurityDescriptorControl",
            &[
                "std::ptr::from_ref(descriptor).cast_mut().cast()",
                "&mut control",
                "&mut revision",
            ],
        ),
        (Some("windows_private_file.rs"), "descriptor_ace_header", 1) => (
            "GetAce",
            &["std::ptr::from_ref(dacl).cast()", "index", "&mut entry"],
        ),
        (Some("windows_private_file.rs"), "descriptor_ace_header", 2) => {
            ("std::ptr::read", &["entry.cast::<ACE_HEADER>()"])
        }
        (Some("windows_private_file.rs"), "create_directory", 1) => {
            ("CreateDirectoryW", &["path.units.as_ptr()", "&attributes"])
        }
        (Some("windows_private_file.rs"), "create_file", 1) => (
            "CreateFileW",
            &[
                "path.units.as_ptr()",
                "GENERIC_READ | GENERIC_WRITE",
                "sharing",
                "&attributes",
                "CREATE_NEW",
                "FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT",
                "ptr::null_mut()",
            ],
        ),
        (Some("windows_private_file.rs"), "create_file", 2) => {
            ("OwnedHandle::from_raw_handle", &["handle"])
        }
        (Some("windows_private_file.rs"), "replace_file", 1) => (
            "ReplaceFileW",
            &[
                "destination.units.as_ptr()",
                "stage.units.as_ptr()",
                "ptr::null()",
                "0",
                "ptr::null()",
                "ptr::null()",
            ],
        ),
        (Some("macos_access.rs"), "extended_security_support", 1) => (
            "libc::fpathconf",
            &["file.as_raw_fd()", "libc::_PC_EXTENDED_SECURITY_NP"],
        ),
        (Some("macos_access.rs"), "read_attributes", 1) => (
            "libc::fgetattrlist",
            &[
                "file.as_raw_fd()",
                "(&mut attributes as *mut libc::attrlist).cast()",
                "buffer.as_mut_ptr().cast()",
                "buffer.len()",
                "libc::FSOPT_REPORT_FULLSIZE | libc::FSOPT_PACK_INVAL_ATTRS",
            ],
        ),
        (Some("macos_access.rs"), "read_attributes_relative", 1) => (
            "libc::getattrlistat",
            &[
                "parent.as_raw_fd()",
                "child.as_ptr()",
                "(&mut attributes as *mut libc::attrlist).cast()",
                "buffer.as_mut_ptr().cast()",
                "buffer.len()",
                "(libc::FSOPT_REPORT_FULLSIZE | libc::FSOPT_PACK_INVAL_ATTRS | libc::FSOPT_NOFOLLOW).into()",
            ],
        ),
        (Some("macos_access.rs"), "write_attributes", 1) => (
            "libc::fsetattrlist",
            &[
                "file.as_raw_fd()",
                "(&mut attributes as *mut libc::attrlist).cast()",
                "buffer.as_mut_ptr().cast()",
                "buffer.len()",
                "0",
            ],
        ),
        _ => return false,
    };
    syn::parse_str::<syn::Expr>(name).is_ok_and(|expression| *call.func == expression)
        && call.args.len() == expected.len()
        && call
            .args
            .iter()
            .zip(expected.iter())
            .all(|(actual, expected)| {
                syn::parse_str::<syn::Expr>(expected).is_ok_and(|expression| *actual == expression)
            })
}
fn contains_identifier(stream: proc_macro2::TokenStream, name: &str) -> bool {
    stream.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(id) => id == name,
        proc_macro2::TokenTree::Group(group) => contains_identifier(group.stream(), name),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn audit(code: &str) -> Audit {
        audit_in("windows.rs", code)
    }
    fn audit_in(source: &str, code: &str) -> Audit {
        let mut audit = Audit {
            source: source.into(),
            domain: if Path::new(source).starts_with("graphit-core") {
                SourceDomain::Core
            } else if Path::new(source).starts_with("graphit-process") {
                SourceDomain::Process
            } else {
                SourceDomain::Filesystem
            },
            function: None,
            block_base: 0,
            allowed: false,
            attribute_allowed: false,
            exception_scope: false,
            depth: 0,
            exceptions: 0,
            blocks: 0,
            errors: Vec::new(),
        };
        audit.visit_file(&syn::parse_file(code).unwrap());
        audit
    }

    #[test]
    fn process_job_allows_only_exact_private_accounting_query() {
        let source = include_str!("../../crates/graphit-process/src/windows_job.rs");
        let valid = audit_in("graphit-process/windows_job.rs", source);
        assert!(valid.errors.is_empty(), "{:?}", valid.errors);
        assert_eq!((valid.exceptions, valid.blocks), (1, 1));
        for invalid in [
            source.replace("fn active_processes", "pub fn active_processes"),
            source.replace("job.handle() as _", "other.handle() as _"),
            source.replace(
                "JobObjectBasicAccountingInformation,",
                "JobObjectBasicProcessIdList,",
            ),
            source.replace("std::mem::size_of_val(&information) as u32,", "0,"),
            source.replace("unsafe {", "unsafe { other();"),
        ] {
            assert!(
                !audit_in("graphit-process/windows_job.rs", &invalid)
                    .errors
                    .is_empty()
            );
        }
        assert!(
            !audit_in("graphit-process/other.rs", source)
                .errors
                .is_empty()
        );
    }

    #[test]
    fn generated_sdk_pin_rejects_abi_signatures_symbols_visibility_and_scope() {
        let sdk = include_str!("../../crates/graphit-fs/src/macos_private_sdk.rs");
        assert!(verify_generated_sdk(sdk).is_ok());
        for invalid in [
            sdk.replace("extern \"C\"", "extern \"C-unwind\""),
            sdk.replace(
                "count: ::std::os::raw::c_int",
                "count: ::std::os::raw::c_uint",
            ),
            sdk.replace(
                "pub(super) fn acl_init",
                "pub(super) fn extra(); pub(super) fn acl_init",
            ),
            sdk.replace("pub(super) fn acl_init", "pub fn acl_init"),
            sdk.replace("#[allow(unsafe_code)]", "#![allow(unsafe_code)]"),
            format!("mod moved {{ {sdk} }}"),
        ] {
            assert!(verify_generated_sdk(&invalid).is_err());
        }
        assert!(
            !audit_in("lib.rs", "pub use macos_private_sdk::*;")
                .errors
                .is_empty()
        );
        assert!(
            !audit_in(
                "lib.rs",
                "use macos_private_sdk as bridge; pub use bridge::acl_init;"
            )
            .errors
            .is_empty()
        );
        assert!(
            !audit_in(
                "lib.rs",
                "use macos_private_sdk as first; use first as second; pub use second::acl_t;"
            )
            .errors
            .is_empty()
        );
    }

    #[test]
    fn mac_private_birth_sites_reject_wrong_source_arguments_and_nesting() {
        let sdk = include_str!("../../crates/graphit-fs/src/macos_private_file.rs");
        assert!(audit_in("macos_private_file.rs", sdk).errors.is_empty());
        assert!(!audit_in("windows_private_file.rs", sdk).errors.is_empty());
        for invalid in [
            sdk.replace("sdk::acl_init(0)", "sdk::acl_init(1)"),
            sdk.replace("sdk::acl_flag_t_ACL_FLAG_NO_INHERIT", "0"),
            sdk.replace(
                "(&acl.pointer as *const sdk::acl_t).cast()",
                "acl.pointer.cast()",
            ),
            sdk.replace(
                "(&owner as *const libc::uid_t).cast()",
                "(&owner as *const libc::mode_t).cast()",
            ),
            sdk.replace("(&mode as *const libc::mode_t).cast()", "std::ptr::null()"),
            sdk.replace("libc::O_EXCL", "libc::O_TRUNC"),
            sdk.replace(
                "OwnedFd::from_raw_fd(descriptor)",
                "OwnedFd::from_raw_fd(-1)",
            ),
            sdk.replace("fn allocate_acl()", "pub fn allocate_acl()"),
            sdk.replace(
                "unsafe { sdk::acl_init(0) }",
                "{ fn nested() { unsafe { sdk::acl_init(0) }; } std::ptr::null_mut() }",
            ),
            sdk.replace("sdk::filesec_init()", "sdk::filesec_init(); extra()"),
        ] {
            assert!(
                !audit_in("macos_private_file.rs", &invalid)
                    .errors
                    .is_empty()
            );
        }
    }

    #[test]
    fn mac_sdk_sites_pin_descriptor_storage_and_options() {
        for (name, calls) in [
            (
                "read_attributes_relative",
                "unsafe { libc::getattrlistat(parent.as_raw_fd(), child.as_ptr(), (&mut attributes as *mut libc::attrlist).cast(), buffer.as_mut_ptr().cast(), buffer.len(), (libc::FSOPT_REPORT_FULLSIZE | libc::FSOPT_PACK_INVAL_ATTRS | libc::FSOPT_NOFOLLOW).into()) }",
            ),
            (
                "write_attributes",
                "unsafe { libc::fsetattrlist(file.as_raw_fd(), (&mut attributes as *mut libc::attrlist).cast(), buffer.as_mut_ptr().cast(), buffer.len(), 0) }",
            ),
            (
                "extended_security_support",
                "unsafe { libc::fpathconf(file.as_raw_fd(), libc::_PC_EXTENDED_SECURITY_NP) }",
            ),
            (
                "read_attributes",
                "unsafe { libc::fgetattrlist(file.as_raw_fd(), (&mut attributes as *mut libc::attrlist).cast(), buffer.as_mut_ptr().cast(), buffer.len(), libc::FSOPT_REPORT_FULLSIZE | libc::FSOPT_PACK_INVAL_ATTRS) }",
            ),
        ] {
            let code = format!("#[allow(unsafe_code)] fn {name}() {{ {calls} }}");
            assert!(audit_in("macos_access.rs", &code).errors.is_empty());
            assert!(!audit_in("another.rs", &code).errors.is_empty());
            for invalid in [
                code.replace("fn ", "pub fn "),
                code.replace("fn ", "unsafe fn "),
                code.replace("unsafe {", "unsafe { unexpected();"),
                code.replace("file.as_raw_fd()", "0"),
                code.replace("libc::_PC_EXTENDED_SECURITY_NP", "libc::_PC_NAME_MAX"),
                code.replace("parent.as_raw_fd()", "0"),
                code.replace("child.as_ptr()", "std::ptr::null()"),
                code.replace(" | libc::FSOPT_NOFOLLOW", ""),
                code.replace("buffer.len()", "usize::MAX"),
                code.replace(
                    "libc::FSOPT_REPORT_FULLSIZE | libc::FSOPT_PACK_INVAL_ATTRS",
                    "0",
                ),
                code.replace(
                    "(&mut attributes as *mut libc::attrlist).cast()",
                    "std::ptr::null_mut()",
                ),
                format!("mod nested {{ {code} }}"),
                format!(
                    "#[allow(unsafe_code)] fn {name}() {{ impl Local {{ fn nested() {{ {calls} }} }} }}"
                ),
                format!(
                    "#[allow(unsafe_code)] fn {name}() {{ trait Local {{ fn nested() {{ {calls} }} }} }}"
                ),
                code.replace("() {", "() { unsafe {}"),
            ] {
                if invalid != code {
                    assert!(
                        !audit_in("macos_access.rs", &invalid).errors.is_empty(),
                        "escaped Mac SDK site {name}: {invalid}"
                    );
                }
            }
        }
    }
    #[test]
    fn sdk_sites_reject_wider_scope_arguments_and_extra_calls() {
        for (name, calls) in [
            (
                "descriptor_control",
                "unsafe { GetSecurityDescriptorControl(std::ptr::from_ref(descriptor).cast_mut().cast(), &mut control, &mut revision) }",
            ),
            (
                "descriptor_ace_header",
                "unsafe { GetAce(std::ptr::from_ref(dacl).cast(), index, &mut entry) }; unsafe { std::ptr::read(entry.cast::<ACE_HEADER>()) }",
            ),
            (
                "create_directory",
                "unsafe { CreateDirectoryW(path.units.as_ptr(), &attributes) }",
            ),
            (
                "create_file",
                "unsafe { CreateFileW(path.units.as_ptr(), GENERIC_READ | GENERIC_WRITE, sharing, &attributes, CREATE_NEW, FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT, ptr::null_mut()) }; unsafe { OwnedHandle::from_raw_handle(handle) }",
            ),
            (
                "replace_file",
                "unsafe { ReplaceFileW(destination.units.as_ptr(), stage.units.as_ptr(), ptr::null(), 0, ptr::null(), ptr::null()) }",
            ),
        ] {
            let code = format!("#[allow(unsafe_code)] fn {name}() {{ {calls} }}");
            assert!(audit_in("windows_private_file.rs", &code).errors.is_empty());
            assert!(!audit_in("another.rs", &code).errors.is_empty());
            for invalid in [
                code.replace("fn ", "pub fn "),
                code.replace("fn ", "unsafe fn "),
                code.replace("unsafe {", "unsafe { unexpected();"),
                code.replace("path.units.as_ptr()", "ptr::null()"),
                code.replace("CREATE_NEW", "CREATE_ALWAYS"),
                code.replace(
                    ", sharing,",
                    ", FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,",
                ),
                code.replace(", sharing,", ", arbitrary_sharing,"),
                code.replace("from_ref(descriptor)", "from_ref(other_descriptor)"),
                code.replace("from_ref(dacl)", "from_ref(other_acl)"),
                code.replace("index, &mut entry", "0, &mut entry"),
                code.replace("entry.cast::<ACE_HEADER>()", "entry.cast::<ACL>()"),
                code.replace("ptr::null(), 0,", "ptr::null(), 2,"),
                format!("mod nested {{ {code} }}"),
                format!(
                    "#[allow(unsafe_code)] fn {name}() {{ impl Local {{ fn nested() {{ {calls} }} }} }}"
                ),
                format!(
                    "#[allow(unsafe_code)] fn {name}() {{ trait Local {{ fn nested() {{ {calls} }} }} }}"
                ),
                code.replace("() {", "() { unsafe {}"),
            ] {
                if invalid != code {
                    assert!(
                        !audit_in("windows_private_file.rs", &invalid)
                            .errors
                            .is_empty(),
                        "escaped SDK site {name}: {invalid}"
                    );
                }
            }
        }
    }
    #[test]
    fn principal_facade_is_exact_and_does_not_allow_raw_sdk_exports() {
        assert!(
            audit_in(
                "lib.rs",
                "pub use native_principal::{NativePrincipalError, NativePrincipalObservation};"
            )
            .errors
            .is_empty()
        );
        for code in [
            "pub use native_principal_raw::*;",
            "pub use winsafe::*;",
            "pub use windows_facts::*;",
            "pub use rustix::process::*;",
        ] {
            assert!(
                !audit_in("lib.rs", code).errors.is_empty(),
                "escaped principal facade: {code}"
            );
        }
        assert!(
            !audit_in(
                "native_principal/windows.rs",
                "fn token() { unsafe { raw_token(); } }"
            )
            .errors
            .is_empty()
        );
    }

    #[test]
    fn wider_exceptions_and_extra_blocks_are_rejected() {
        for code in [
            "#![allow(unsafe_code)]",
            "#[allow(unsafe_code)] mod extra {}",
            "#[allow(unsafe_code)] pub fn set_junction() {}",
            "#[allow(unsafe_code)] fn other() {}",
            "#[cfg_attr(windows, allow(unsafe_code))] mod extra {}",
            "fn other() { unsafe {} }",
            "unsafe fn other() {}",
        ] {
            assert!(!audit(code).errors.is_empty(), "escaped boundary: {code}");
        }
        let call = "DeviceIoControl(file.as_raw_handle(), FSCTL_SET_REPARSE_POINT, buffer.as_ptr().cast(), buffer.len() as u32, std::ptr::null_mut(), 0, &mut returned, std::ptr::null_mut())";
        let code = format!("#[allow(unsafe_code)] fn set_junction() {{ unsafe {{ {call} }} }}");
        let valid = audit(&code);
        assert!(valid.errors.is_empty());
        assert_eq!((valid.exceptions, valid.blocks), (1, 1));
        for invalid in [
            code.replace("FSCTL_SET_REPARSE_POINT", "FSCTL_GET_REPARSE_POINT"),
            code.replace(
                "file.as_raw_handle()",
                "{ AnotherUnsafeFFI(); file.as_raw_handle() }",
            ),
            "macro_rules! escape { () => { unsafe { other() } }; }".to_owned(),
            format!("mod nested {{ {code} }}"),
            code.replace("unsafe {", "#[allow(unsafe_code)] unsafe {"),
            code.replace(
                "fn set_junction() {",
                "fn set_junction() { unsafe extern \"C\" { fn other(); }",
            ),
            code.replace(
                "fn set_junction() {",
                "fn set_junction() { include!(\"other.rs\");",
            ),
            code.replace(
                "fn set_junction() {",
                "fn set_junction() { external::operation!();",
            ),
        ] {
            assert!(
                !audit(&invalid).errors.is_empty(),
                "escaped native call: {invalid}"
            );
        }
        assert!(
            !audit("#[allow(unsafe_code)] fn set_junction() { unsafe { other() } }")
                .errors
                .is_empty()
        );
    }
    #[test]
    fn activated_relative_and_identity_sdk_sources_have_exact_sites() {
        let relative = audit_in(
            "windows_relative_sdk.rs",
            include_str!("../../crates/graphit-fs/src/windows_relative_sdk.rs"),
        );
        assert!(relative.errors.is_empty(), "{:?}", relative.errors);
        assert_eq!((relative.exceptions, relative.blocks), (3, 5));
        let identity_path = Path::new("graphit-core").join("native_file_identity.rs");
        let identity = audit_in(
            identity_path.to_str().unwrap(),
            include_str!("../../crates/graphit-core/src/native_file_identity.rs"),
        );
        assert!(identity.errors.is_empty(), "{:?}", identity.errors);
        assert_eq!((identity.exceptions, identity.blocks), (1, 1));
    }

    #[test]
    fn relative_and_identity_sdk_sites_reject_laundered_calls_and_scope() {
        for (source, name, visibility, calls, blocks, callee, argument) in [
            (
                "windows_relative_sdk.rs",
                "open_relative",
                "",
                "unsafe { NtCreateFile(&mut raw, access, &attributes, &mut status_block, ptr::null(), 0, sharing, disposition, options, ptr::null(), 0) }; unsafe { RtlNtStatusToDosError(status) }; unsafe { OwnedHandle::from_raw_handle(raw) }",
                3,
                "NtCreateFile",
                "disposition",
            ),
            (
                "windows_relative_sdk.rs",
                "query_directory",
                "pub(super) ",
                "unsafe { GetFileInformationByHandleEx(file.as_raw_handle(), FileIdBothDirectoryInfo, buffer.as_mut_ptr().cast(), bytes) }",
                1,
                "GetFileInformationByHandleEx",
                "FileIdBothDirectoryInfo",
            ),
            (
                "windows_relative_sdk.rs",
                "directory_buffer_bytes",
                "pub(super) ",
                "unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast(), length) }",
                1,
                "std::slice::from_raw_parts",
                "length",
            ),
            (
                "graphit-core/native_file_identity.rs",
                "query_file_id128",
                "",
                "unsafe { GetFileInformationByHandleEx(file.as_raw_handle(), FileIdInfo, (&mut information as *mut FILE_ID_INFO).cast(), bytes) }",
                1,
                "GetFileInformationByHandleEx",
                "FileIdInfo",
            ),
        ] {
            let declaration = format!("{visibility}fn {name}");
            let code = format!("#[allow(unsafe_code)] {declaration}() {{ {calls} }}");
            let actual = audit_in(source, &code);
            assert!(actual.errors.is_empty(), "{name}: {:?}", actual.errors);
            assert_eq!((actual.exceptions, actual.blocks), (1, blocks));
            assert!(!audit_in("another.rs", &code).errors.is_empty());
            // The same filename in the other crate does not gain this exception.
            let other_crate = if visibility.is_empty() {
                "native_file_identity.rs"
            } else {
                "graphit-core/windows_relative_sdk.rs"
            };
            assert!(!audit_in(other_crate, &code).errors.is_empty());
            for invalid in [
                code.replace(&declaration, &format!("pub fn {name}")),
                code.replace(&declaration, &format!("pub(crate) fn {name}")),
                code.replace(&declaration, &format!("pub(in super) fn {name}")),
                code.replace(
                    &declaration,
                    &format!(
                        "{}fn {name}",
                        if visibility.is_empty() {
                            "pub(super) "
                        } else {
                            ""
                        }
                    ),
                ),
                code.replace(&declaration, &format!("{visibility}unsafe fn {name}")),
                code.replace(&declaration, &format!("{visibility}fn unlisted")),
                code.replace(callee, "SubstitutedSdkCall"),
                code.replace(argument, "WrongArgument"),
                code.replace("unsafe {", "unsafe { unexpected();"),
                code.replace("() {", "() { unsafe {}"),
                code.replace("unsafe {", "unsafe { unsafe { extra() };"),
                code.replace("#[allow(unsafe_code)]", "#![allow(unsafe_code)]"),
                format!("mod nested {{ {code} }}"),
                format!("fn outside() {{ {code} }}"),
                format!("#[allow(unsafe_code)] mod widened {{ {code} }}"),
                format!("{code} extern \"system\" {{ fn manual_binding(); }}"),
                format!("{code} unsafe extern \"system\" {{ fn manual_binding(); }}"),
                code.replace("() {", "() { include!(\"unreviewed.rs\");"),
            ] {
                assert_ne!(invalid, code);
                assert!(
                    !audit_in(source, &invalid).errors.is_empty(),
                    "escaped {source}::{name}: {invalid}"
                );
            }
            if name == "open_relative" {
                for invalid in [
                    code.replace("RtlNtStatusToDosError", "KernelStatusConverter"),
                    code.replace("RtlNtStatusToDosError(status)", "RtlNtStatusToDosError(0)"),
                    code.replace(
                        "OwnedHandle::from_raw_handle",
                        "OtherOwner::from_raw_handle",
                    ),
                    code.replace(
                        "OwnedHandle::from_raw_handle(raw)",
                        "OwnedHandle::from_raw_handle(foreign)",
                    ),
                    code.replace("&attributes", "ptr::null()"),
                    code.replace("&mut status_block", "ptr::null_mut()"),
                    code.replace("sharing", "FILE_SHARE_DELETE"),
                    code.replace("options", "0"),
                ] {
                    assert!(
                        !audit_in(source, &invalid).errors.is_empty(),
                        "escaped {name}: {invalid}"
                    );
                }
            }
        }
    }
    #[test]
    fn windows_access_snapshot_keeps_private_handle_and_exact_sdk_calls() {
        let source = "windows_document_access.rs";
        let code = include_str!("../../crates/graphit-fs/src/windows_document_access.rs");
        let valid = audit_in(source, code);
        assert!(valid.errors.is_empty(), "{:?}", valid.errors);
        assert_eq!((valid.exceptions, valid.blocks), (1, 4));
        assert!(audit_in("lib.rs", "pub use windows_document_access::{WindowsDocumentAccess, windows_document_access_snapshot};").errors.is_empty());
        assert!(
            !audit_in(
                "lib.rs",
                "pub use windows_sys::Win32::Security::GetKernelObjectSecurity;"
            )
            .errors
            .is_empty()
        );
        for invalid in [
            code.replace("fn snapshot(", "pub fn snapshot("),
            code.replace("file.as_raw_handle()", "ptr::null_mut()"),
            code.replace("IsValidSecurityDescriptor", "OtherSdkCall"),
            code.replace("&mut control", "ptr::null_mut()"),
            code.replace("unsafe {", "unsafe { unexpected();"),
        ] {
            assert!(!audit_in(source, &invalid).errors.is_empty());
        }
    }

    #[test]
    fn core_and_fs_require_both_crate_safety_attributes() {
        let valid = "#![deny(unsafe_code)] #![forbid(unsafe_op_in_unsafe_fn)]";
        assert!(verify_crate_safety(&syn::parse_file(valid).unwrap()).is_ok());
        for invalid in [
            "#![forbid(unsafe_op_in_unsafe_fn)]",
            "#![deny(unsafe_code)]",
            "#![allow(unsafe_code)] #![forbid(unsafe_op_in_unsafe_fn)]",
            "#![deny(unsafe_code)] #![allow(unsafe_op_in_unsafe_fn)]",
        ] {
            assert!(verify_crate_safety(&syn::parse_file(invalid).unwrap()).is_err());
        }
    }

    #[test]
    fn recursive_core_walk_never_inherits_fs_or_pinned_sdk_exceptions() {
        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "graphit-core-safety-{}-{nonce}",
            std::process::id(),
        )));
        fs::create_dir(&fixture.0).unwrap();
        let identity = include_str!("../../crates/graphit-core/src/native_file_identity.rs");
        fs::write(fixture.0.join("native_file_identity.rs"), identity).unwrap();
        fs::create_dir(fixture.0.join("nested")).unwrap();
        fs::write(fixture.0.join("nested/safe.rs"), "fn safe() {} ").unwrap();
        let domain = SourceDomain::Core;
        let mut totals = (0, 0);
        inspect_directory(&fixture.0, &fixture.0, domain, &mut totals).unwrap();
        assert_eq!(totals, (1, 1));
        for (path, source) in [
            ("other.rs", "fn unauthorized() { unsafe { other() } }"),
            (
                "nested/hidden.rs",
                "#[allow(unsafe_code)] fn unauthorized() {}",
            ),
            ("nested/native_file_identity.rs", identity),
            (
                "windows_relative_sdk.rs",
                include_str!("../../crates/graphit-fs/src/windows_relative_sdk.rs"),
            ),
            (
                "macos_private_sdk.rs",
                include_str!("../../crates/graphit-fs/src/macos_private_sdk.rs"),
            ),
            (
                "windows.rs",
                "#[allow(unsafe_code)] fn set_junction() { unsafe { DeviceIoControl(file.as_raw_handle(), FSCTL_SET_REPARSE_POINT, buffer.as_ptr().cast(), buffer.len() as u32, std::ptr::null_mut(), 0, &mut returned, std::ptr::null_mut()) } }",
            ),
            ("manual.rs", "unsafe extern \"system\" { fn manual(); }"),
            (
                "macro.rs",
                "macro_rules! laundering { () => { unsafe { other() } }; }",
            ),
            ("blanket.rs", "#![allow(unsafe_code)]"),
        ] {
            let path = fixture.0.join(path);
            fs::write(&path, source).unwrap();
            assert!(
                inspect_directory(&fixture.0, &fixture.0, domain, &mut (0, 0)).is_err(),
                "escaped {}",
                path.display()
            );
            fs::remove_file(path).unwrap();
        }
        let fs_root = fixture.0.join("fs");
        fs::create_dir_all(fs_root.join("graphit-core")).unwrap();
        fs::write(
            fs_root.join("graphit-core/native_file_identity.rs"),
            identity,
        )
        .unwrap();
        assert!(
            inspect_directory(&fs_root, &fs_root, SourceDomain::Filesystem, &mut (0, 0)).is_err()
        );
        fs::remove_dir_all(fs_root).unwrap();
        for invalid in [
            identity.replace("fn query_file_id128", "pub(super) fn query_file_id128"),
            identity.replace("fn query_file_id128", "pub fn query_file_id128"),
            format!("mod nested {{ {identity} }}"),
        ] {
            fs::write(fixture.0.join("native_file_identity.rs"), invalid).unwrap();
            assert!(inspect_directory(&fixture.0, &fixture.0, domain, &mut (0, 0)).is_err());
        }
    }
}

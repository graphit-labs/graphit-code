//! Remove only the release files listed by the package's relative ownership receipt.
use graphit_core::{
    brand::Brand,
    file_lock::FileLock,
    installation_identity::{InstallationId, InstallationIdentityStore},
    project_paths::InstallationPaths,
};
use graphit_hub::policy::RequestContext;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

const RECEIPT: &str = "graphit-runtime/installed-files.json";
const OWNERS: &str = "graphit-runtime/installation-roots.json";
const USAGE_LOCK: &str = "graphit-runtime/package-usage.lock";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    root: PathBuf,
    unit: String,
}

pub(crate) struct Usage {
    root: PathBuf,
    owners: Vec<Owner>,
    _lock: FileLock,
}

pub(crate) fn usage(request: &RequestContext<'_>) -> io::Result<Option<Usage>> {
    let executable = fs::canonicalize(std::env::current_exe()?)?;
    let root = executable.parent().ok_or(io::ErrorKind::InvalidInput)?;
    if !regular(root, Path::new(RECEIPT))? {
        return Ok(None);
    }
    let lock = match FileLock::acquire_in_existing_parent_until(
        &root.join(USAGE_LOCK),
        Some(request.snapshot().required_deadline()?),
        request.cancellation(),
    ) {
        Ok(lock) => lock,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if !regular(root, Path::new(RECEIPT))? {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Graphit package was removed",
        ));
    }
    let owners = match graphit_core::local_files::read_regular(&root.join(OWNERS), 256 * 1024)? {
        Some((bytes, _)) => {
            graphit_fs::verify_private_file_path(&root.join(OWNERS))?;
            serde_json::from_slice::<Vec<Owner>>(&bytes).map_err(io::Error::other)?
        }
        None => Vec::new(),
    };
    if owners
        .iter()
        .any(|owner| !owner.root.is_absolute() || InstallationId::new(&owner.unit).is_err())
    {
        return Err(invalid("invalid registered installation root"));
    }
    Ok(Some(Usage {
        root: root.to_owned(),
        owners,
        _lock: lock,
    }))
}

/// Record every CLI path that can activate a packaged daemon.
pub(crate) fn bind(
    installation: &InstallationPaths,
    request: &RequestContext<'_>,
) -> io::Result<Option<Usage>> {
    let Some(mut usage) = usage(request)? else {
        return Ok(None);
    };
    let timeout = request.remaining()?.ok_or(io::ErrorKind::InvalidInput)?;
    let identity =
        InstallationIdentityStore::new(installation)?.ensure(timeout, request.cancellation())?;
    usage.register(installation, identity.unit_id())?;
    Ok(Some(usage))
}

impl Usage {
    fn register(
        &mut self,
        installation: &InstallationPaths,
        unit: &InstallationId,
    ) -> io::Result<()> {
        let root = fs::canonicalize(installation.root())?;
        if self
            .owners
            .iter()
            .any(|owner| owner.root == root && owner.unit == unit.as_str())
        {
            return Ok(());
        }
        self.owners.retain(|owner| owner.root != root);
        self.owners.push(Owner {
            root,
            unit: unit.as_str().to_owned(),
        });
        self.persist()
    }

    fn persist(&self) -> io::Result<()> {
        let path = self.root.join(OWNERS);
        let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let mut stage = tempfile::Builder::new()
            .prefix(".installation-roots-")
            .make_in(parent, graphit_fs::create_private_file)?;
        serde_json::to_writer(&mut stage, &self.owners).map_err(io::Error::other)?;
        stage.flush()?;
        stage.as_file().sync_all()?;
        stage
            .into_temp_path()
            .persist(path)
            .map_err(|error| error.error)
    }

    pub(super) fn plan_uninstall(
        &self,
        installation: &InstallationPaths,
        unit: Option<&InstallationId>,
        request: &RequestContext<'_>,
    ) -> io::Result<Removal> {
        let root = match fs::canonicalize(installation.root()) {
            Ok(root) => root,
            Err(error) if error.kind() == io::ErrorKind::NotFound => installation.root().to_owned(),
            Err(error) => return Err(error),
        };
        let matches = self
            .owners
            .iter()
            .any(|owner| owner.root == root && unit.is_none_or(|unit| unit.as_str() == owner.unit));
        if !matches {
            return Ok(Removal::Unregistered);
        }
        if self.owners.iter().any(|owner| owner.root != root) {
            return Ok(Removal::Shared(root));
        }
        let mut package = capture(installation.brand(), request)?.ok_or(io::ErrorKind::NotFound)?;
        if regular(&self.root, Path::new(OWNERS))? {
            let path = self.root.join(OWNERS);
            let sha256 = digest(&path, request)?;
            let end = package.files.len() - 1;
            package.files.insert(end, File { path, sha256 });
        }
        Ok(Removal::Owned(package))
    }

    fn release(&mut self, root: &Path) -> io::Result<()> {
        self.owners.retain(|owner| owner.root != root);
        self.persist()
    }
}

pub(super) enum Removal {
    Unmanaged,
    Unregistered,
    Shared(PathBuf),
    Owned(Package),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    files: BTreeMap<String, String>,
}

struct File {
    path: PathBuf,
    sha256: String,
}

pub(super) struct Package {
    root: PathBuf,
    files: Vec<File>,
    #[cfg(windows)]
    path_entry: Option<String>,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn digest(path: &Path, request: &RequestContext<'_>) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        request.check()?;
        let count = file.read(&mut bytes)?;
        if count == 0 {
            return Ok(hex(&hash.finalize()));
        }
        hash.update(&bytes[..count]);
    }
}

fn regular(root: &Path, relative: &Path) -> io::Result<bool> {
    let mut path = root.to_owned();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(invalid("invalid release file path"));
        };
        path.push(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(invalid("release file path contains a symbolic link"));
            }
            Ok(metadata) if components.peek().is_none() && metadata.is_file() => return Ok(true),
            Ok(metadata) if components.peek().is_some() && metadata.is_dir() => (),
            Ok(_) => return Err(invalid("release file path has an unexpected type")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        }
    }
    Err(invalid("empty release file path"))
}

fn package_path(relative: &str, executable: &str, model: &str) -> bool {
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || relative.chars().any(char::is_control)
    {
        return false;
    }
    if relative
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return false;
    }
    if relative == executable || relative == model {
        return true;
    }
    if relative.starts_with("graphit-runtime/") {
        return relative != RECEIPT
            && !Path::new(&relative.to_ascii_lowercase()).starts_with("graphit-runtime/broker")
            && !Path::new(&relative.to_ascii_lowercase())
                .starts_with("graphit-runtime/licenses/broker");
    }
    cfg!(windows) && !relative.contains('/') && relative.to_ascii_lowercase().ends_with(".dll")
}

pub(super) fn capture(brand: &Brand, request: &RequestContext<'_>) -> io::Result<Option<Package>> {
    let current = fs::canonicalize(std::env::current_exe()?)?;
    let root = current.parent().ok_or(io::ErrorKind::InvalidInput)?;
    if !regular(root, Path::new(RECEIPT))? {
        return Ok(None);
    }
    let executable = current
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(io::ErrorKind::InvalidInput)?;
    let expected_name = format!("{}{}", brand.binary_name(), std::env::consts::EXE_SUFFIX);
    if executable != expected_name {
        return Err(invalid(
            "release receipt does not match the running executable",
        ));
    }
    let model = format!(
        "{}-model-server{}",
        brand.binary_name(),
        std::env::consts::EXE_SUFFIX
    );
    let receipt = root.join(RECEIPT);
    let mut bytes = Vec::new();
    fs::File::open(&receipt)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if !manifest.files.contains_key(executable)
        || !manifest.files.contains_key(&model)
        || !manifest.files.contains_key("graphit-runtime/build.json")
    {
        return Err(invalid("release receipt is incomplete"));
    }
    let mut files = Vec::with_capacity(manifest.files.len() + 1);
    for (relative, sha256) in manifest.files {
        request.check()?;
        if !package_path(&relative, executable, &model)
            || sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(invalid("invalid release ownership receipt"));
        }
        let path = root.join(&relative);
        if regular(root, Path::new(&relative))? && digest(&path, request)? != sha256 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("release file changed: {relative}"),
            ));
        }
        files.push(File { path, sha256 });
    }
    // Keep the running binary and receipt available until the payload has been removed.
    files.sort_by_key(|file| file.path == current);
    #[cfg(windows)]
    let path_entry = {
        let marker = Path::new("graphit-runtime/user-path-added");
        if regular(root, marker)? {
            let path = root.join(marker);
            let mut entry = String::new();
            fs::File::open(&path)?
                .take(32769)
                .read_to_string(&mut entry)?;
            if entry.len() > 32768
                || !Path::new(&entry).is_absolute()
                || fs::canonicalize(&entry)? != root
            {
                return Err(invalid("invalid installer PATH receipt"));
            }
            files.insert(
                files.len() - 1,
                File {
                    path,
                    sha256: hex(&Sha256::digest(entry.as_bytes())),
                },
            );
            Some(entry)
        } else {
            None
        }
    };
    files.push(File {
        path: receipt,
        sha256: hex(&Sha256::digest(&bytes)),
    });
    Ok(Some(Package {
        root: root.to_owned(),
        files,
        #[cfg(windows)]
        path_entry,
    }))
}

pub(super) fn remove(
    package: Removal,
    usage: Option<&mut Usage>,
    request: &RequestContext<'_>,
    outcome: &mut Value,
) -> io::Result<()> {
    let package = match package {
        Removal::Unmanaged => {
            *outcome = json!({"state":"unmanaged"});
            return Ok(());
        }
        Removal::Unregistered => {
            *outcome = json!({"state":"unregistered"});
            return Ok(());
        }
        Removal::Shared(root) => {
            usage.ok_or(io::ErrorKind::InvalidInput)?.release(&root)?;
            *outcome = json!({"state":"shared"});
            return Ok(());
        }
        Removal::Owned(package) => package,
    };
    *outcome = json!({"state":"partial","files":0});
    #[cfg(windows)]
    {
        request.check()?;
        let failure_log = defer_windows(&package)?;
        *outcome = json!({"state":"scheduled_after_exit","files":package.files.len(),"failure_log":failure_log});
    }
    #[cfg(not(windows))]
    {
        let mut directories = std::collections::BTreeSet::new();
        let mut count = 0usize;
        for file in &package.files {
            request.check()?;
            let relative = file
                .path
                .strip_prefix(&package.root)
                .map_err(io::Error::other)?;
            if regular(&package.root, relative)? {
                if digest(&file.path, request)? != file.sha256 {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "release file changed during uninstall",
                    ));
                }
                fs::remove_file(&file.path)?;
                count += 1;
                outcome["files"] = json!(count);
            }
            directories.extend(
                file.path
                    .ancestors()
                    .skip(1)
                    .take_while(|parent| *parent != package.root)
                    .map(Path::to_owned),
            );
        }
        for directory in directories.into_iter().rev() {
            match fs::remove_dir(directory) {
                Ok(()) => (),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        outcome["state"] = "removed".into();
    }
    Ok(())
}

#[cfg(windows)]
fn defer_windows(package: &Package) -> io::Result<PathBuf> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    fn powershell_path(path: &Path) -> io::Result<String> {
        let text = path.to_str().ok_or(io::ErrorKind::InvalidInput)?;
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            Ok(format!(r"\\{unc}"))
        } else {
            Ok(text.strip_prefix(r"\\?\").unwrap_or(text).to_owned())
        }
    }
    let files = package
        .files
        .iter()
        .map(|file| Ok(json!({"path":powershell_path(&file.path)?, "sha256":file.sha256})))
        .collect::<io::Result<Vec<_>>>()?;
    let mut plan = tempfile::Builder::new()
        .prefix("graphit-uninstall-")
        .make_in(std::env::temp_dir(), graphit_fs::create_private_file)?;
    serde_json::to_writer(
        &mut plan,
        &json!({
            "root":powershell_path(&package.root)?,
            "files":files,
            "path_entry":package.path_entry,
            "lock":powershell_path(&package.root.join(USAGE_LOCK))?,
            "owners":powershell_path(&package.root.join(OWNERS))?,
        }),
    )
    .map_err(io::Error::other)?;
    plan.flush()?;
    let (_, plan) = plan.keep().map_err(io::Error::other)?;
    let script = r#"
$ErrorActionPreference = 'Stop'
$PlanPath = $env:GRAPHIT_UNINSTALL_PLAN
$Lock = $null
try {
    Wait-Process -Id ([int]$env:GRAPHIT_UNINSTALL_PID) -ErrorAction SilentlyContinue
    $Plan = Get-Content -Raw -LiteralPath $PlanPath | ConvertFrom-Json
    $Share = [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete
    $Lock = [IO.File]::Open($Plan.lock, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, $Share)
    $Deadline = [DateTime]::UtcNow.AddSeconds(30)
    while ($true) {
        try {
            $Lock.Lock(0, 1)
            break
        } catch [IO.IOException] {
            if ([DateTime]::UtcNow -ge $Deadline) { throw }
            Start-Sleep -Milliseconds 100
        }
    }
    $Owners = @($Plan.files | Where-Object { $_.path -eq $Plan.owners })
    if ((Test-Path -LiteralPath $Plan.owners) -ne ($Owners.Count -eq 1) -or
        ($Owners.Count -eq 1 -and (Get-FileHash -LiteralPath $Plan.owners -Algorithm SHA256).Hash -ne $Owners[0].sha256)) {
        throw 'Registered installations changed before package removal'
    }
    $Directories = [Collections.Generic.HashSet[string]]::new()
    foreach ($File in $Plan.files) {
        if (Test-Path -LiteralPath $File.path) {
            $Item = Get-Item -LiteralPath $File.path -Force
            if ($Item.PSIsContainer -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unexpected release file type' }
            if ((Get-FileHash -LiteralPath $File.path -Algorithm SHA256).Hash -ne $File.sha256) { throw 'Release file changed during uninstall' }
            $Directory = [IO.Path]::GetDirectoryName($File.path)
            while ($Directory -ne $Plan.root) {
                if ((Get-Item -LiteralPath $Directory -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unexpected release directory type' }
                [void]$Directories.Add($Directory)
                $Directory = [IO.Path]::GetDirectoryName($Directory)
            }
            Remove-Item -LiteralPath $File.path -Force
        }
    }
    $Directories | Sort-Object Length -Descending | ForEach-Object {
        if ((Test-Path -LiteralPath $_) -and @(Get-ChildItem -LiteralPath $_ -Force).Count -eq 0) {
            Remove-Item -LiteralPath $_ -Force
        }
    }
    if ($Plan.path_entry -and @(Get-ChildItem -LiteralPath $Plan.root -File -Force).Count -eq 0) {
        $UserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
        $Entries = @($UserPath -split ';' | Where-Object { $_ -ne $Plan.path_entry })
        [Environment]::SetEnvironmentVariable('PATH', ($Entries -join ';'), 'User')
    }
    Remove-Item -LiteralPath $PlanPath -Force
} catch {
    $_.Exception.Message | Set-Content -LiteralPath ($PlanPath + '.error')
    exit 1
} finally {
    if ($Lock) { $Lock.Dispose() }
}
"#;
    let system = std::env::var_os("SystemRoot").ok_or(io::ErrorKind::NotFound)?;
    let mut command = std::process::Command::new(
        PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
    );
    let launched = command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("GRAPHIT_UNINSTALL_PLAN", &plan)
        .env("GRAPHIT_UNINSTALL_PID", std::process::id().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x08000000)
        .spawn();
    match launched {
        Ok(_) => {
            let mut log = plan.into_os_string();
            log.push(".error");
            Ok(PathBuf::from(log))
        }
        Err(error) => {
            let _ = fs::remove_file(plan);
            Err(error)
        }
    }
}

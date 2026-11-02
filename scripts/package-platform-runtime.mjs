#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

// Native platform tools own dependency inspection. This only stages their resolved files.
export function packagePlatformRuntime(platform, stage, executables, { env = process.env, run, runtimeSubdir = 'graphit-runtime' } = {}) {
  if (!['darwin-arm64', 'windows-amd64'].includes(platform)) throw new Error('unsupported native runtime platform');
  const environment = { ...env, LC_ALL: 'C', VSLANG: '1033' };
  for (const key of ['DYLD_LIBRARY_PATH', 'DYLD_FALLBACK_LIBRARY_PATH', 'DYLD_INSERT_LIBRARIES']) delete environment[key];
  run ??= (command, args) => {
    const result = spawnSync(command, args, { encoding: 'utf8', env: environment });
    if (result.error || result.status !== 0) throw new Error(`native runtime tool failed: ${command}: ${result.error?.code ?? result.stderr?.trim()}`);
    return result.stdout;
  };
  stage = fs.realpathSync(stage);
  if (!executables.length || executables.some(name => path.basename(name) !== name || !fs.statSync(path.join(stage, name)).isFile())) {
    throw new Error('native runtime requires staged executable basenames');
  }
  if (!runtimeSubdir || !runtimeSubdir.split('/').every(part => /^[A-Za-z0-9_-]+$/.test(part))) throw new Error('runtime namespace must contain relative directory names');
  const runtime = path.join(stage, runtimeSubdir);
  const digest = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  const manifest = { platform, host_libraries: [], libraries: [], executables: [] };
  const inputExecutables = new Map(executables.map(name => [name, digest(path.join(stage, name))]));
  const regular = file => {
    if (!fs.statSync(file).isFile() || fs.statSync(file).size === 0) throw new Error('missing native runtime file');
    return fs.realpathSync(file);
  };
  const copy = (source, target) => {
    regular(source);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(source, target);
  };
  const notice = (name, text) => {
    const destination = path.join(runtime, 'licenses', platform, name);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(destination, text);
  };
  const openssl = env.OPENSSL_DIR ? fs.realpathSync(env.OPENSSL_DIR) : null;
  let opensslRetained = false;
  const retainOpenSSLNotice = () => {
    if (opensslRetained) return;
    const candidates = ['LICENSE.txt', 'LICENSE', 'share/doc/openssl@3/LICENSE.txt', 'share/openssl/copyright'];
    const source = candidates.map(name => path.join(openssl, name)).find(file => fs.existsSync(file) && fs.statSync(file).isFile());
    if (!source) throw new Error('missing OpenSSL distribution license');
    copy(source, path.join(runtime, 'licenses', platform, 'OpenSSL-LICENSE.txt'));
    opensslRetained = true;
  };
  const host = new Set();
  const retained = new Map();
  const minimumMacOS = new Map();
  const add = (name, source, target, origin) => {
    source = regular(source);
    const previous = retained.get(name);
    if (previous && previous.source !== source) throw new Error(`conflicting native runtime dependency: ${name}`);
    if (previous) return previous;
    copy(source, target);
    const entry = { source, target, origin, input_sha256: digest(source) };
    retained.set(name, entry);
    return entry;
  };
  if (platform === 'darwin-arm64') {
    const libraries = path.join(runtime, 'lib');
    const dependencies = file => run('otool', ['-L', file]).split(/\r?\n/).slice(1).filter(line => line.trim()).map(line => {
      const match = line.match(/^\s+(.+?) \(compatibility version /);
      if (!match) throw new Error('unrecognized Mach-O dependency');
      return match[1];
    });
    const system = name => name.startsWith('/usr/lib/') || name.startsWith('/System/Library/');
    const select = name => {
      const basename = path.posix.basename(name);
      if (!openssl || !/^lib(?:ssl|crypto)\.3\.dylib$/.test(basename)) throw new Error(`unsupported macOS runtime dependency: ${name}`);
      const source = regular(path.join(openssl, 'lib', basename));
      if (!source.startsWith(openssl + path.sep)) throw new Error('OpenSSL runtime escapes explicit prefix');
      if (name.startsWith('/') && regular(name) !== source) throw new Error('OpenSSL Mach-O source differs from explicit prefix');
      retainOpenSSLNotice();
      return add(basename, source, path.join(libraries, basename), 'OpenSSL 3; explicit OPENSSL_DIR');
    };
    const visit = (file, executable) => {
      if (run('lipo', ['-archs', file]).trim() !== 'arm64') throw new Error('release Mach-O must target arm64 only');
      for (const dependency of dependencies(file)) {
        if (system(dependency)) { host.add(dependency); continue; }
        // A dylib lists its own install name first.
        if (!executable && path.posix.basename(dependency) === path.basename(file)) continue;
        const entry = select(dependency);
        const replacement = executable ? `@loader_path/${runtimeSubdir}/lib/${path.basename(entry.target)}` : `@loader_path/${path.basename(entry.target)}`;
        run('install_name_tool', ['-change', dependency, replacement, file]);
      }
      const commands = run('otool', ['-l', file]);
      const versions = [
        ...commands.matchAll(/\bcmd LC_BUILD_VERSION\s+cmdsize \d+\s+platform (?:1|macos)\s+minos (\d+(?:\.\d+){1,2})\b/gi),
        ...commands.matchAll(/\bcmd LC_VERSION_MIN_MACOSX\s+cmdsize \d+\s+version (\d+(?:\.\d+){1,2})\b/g),
      ];
      if (versions.length !== 1) throw new Error('expected one macOS minimum version load command');
      minimumMacOS.set(file, versions[0][1]);
      for (const match of commands.matchAll(/\bcmd LC_RPATH\s+cmdsize \d+\s+path (.+?) \(offset \d+\)/g)) {
        if (!match[1].startsWith('@loader_path') && !match[1].startsWith('@executable_path')) run('install_name_tool', ['-delete_rpath', match[1], file]);
      }
      if (!executable) run('install_name_tool', ['-id', `@loader_path/${path.basename(file)}`, file]);
    };
    for (const name of executables) visit(path.join(stage, name), true);
    for (const entry of retained.values()) visit(entry.target, false);
    const files = [...retained.values()].map(entry => entry.target).concat(executables.map(name => path.join(stage, name)));
    for (const file of files) {
      run('codesign', ['--force', '--sign', '-', file]);
      run('codesign', ['--verify', '--strict', file]);
      for (const dependency of dependencies(file)) {
        if (system(dependency)) continue;
        if (!dependency.startsWith('@loader_path/')) throw new Error('nonrelocatable packaged Mach-O dependency');
        const resolved = regular(path.resolve(path.dirname(file), dependency.slice('@loader_path/'.length)));
        if (!resolved.startsWith(stage + path.sep)) throw new Error('Mach-O dependency escapes package');
      }
    }
  } else {
    // UCRT and Windows API contracts are OS components on supported Windows 10/11.
    const system = new Set(['advapi32.dll', 'avrt.dll', 'bcrypt.dll', 'bcryptprimitives.dll', 'cabinet.dll', 'cfgmgr32.dll', 'combase.dll', 'comctl32.dll', 'comdlg32.dll', 'crypt32.dll', 'cryptbase.dll', 'cryptnet.dll', 'cryptui.dll', 'd3d11.dll', 'dbghelp.dll', 'dnsapi.dll', 'dwmapi.dll', 'dxgi.dll', 'gdi32.dll', 'hid.dll', 'imm32.dll', 'iphlpapi.dll', 'kernel32.dll', 'msimg32.dll', 'msvcrt.dll', 'ncrypt.dll', 'netapi32.dll', 'normaliz.dll', 'ntdll.dll', 'ole32.dll', 'oleaut32.dll', 'powrprof.dll', 'propsys.dll', 'psapi.dll', 'rpcrt4.dll', 'secur32.dll', 'setupapi.dll', 'shell32.dll', 'shlwapi.dll', 'user32.dll', 'userenv.dll', 'usp10.dll', 'ucrtbase.dll', 'uxtheme.dll', 'version.dll', 'wevtapi.dll', 'winhttp.dll', 'wininet.dll', 'winmm.dll', 'winspool.drv', 'wintrust.dll', 'wldap32.dll', 'ws2_32.dll', 'wtsapi32.dll']);
    const crtRoot = env.VCToolsRedistDir ? path.join(fs.realpathSync(env.VCToolsRedistDir), 'x64', 'Microsoft.VC143.CRT') : null;
    let crtRetained = false;
    const dependencies = file => {
      if (!/8664 machine \(x64\)/i.test(run('dumpbin', ['/nologo', '/headers', file]))) throw new Error('release PE must target x64');
      const output = run('dumpbin', ['/nologo', '/dependents', file]);
      if (!/Image has the following (?:delay load )?dependencies:/i.test(output)) {
        if (/Summary/.test(output)) return [];
        throw new Error('unrecognized PE dependency output');
      }
      return [...output.matchAll(/^\s+([A-Za-z0-9_.-]+\.(?:dll|drv))\s*$/gim)].map(match => match[1].toLowerCase());
    };
    const select = name => {
      if (/^lib(?:ssl|crypto)-3-x64\.dll$/.test(name)) {
        if (!openssl) throw new Error('missing explicit OPENSSL_DIR for Windows runtime');
        const source = regular(path.join(openssl, 'bin', name));
        if (!source.startsWith(openssl + path.sep)) throw new Error('OpenSSL runtime escapes explicit prefix');
        retainOpenSSLNotice();
        return add(name, source, path.join(stage, name), 'OpenSSL 3; explicit OPENSSL_DIR');
      }
      if (/^(?:msvcp140(?:_1|_2|_atomic_wait|_codecvt_ids)?|vcruntime140(?:_1)?|concrt140)\.dll$/.test(name)) {
        if (!crtRoot) throw new Error('missing explicit VCToolsRedistDir for app-local CRT');
        const source = regular(path.join(crtRoot, name));
        const entry = add(name, source, path.join(stage, name), 'Microsoft Visual C++ 2022 x64 redistributable; VC143.CRT');
        if (!crtRetained) {
          notice('Microsoft-VC-REDIST-NOTICE.txt', 'Microsoft Visual C++ Redistributable for Visual Studio 2022 (x64), app-local deployment.\nCopyright Microsoft Corporation. All rights reserved.\nRedistribution by the licensed build producer is subject to the Microsoft Visual Studio license terms and its Distributable Code list:\nhttps://learn.microsoft.com/visualstudio/releases/2022/redistribution\nhttps://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files\nOnly the release VC143.CRT files required by this package are included. Microsoft Windows UCRT/API DLLs are supplied by the operating system.\n');
          if (env.GRAPHIT_MSVC_REDIST_LICENSE) copy(env.GRAPHIT_MSVC_REDIST_LICENSE, path.join(runtime, 'licenses', platform, 'Microsoft-VC-REDIST-LICENSE.txt'));
          crtRetained = true;
        }
        return entry;
      }
      throw new Error(`unsupported Windows runtime dependency: ${name}`);
    };
    const visit = file => {
      for (const dependency of dependencies(file)) {
        if (system.has(dependency) || /^(?:api|ext)-ms-win-[a-z0-9-]+\.dll$/.test(dependency)) { host.add(dependency); continue; }
        select(dependency);
      }
    };
    for (const name of executables) visit(path.join(stage, name));
    for (const entry of retained.values()) visit(entry.target);
  }
  if (platform === 'darwin-arm64') {
    manifest.minimum_macos = [...minimumMacOS.values()].sort((a, b) => {
      const left = a.split('.').map(Number), right = b.split('.').map(Number);
      for (let index = 0; index < 3; index++) {
        const order = (left[index] ?? 0) - (right[index] ?? 0);
        if (order) return order;
      }
      return 0;
    }).at(-1);
  }
  const platformMinimum = file => platform === 'darwin-arm64' ? { minimum_macos: minimumMacOS.get(file) } : {};
  manifest.host_libraries = [...host].sort();
  manifest.libraries = [...retained.entries()].map(([name, entry]) => ({ name, file: path.relative(stage, entry.target).split(path.sep).join('/'), origin: entry.origin, input_sha256: entry.input_sha256, sha256: digest(entry.target), ...platformMinimum(entry.target) }));
  manifest.executables = executables.map(name => ({ file: name, input_sha256: inputExecutables.get(name), sha256: digest(path.join(stage, name)), ...platformMinimum(path.join(stage, name)) }));
  fs.mkdirSync(runtime, { recursive: true });
  fs.writeFileSync(path.join(runtime, `${platform === 'darwin-arm64' ? 'macos' : 'windows'}-libraries.json`), JSON.stringify(manifest, null, 2) + '\n');
  return manifest;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const [platform, stage, runtimeSubdir, ...executables] = process.argv.slice(2);
  packagePlatformRuntime(platform, stage, executables, { runtimeSubdir });
}

#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { packagePlatformRuntime } from './package-platform-runtime.mjs';

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'graphit-platform-package-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const stage = path.join(root, 'payload');
  fs.mkdirSync(stage);
  const file = (relative, value = 'native-fixture') => {
    const target = path.join(root, relative);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, value);
    return target;
  };
  file('payload/graphit'); file('payload/graphit-model-server'); file('payload/graphit.exe');
  const openssl = path.join(root, 'openssl');
  file('openssl/LICENSE.txt', 'OpenSSL test license');
  return { root, stage, file, openssl };
}
function windowsTools(deps, headers = '8664 machine (x64)') {
  return (command, args) => {
    assert.equal(command, 'dumpbin');
    if (args.includes('/headers')) return headers;
    const name = path.basename(args.at(-1)).toLowerCase();
    return `Microsoft (R) COFF/PE Dumper\nImage has the following dependencies:\n${(deps[name] ?? []).map(dep => `    ${dep}`).join('\n')}\n  Summary\n`;
  };
}
function macTools(initial, calls, minimums = {}) {
  const deps = new Map(Object.entries(initial));
  return (command, args) => {
    calls.push([command, ...args]);
    const file = args.at(-1), name = path.basename(file);
    if (command === 'lipo') return 'arm64\n';
    if (command === 'otool' && args[0] === '-L') {
      const list = deps.get(file) ?? deps.get(name) ?? [];
      return `${file}:\n${list.map(dep => `\t${dep} (compatibility version 1.0.0, current version 1.0.0)`).join('\n')}\n`;
    }
    if (command === 'otool') return 'Load command 0\n          cmd LC_RPATH\n      cmdsize 48\n         path /opt/homebrew/lib (offset 12)\n' + (minimums[name] ?? 'Load command 1\n          cmd LC_BUILD_VERSION\n      cmdsize 32\n     platform 1\n        minos 13.0\n          sdk 14.0\n');
    if (command === 'install_name_tool' && args[0] === '-change') {
      const list = deps.get(file) ?? deps.get(name) ?? [];
      deps.set(file, list.map(dep => dep === args[1] ? args[2] : dep));
    } else if (command === 'install_name_tool' && args[0] === '-id') {
      const list = deps.get(file) ?? deps.get(name) ?? [];
      deps.set(file, list.map(dep => path.basename(dep) === name ? args[1] : dep));
    } else assert.ok(command === 'install_name_tool' || command === 'codesign');
    return '';
  };
}

test('Windows stages recursive OpenSSL and VC143 CRT app-local, with exact hashes and notices', t => {
  const f = fixture(t);
  f.file('openssl/bin/libssl-3-x64.dll'); f.file('openssl/bin/libcrypto-3-x64.dll');
  f.file('redist/x64/Microsoft.VC143.CRT/vcruntime140.dll');
  f.file('redist/x64/Microsoft.VC143.CRT/vcruntime140_1.dll');
  const run = windowsTools({ 'graphit.exe': ['KERNEL32.dll', 'libssl-3-x64.dll', 'VCRUNTIME140.dll'], 'libssl-3-x64.dll': ['libcrypto-3-x64.dll'], 'libcrypto-3-x64.dll': ['VCRUNTIME140_1.dll', 'api-ms-win-crt-heap-l1-1-0.dll'] });
  const manifest = packagePlatformRuntime('windows-amd64', f.stage, ['graphit.exe'], { env: { OPENSSL_DIR: f.openssl, VCToolsRedistDir: path.join(f.root, 'redist') }, run });
  assert.equal(manifest.libraries.length, 4);
  assert.equal(manifest.executables[0].input_sha256, manifest.executables[0].sha256);
  for (const entry of manifest.libraries) { assert.equal(entry.file, entry.name); assert.match(entry.sha256, /^[0-9a-f]{64}$/); assert.ok(fs.existsSync(path.join(f.stage, entry.file))); }
  assert.ok(fs.existsSync(path.join(f.stage, 'graphit-runtime/licenses/windows-amd64/OpenSSL-LICENSE.txt')));
  assert.ok(fs.existsSync(path.join(f.stage, 'graphit-runtime/licenses/windows-amd64/Microsoft-VC-REDIST-NOTICE.txt')));
  assert.ok(!JSON.stringify(manifest).includes(f.root));
});
for (const dependency of ['evil.dll', 'msvcp140d.dll', 'libopenblas.dll']) {
  test(`Windows rejects unexpected/debug/dynamic BLAS dependency ${dependency}`, t => {
    const f = fixture(t);
    assert.throws(() => packagePlatformRuntime('windows-amd64', f.stage, ['graphit.exe'], { env: {}, run: windowsTools({ 'graphit.exe': [dependency] }) }), /unsupported Windows runtime dependency/);
  });
}
test('Windows fails for missing app-local CRT source rather than resolving host PATH', t => {
  const f = fixture(t);
  assert.throws(() => packagePlatformRuntime('windows-amd64', f.stage, ['graphit.exe'], { env: {}, run: windowsTools({ 'graphit.exe': ['VCRUNTIME140.dll'] }) }), /VCToolsRedistDir/);
});
test('Windows OS-only Broker needs no OpenSSL or VC redistributable configuration', t => {
  const f = fixture(t);
  const m = packagePlatformRuntime('windows-amd64', f.stage, ['graphit.exe'], { env: {}, runtimeSubdir: 'graphit-runtime/broker', run: windowsTools({ 'graphit.exe': ['BCRYPT.dll', 'ucrtbase.dll'] }) });
  assert.equal(m.libraries.length, 0);
  assert.ok(fs.existsSync(path.join(f.stage, 'graphit-runtime/broker/windows-libraries.json')));
});
test('Windows rejects wrong machine', t => {
  const f = fixture(t);
  assert.throws(() => packagePlatformRuntime('windows-amd64', f.stage, ['graphit.exe'], { env: {}, run: windowsTools({}, '14C machine (x86)') }), /x64/);
});
test('macOS bundles OpenSSL recursively, rewrites relative load commands, removes host rpaths and signs', t => {
  const f = fixture(t), calls = [];
  const ssl = f.file('openssl/lib/libssl.3.dylib'), crypto = f.file('openssl/lib/libcrypto.3.dylib');
  const run = macTools({ graphit: [ssl, '/usr/lib/libc++.1.dylib'], 'graphit-model-server': ['/System/Library/Frameworks/Accelerate.framework/Versions/A/Accelerate'], 'libssl.3.dylib': [ssl, crypto], 'libcrypto.3.dylib': [crypto, '/usr/lib/libSystem.B.dylib'] }, calls);
  const m = packagePlatformRuntime('darwin-arm64', f.stage, ['graphit', 'graphit-model-server'], { env: { OPENSSL_DIR: f.openssl }, run });
  assert.equal(m.libraries.length, 2);
  assert.ok(calls.some(c => c[0] === 'install_name_tool' && c[1] === '-change' && c[3] === '@loader_path/graphit-runtime/lib/libssl.3.dylib'));
  assert.ok(calls.some(c => c[0] === 'install_name_tool' && c[1] === '-delete_rpath'));
  assert.equal(calls.filter(c => c[0] === 'codesign' && c[1] === '--verify').length, 4);
  assert.ok(!JSON.stringify(m).includes(f.root));
});
test('macOS rejects undeclared Homebrew dylibs', t => {
  const f = fixture(t);
  assert.throws(() => packagePlatformRuntime('darwin-arm64', f.stage, ['graphit'], { env: {}, run: macTools({ graphit: ['/opt/homebrew/lib/libomp.dylib'] }, []) }), /unsupported macOS runtime dependency/);
});
test('macOS system-only Broker remains relocatable without third-party installation', t => {
  const f = fixture(t);
  const m = packagePlatformRuntime('darwin-arm64', f.stage, ['graphit'], { env: {}, runtimeSubdir: 'graphit-runtime/broker', run: macTools({ graphit: ['/usr/lib/libc++.1.dylib', '/usr/lib/libSystem.B.dylib'] }, []) });
  assert.equal(m.libraries.length, 0);
  assert.ok(fs.existsSync(path.join(f.stage, 'graphit-runtime/broker/macos-libraries.json')));
});
test('runtime namespace rejects directory escape', t => {
  const f = fixture(t);
  assert.throws(() => packagePlatformRuntime('darwin-arm64', f.stage, ['graphit'], { runtimeSubdir: '../outside' }), /namespace/);
});

test('Windows also stages delay-load dependencies rather than dropping their imports', t => {
  const f = fixture(t);
  f.file('redist/x64/Microsoft.VC143.CRT/vcruntime140.dll');
  const base = windowsTools({});
  const run = (command, args) => args.includes('/dependents') && args.at(-1).endsWith('graphit.exe')
    ? 'Image has the following delay load dependencies:\n    vcruntime140.dll\n  Summary\n'
    : base(command, args);
  const m = packagePlatformRuntime('windows-amd64', f.stage, ['graphit.exe'], { env: { VCToolsRedistDir: path.join(f.root, 'redist') }, run });
  assert.equal(m.libraries[0].name, 'vcruntime140.dll');
});

test('macOS package minimum is the highest observed executable or bundled dylib minimum', t => {
  const f = fixture(t), calls = [];
  const crypto = f.file('openssl/lib/libcrypto.3.dylib');
  const run = macTools({ graphit: [crypto], 'libcrypto.3.dylib': [crypto, '/usr/lib/libSystem.B.dylib'] }, calls, {
    graphit: 'Load command 1\n          cmd LC_VERSION_MIN_MACOSX\n      cmdsize 16\n      version 12.3\n          sdk 14.0\n',
    'libcrypto.3.dylib': 'Load command 1\n          cmd LC_BUILD_VERSION\n      cmdsize 32\n     platform macos\n        minos 14.2.1\n          sdk 15.0\n',
  });
  const m = packagePlatformRuntime('darwin-arm64', f.stage, ['graphit'], { env: { OPENSSL_DIR: f.openssl }, run });
  assert.equal(m.minimum_macos, '14.2.1');
  assert.equal(m.executables[0].minimum_macos, '12.3');
  assert.equal(m.libraries[0].minimum_macos, '14.2.1');
});
test('macOS fails if the native load commands do not establish its minimum OS', t => {
  const f = fixture(t);
  assert.throws(() => packagePlatformRuntime('darwin-arm64', f.stage, ['graphit'], { env: {}, run: macTools({ graphit: ['/usr/lib/libSystem.B.dylib'] }, [], { graphit: '' }) }), /minimum version load command/);
});

test('macOS preserves original executable digest separately from its patched and signed payload', t => {
  const f = fixture(t), base = macTools({ graphit: ['/usr/lib/libSystem.B.dylib'] }, []);
  const run = (command, args) => {
    if (command === 'codesign' && args[0] === '--force') fs.appendFileSync(args.at(-1), '-signed');
    return base(command, args);
  };
  const m = packagePlatformRuntime('darwin-arm64', f.stage, ['graphit'], { env: {}, run });
  assert.match(m.executables[0].input_sha256, /^[0-9a-f]{64}$/);
  assert.notEqual(m.executables[0].input_sha256, m.executables[0].sha256);
});

// Generate the three shipped assets. Ordinary Cargo builds only embed these versioned bytes.
import { spawnSync } from 'node:child_process'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../', import.meta.url))
const build = spawnSync(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['--prefix', 'ui', 'run', 'build'], { cwd: root, stdio: 'inherit', shell: process.platform === 'win32' })
if (build.error) throw build.error
if (build.status !== 0) process.exit(build.status ?? 1)
const names = ['index.html', 'logo.svg', 'favicon.svg']
const assets = await Promise.all(names.map(async name => {
  const bytes = await readFile(new URL('../ui/dist/' + name, import.meta.url))
  const text = bytes.toString('utf8')
  if (name === 'index.html' ? !text.includes('<script') || !text.includes('id="root"') || !text.toLowerCase().includes('<!doctype html>') : !text.includes('<svg')) throw new Error('Invalid Vite asset: ' + name)
  return bytes
}))
const destination = new URL('../crates/graphit-app/assets/ui/', import.meta.url)
await mkdir(destination, { recursive: true })
for (let index = 0; index < names.length; index++) await writeFile(new URL(names[index], destination), assets[index])
console.log('Shipped Vite assets: ' + names.join(', '))

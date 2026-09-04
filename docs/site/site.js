const root = document.documentElement
const storedTheme = localStorage.getItem('graphit-site-theme')
if (storedTheme === 'light' || storedTheme === 'dark') root.dataset.theme = storedTheme

const themeToggle = document.querySelector('#theme-toggle')
themeToggle?.addEventListener('click', () => {
  const next = root.dataset.theme === 'light' ? 'dark' : 'light'
  root.dataset.theme = next
  localStorage.setItem('graphit-site-theme', next)
})

const menuToggle = document.querySelector('#menu-toggle')
const navigation = document.querySelector('#site-nav')
menuToggle?.addEventListener('click', () => {
  const open = navigation?.classList.toggle('is-open') ?? false
  menuToggle.setAttribute('aria-expanded', String(open))
})
navigation?.querySelectorAll('a').forEach((link) => {
  link.addEventListener('click', () => {
    navigation.classList.remove('is-open')
    menuToggle?.setAttribute('aria-expanded', 'false')
  })
})

const installConsole = document.querySelector('.install-console')
const tabs = [...document.querySelectorAll('.install-tab')]
const panels = installConsole ? [...installConsole.querySelectorAll('.command-panel')] : []
const activateTab = (tab) => {
    tabs.forEach((item) => {
      const active = item === tab
      item.classList.toggle('is-active', active)
      item.setAttribute('aria-selected', String(active))
      item.tabIndex = active ? 0 : -1
    })
    panels.forEach((panel) => {
      const active = panel.dataset.panel === tab.dataset.tab
      panel.classList.toggle('is-active', active)
      panel.hidden = !active
    })
}
tabs.forEach((tab, index) => {
  tab.addEventListener('click', () => activateTab(tab))
  tab.addEventListener('keydown', (event) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
    event.preventDefault()
    const nextIndex = event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? tabs.length - 1
        : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length
    tabs[nextIndex].focus()
    activateTab(tabs[nextIndex])
  })
})

document.querySelectorAll('.copy-button').forEach((button) => {
  button.addEventListener('click', async () => {
    const code = button.parentElement?.querySelector('code')?.textContent ?? ''
    try {
      await navigator.clipboard.writeText(code)
      button.textContent = 'Copied'
      window.setTimeout(() => { button.textContent = 'Copy' }, 1600)
    } catch {
      button.textContent = 'Select text'
    }
  })
})

const reveal = new IntersectionObserver((entries) => {
  entries.forEach((entry) => {
    if (entry.isIntersecting) {
      entry.target.classList.add('is-visible')
      reveal.unobserve(entry.target)
    }
  })
}, { threshold: 0.08 })
document.querySelectorAll('.reveal').forEach((element) => reveal.observe(element))

const year = document.querySelector('#year')
if (year) year.textContent = String(new Date().getFullYear())

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

// Scoped to the install console on purpose. A global '.command-panel' query also collects the
// standalone snippets elsewhere on the page, and since those carry no data-panel the tab handler
// would compute active === false and hide them the first time anyone clicked a tab.
const installConsole = document.querySelector('.install-console')
const tabs = [...document.querySelectorAll('.install-tab')]
const panels = installConsole ? [...installConsole.querySelectorAll('.command-panel')] : []
tabs.forEach((tab) => {
  tab.addEventListener('click', () => {
    tabs.forEach((item) => {
      const active = item === tab
      item.classList.toggle('is-active', active)
      item.setAttribute('aria-selected', String(active))
    })
    panels.forEach((panel) => {
      const active = panel.dataset.panel === tab.dataset.tab
      panel.classList.toggle('is-active', active)
      panel.hidden = !active
    })
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

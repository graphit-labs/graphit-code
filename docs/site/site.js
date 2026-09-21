const root = document.documentElement;
try {
  const stored = localStorage.getItem("graphit-site-theme");
  if (stored === "light" || stored === "dark") root.dataset.theme = stored;
} catch {
  /* Local theme remains usable without storage. */
}
document.querySelector("#theme-toggle")?.addEventListener("click", () => {
  root.dataset.theme = root.dataset.theme === "dark" ? "light" : "dark";
  try {
    localStorage.setItem("graphit-site-theme", root.dataset.theme);
  } catch {
    /* Optional persistence. */
  }
});
const menu = document.querySelector("#menu-toggle");
const nav = document.querySelector("#site-nav");
const closeMenu = () => {
  nav?.classList.remove("is-open");
  menu?.setAttribute("aria-expanded", "false");
};
menu?.addEventListener("click", () => {
  const open = nav?.classList.toggle("is-open");
  menu.setAttribute("aria-expanded", String(!!open));
});
nav
  ?.querySelectorAll("a")
  .forEach((link) => link.addEventListener("click", closeMenu));
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && nav?.classList.contains("is-open")) {
    closeMenu();
    menu?.focus();
  }
});
function wireTabs(containerSelector, tabSelector) {
  const container = document.querySelector(containerSelector);
  const tabs = [...(container?.querySelectorAll(tabSelector) || [])];
  const activate = (tab) =>
    tabs.forEach((item) => {
      const selected = item === tab;
      item.setAttribute("aria-selected", String(selected));
      item.tabIndex = selected ? 0 : -1;
      item.classList.toggle("is-active", selected);
      const panel = document.getElementById(item.getAttribute("aria-controls"));
      if (panel) {
        panel.hidden = !selected;
        panel.classList.toggle("is-active", selected);
      }
    });
  tabs.forEach((tab, index) => {
    tab.addEventListener("click", () => activate(tab));
    tab.addEventListener("keydown", (event) => {
      if (
        ![
          "ArrowLeft",
          "ArrowRight",
          "ArrowUp",
          "ArrowDown",
          "Home",
          "End",
        ].includes(event.key)
      )
        return;
      event.preventDefault();
      const next =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? tabs.length - 1
            : (index +
                (["ArrowRight", "ArrowDown"].includes(event.key) ? 1 : -1) +
                tabs.length) %
              tabs.length;
      activate(tabs[next]);
      tabs[next].focus();
    });
  });
}
wireTabs(".install-console", ".install-tab");
wireTabs(".adoption-layout", '[role="tab"]');
document.querySelectorAll(".copy-button").forEach((button) =>
  button.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(
        button.parentElement.querySelector("code")?.textContent || "",
      );
      button.textContent = "Copied";
      setTimeout(() => {
        button.textContent = "Copy";
      }, 1600);
    } catch {
      button.textContent = "Select text to copy";
    }
  }),
);
const year = document.querySelector("#year");
if (year) year.textContent = String(new Date().getFullYear());

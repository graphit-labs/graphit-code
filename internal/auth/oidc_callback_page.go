package auth

import (
	"html"
	"io"
	"net/http"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const oidcCallbackPageStart = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="color-scheme" content="light dark">
  <meta name="referrer" content="no-referrer">
  <title>__PRODUCT_NAME__ authentication</title>
  <style>
    :root {
      color-scheme: light dark;
      --background: #f7f5ed;
      --foreground: #1d232b;
      --card: #fffef9;
      --muted: #667078;
      --border: #c9cdd0;
      --primary: #118d5c;
      --primary-strong: #087047;
      --accent: #b9fb63;
      --sidebar: #101311;
      --grid: rgb(29 35 43 / 0.04);
      --shadow: rgb(29 35 43 / 0.16);
      --danger: #c53e35;
      --danger-soft: #fff0ec;
    }

    * { box-sizing: border-box; }

    html, body { min-width: 320px; min-height: 100%; }

    body {
      min-height: 100vh;
      margin: 0;
      display: grid;
      place-items: center;
      overflow-x: hidden;
      color: var(--foreground);
      background:
        radial-gradient(circle at 82% 12%, rgb(17 141 92 / 0.13), transparent 24rem),
        linear-gradient(var(--grid) 1px, transparent 1px),
        linear-gradient(90deg, var(--grid) 1px, transparent 1px),
        var(--background);
      background-size: auto, 32px 32px, 32px 32px, auto;
      font-family: Manrope, Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      font-size: 16px;
      line-height: 1.55;
      -webkit-font-smoothing: antialiased;
    }

    body::before {
      position: fixed;
      inset: 0;
      content: "";
      pointer-events: none;
      background: linear-gradient(135deg, transparent 18%, rgb(17 141 92 / 0.035), transparent 72%);
    }

    .shell {
      position: relative;
      width: min(100% - 2rem, 580px);
      margin: 2rem auto;
      overflow: hidden;
      border: 1px solid rgb(201 205 208 / 0.82);
      border-radius: 18px;
      background: rgb(255 254 249 / 0.96);
      box-shadow: 0 30px 80px -42px var(--shadow), 0 1px 0 rgb(255 255 255 / 0.85) inset;
    }

    .brand {
      display: flex;
      align-items: center;
      gap: 0.8rem;
      min-height: 82px;
      padding: 1.1rem 1.35rem;
      color: #f4f4ed;
      background:
        radial-gradient(circle at 12% 0%, rgb(185 251 99 / 0.11), transparent 15rem),
        linear-gradient(180deg, #161a18 0%, var(--sidebar) 100%);
      border-bottom: 1px solid rgb(255 255 255 / 0.09);
    }

    .glyph {
      position: relative;
      width: 42px;
      height: 42px;
      flex: 0 0 auto;
      overflow: hidden;
      border: 1px solid rgb(179 255 101 / 0.48);
      border-radius: 10px;
      background: var(--accent);
      box-shadow: 0 0 0 4px rgb(185 251 99 / 0.06);
    }

    .glyph::before {
      position: absolute;
      inset: 8px;
      content: "";
      border: 1px solid rgb(16 19 17 / 0.4);
      border-radius: 50%;
      box-shadow: -13px 12px 0 -10px var(--sidebar), 14px -9px 0 -10px var(--sidebar);
    }

    .glyph::after {
      position: absolute;
      left: 8px;
      bottom: 8px;
      width: 6px;
      height: 6px;
      content: "";
      border-radius: 50%;
      background: var(--sidebar);
    }

    .wordmark { min-width: 0; overflow: hidden; }

    .wordmark strong {
      display: block;
      font-size: 1.08rem;
      font-weight: 800;
      line-height: 1.1;
      letter-spacing: -0.04em;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .wordmark span, .edition, .eyebrow, .footer {
      font-family: "IBM Plex Mono", "SFMono-Regular", Consolas, "Liberation Mono", monospace;
      text-transform: uppercase;
      letter-spacing: 0.16em;
    }

    .wordmark span {
      display: block;
      margin-top: 0.35rem;
      color: rgb(185 251 99 / 0.72);
      font-size: 0.51rem;
      font-weight: 600;
    }

    .edition {
      margin-left: auto;
      align-self: flex-start;
      color: rgb(255 255 255 / 0.3);
      font-size: 0.5rem;
    }

    .content { padding: clamp(2rem, 7vw, 3.25rem); }

    .status-icon {
      position: relative;
      width: 58px;
      height: 58px;
      margin-bottom: 1.45rem;
      border: 1px solid rgb(17 141 92 / 0.42);
      border-radius: 15px;
      color: var(--primary-strong);
      background: rgb(17 141 92 / 0.1);
      box-shadow: 8px 8px 0 rgb(17 141 92 / 0.09);
    }

    .status-icon::before {
      position: absolute;
      left: 20px;
      top: 14px;
      width: 14px;
      height: 24px;
      content: "";
      border-right: 3px solid currentColor;
      border-bottom: 3px solid currentColor;
      transform: rotate(42deg);
    }

    .error .status-icon {
      color: var(--danger);
      border-color: rgb(197 62 53 / 0.36);
      background: var(--danger-soft);
      box-shadow: 8px 8px 0 rgb(197 62 53 / 0.08);
    }

    .error .status-icon::before,
    .error .status-icon::after {
      position: absolute;
      left: 27px;
      top: 14px;
      width: 3px;
      height: 29px;
      content: "";
      border: 0;
      border-radius: 2px;
      background: currentColor;
    }

    .error .status-icon::before { transform: rotate(45deg); }
    .error .status-icon::after { transform: rotate(-45deg); }

    .eyebrow {
      margin: 0 0 0.55rem;
      color: var(--primary-strong);
      font-size: 0.68rem;
      font-weight: 700;
    }

    .error .eyebrow { color: var(--danger); }

    h1 {
      max-width: 13ch;
      margin: 0;
      font-size: clamp(2rem, 7vw, 3rem);
      font-weight: 800;
      line-height: 1.04;
      letter-spacing: -0.05em;
    }

    .description {
      max-width: 42ch;
      margin: 1rem 0 0;
      color: var(--muted);
      font-size: 0.98rem;
    }

    .next {
      display: flex;
      gap: 0.8rem;
      align-items: flex-start;
      margin-top: 2rem;
      padding: 1rem 1.05rem;
      border: 1px solid rgb(201 205 208 / 0.72);
      border-radius: 11px;
      background: rgb(247 245 237 / 0.78);
    }

    .next-dot {
      width: 9px;
      height: 9px;
      flex: 0 0 auto;
      margin-top: 0.45rem;
      border: 2px solid var(--card);
      border-radius: 50%;
      background: var(--primary);
      box-shadow: 0 0 0 3px rgb(17 141 92 / 0.16);
    }

    .error .next-dot {
      background: var(--danger);
      box-shadow: 0 0 0 3px rgb(197 62 53 / 0.14);
    }

    .next p { margin: 0; color: var(--muted); font-size: 0.84rem; }
    .next strong { color: var(--foreground); font-weight: 750; }

    .footer {
      display: flex;
      justify-content: space-between;
      gap: 1rem;
      padding: 0.85rem 1.35rem;
      color: var(--muted);
      border-top: 1px solid rgb(201 205 208 / 0.64);
      font-size: 0.49rem;
      font-weight: 600;
    }

    .privacy { display: flex; align-items: center; gap: 0.4rem; white-space: nowrap; text-align: right; }
    .privacy::before { width: 5px; height: 5px; content: ""; border-radius: 50%; background: var(--primary); }

    @media (max-width: 430px) {
      .shell { width: min(100% - 1rem, 580px); margin: 0.5rem auto; border-radius: 14px; }
      .brand { min-height: 72px; padding: 0.9rem 1rem; }
      .glyph { width: 38px; height: 38px; }
      .content { padding: 1.8rem 1.25rem 2rem; }
      .footer { align-items: flex-start; padding: 0.8rem 1rem; }
    }

    @media (prefers-color-scheme: dark) {
      :root {
        --background: #111513;
        --foreground: #f2f1e9;
        --card: #181d1a;
        --muted: #a5ada7;
        --border: #343b37;
        --primary: #62e99e;
        --primary-strong: #75efaa;
        --grid: rgb(242 241 233 / 0.035);
        --shadow: rgb(0 0 0 / 0.76);
        --danger: #ff8177;
        --danger-soft: #301d1a;
      }

      .shell { border-color: rgb(52 59 55 / 0.95); background: rgb(24 29 26 / 0.96); box-shadow: 0 30px 90px -38px var(--shadow); }
      .next { border-color: rgb(52 59 55 / 0.9); background: rgb(16 19 17 / 0.58); }
      .next-dot { border-color: var(--card); }
      .footer { border-color: rgb(52 59 55 / 0.82); }
    }

    @media (prefers-reduced-motion: no-preference) {
      .shell { animation: arrive 420ms cubic-bezier(0.2, 0.8, 0.2, 1) both; }
      .status-icon { animation: settle 520ms 100ms cubic-bezier(0.2, 0.8, 0.2, 1) both; }
      @keyframes arrive { from { opacity: 0; transform: translateY(10px); } }
      @keyframes settle { from { opacity: 0; transform: scale(0.86) rotate(-3deg); } }
    }
  </style>
</head>
`

const oidcCallbackPageEnd = `
    </section>
    <footer class="footer">
      <span>Local callback</span>
      <span class="privacy">No credentials shown</span>
    </footer>
  </main>
</body>
</html>
`

func writeOIDCCallbackPage(w http.ResponseWriter, success bool) {
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'")
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Referrer-Policy", "no-referrer")
	w.Header().Set("X-Content-Type-Options", "nosniff")

	stateClass := "success"
	eyebrow := "Secure sign-in"
	title := "Sign-in received"
	description := "Graphit received your authentication response and is finishing securely in your terminal."
	next := "Return to the terminal to continue."
	if !success {
		stateClass = "error"
		eyebrow = "Sign-in interrupted"
		title = "Authentication not completed"
		description = "Graphit could not accept this authentication response. No credentials were exposed."
		next = "Return to the terminal and try signing in again."
	}
	productName := strings.TrimSpace(brand.DisplayName)
	if shortName, _, ok := strings.Cut(productName, ":"); ok {
		productName = strings.TrimSpace(shortName)
	}
	if productName == "" {
		productName = brand.Brand
	}
	productName = html.EscapeString(productName)
	pageStart := strings.Replace(oidcCallbackPageStart, "__PRODUCT_NAME__", productName, 1)

	_, _ = io.WriteString(w, pageStart+`<body class="`+stateClass+`">
  <main class="shell" aria-labelledby="page-title">
    <header class="brand">
      <span class="glyph" aria-hidden="true"></span>
      <span class="wordmark">
        <strong>`+productName+`</strong>
        <span>AI engineering system</span>
      </span>
      <span class="edition">OS</span>
    </header>
    <section class="content" aria-live="polite">
      <div class="status-icon" aria-hidden="true"></div>
      <p class="eyebrow">`+eyebrow+`</p>
      <h1 id="page-title">`+title+`</h1>
      <p class="description">`+description+`</p>
      <div class="next">
        <span class="next-dot" aria-hidden="true"></span>
        <p><strong>You can close this window.</strong><br>`+next+`</p>
      </div>`+oidcCallbackPageEnd)
}

---
title: Fix Code Syntax Highlighting in WikiExplorer Dark Theme
status: done
created: 2026-06-02
updated: 2026-06-02
---

# Fix Code Syntax Highlighting in WikiExplorer Dark Theme

## Objective
Fix the code syntax highlighting background and text contrast issues in WikiExplorer when running under the dark theme. The syntax highlighter previously used only the light theme (`oneLight`) styling, which lacked contrast against the dark background.

## Files Changed
| File | Change | Reason |
|---|---|---|
| [WikiExplorerPage.tsx](../internal/ui/src/components/wiki/WikiExplorerPage.tsx) | Modified | Switch to using `oneDark` style when the active UI theme is `'dark'`. |

## Key Decisions
- Imported and used `oneDark` and `useTheme` inside `WikiMarkdown` component to dynamically toggle syntax highlighting theme based on active theme context.

## Notes
- Inline code blocks and plain text preformatted code were already correctly styling with CSS variables/Tailwind classes, so only syntax-highlighted blocks required the Prism theme toggle.

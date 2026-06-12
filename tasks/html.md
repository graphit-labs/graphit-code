# HTML Tree-sitter Integration

## Status: Complete

### Changes
- Added `tree-sitter-html` registration in `treesitter_adapter.go` (smacker/go-tree-sitter HTML binding)
- Created `internal/ast/queries/html.yaml` — comprehensive query config:
  - **Entities:** Elements (start_tag + self_closing_tag), Script elements, Style elements
  - **Relations:** ID, Class, href, src, action, name, for, data-*, aria-*, role attributes
- Updated all documentation (22→23 languages, 17→18 Tree-sitter)

### Extensions
- `.html`, `.htm`

### Node Types Used
- `element` → `start_tag` → `tag_name`
- `self_closing_tag` → `tag_name`
- `script_element` → `start_tag` → `tag_name`
- `style_element` → `start_tag` → `tag_name`
- `attribute` → `attribute_name` + `quoted_attribute_value` → `attribute_value`
- `comment`

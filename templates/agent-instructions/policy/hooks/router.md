{% if module_mandates %}<{{ brand_marker }}>
{% include "policy/hooks/router-preamble.md" %}
{{ module_mandates|join("
") }}
</{{ brand_marker }}>{% endif %}
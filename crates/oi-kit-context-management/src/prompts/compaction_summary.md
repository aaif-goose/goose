{#
  This template is user-overridable: place a modified copy at
  ~/.config/goose/prompts/compaction_summary.md to experiment with what the
  post-compaction context contains (e.g. `user_intent[:3]` to keep only the
  three most important goals) without rebuilding goose.

  key_code is wrapped via the code_fence filter so embedded fences cannot
  break out of the block.
#}
# Conversation Summary

{% if user_intent %}
## User Intent
{% for item in user_intent %}
- {{ item }}
{% endfor %}

{% endif %}
{% if technical_concepts %}
## Technical Concepts
{% for item in technical_concepts %}
- {{ item }}
{% endfor %}

{% endif %}
{% if files %}
## Files + Code
{% for file in files %}
{% if file.path %}
### {{ file.path }}
{% endif %}
{{ file.summary }}
{% if file.key_code %}
{{ file.key_code | code_fence }}
{% endif %}

{% endfor %}
{% endif %}
{% if errors_and_fixes %}
## Errors + Fixes
{% for item in errors_and_fixes %}
- {{ item }}
{% endfor %}

{% endif %}
{% if problem_solving %}
## Problem Solving
{% for item in problem_solving %}
- {{ item }}
{% endfor %}

{% endif %}
{% if user_messages %}
## User Messages
{% for item in user_messages %}
- {{ item }}
{% endfor %}

{% endif %}
{% if pending_tasks %}
## Pending Tasks
{% for item in pending_tasks %}
- {{ item }}
{% endfor %}

{% endif %}
{% if current_work %}
## Current Work
{{ current_work }}

{% endif %}
{% if next_step %}
## Next Step
{{ next_step }}
{% endif %}

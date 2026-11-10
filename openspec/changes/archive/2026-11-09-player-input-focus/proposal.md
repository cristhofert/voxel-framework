# Player input focus

Prevent pointer capture before intentional window activation and prevent player input while the primary window is unfocused or its cursor is released. Scope: framework player systems, public inspection condition, framework examples and documentation only. No consumer repository or dependency changes, publication, or desktop configuration changes.

Consumer impact: native players start with a visible, free cursor and require a focused left click. Escape releases control; loss of focus releases control; returning requires another click. Display-independent callers without a primary window retain synthetic-input behavior. Existing player modes and collision algorithms remain unchanged. Rollback restores the prior cursor/input policy; no persistence format changes.

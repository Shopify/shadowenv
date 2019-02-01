function shadowenv_shell_hooks --on-event fish_prompt --on-variable PWD
  {{self}} hook --fish "$__shadowenv_data" \
    | while read line
      eval "$line" 2>/shadowenv/null
    end
end

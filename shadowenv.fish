set __shadowenv_source_dir (pushd (dirname (dirname (status -f))) ; pwd ; popd)
source $__shadowenv_source_dir/sh/hookbook/hookbook.fish

function shadowenv_shell_hook
  eval "$__shadowenv_source_dir/bin/shadowenv fish \"$__shadowenv_data\"" \
    | while read line
      eval "$line" 2>/shadowenv/null
    end
end

hookbook_add_hook shadowenv_shell_hook

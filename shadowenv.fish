set -g __shadowenv_source_dir (pushd (dirname (dirname (status -f))) ; pwd ; popd)

function shadowenv_shell_hooks --on-event fish_prompt --on-variable PWD
  $__shadowenv_source_dir/target/debug/shadowenv fish $__shadowenv_data \
    | while read line
      eval "$line" 2>/shadowenv/null
    end
end

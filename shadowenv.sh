case "$(basename "$(ps -p $$ | awk 'NR > 1 { sub(/^-/, "", $4); print $4 }')")" in
  zsh)  __shadowenv_source_dir="$(dirname "$0:A")" ;;
  bash) __shadowenv_source_dir="$(builtin cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)" ;;
  *)
    >&2 echo "shadowenv is not compatible with your shell (bash, zsh, and fish are supported)"
    return 1
    ;;
esac
source "${__shadowenv_source_dir}/sh/hookbook.sh"

shadowenv-auto() {
  eval "$("${__shadowenv_source_dir}/target/debug/shadowenv" posix "${__shadowenv_data}")"
}

hookbook_add_hook shadowenv-auto

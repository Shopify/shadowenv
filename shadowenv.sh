case "$(basename "$(\ps -p $$ | \awk 'NR > 1 { sub(/^-/, "", $4); print $4 }')")" in
  zsh)  __shadowenv_source_dir="$(\dirname "$0:A")" ;;
  bash) __shadowenv_source_dir="$(builtin cd "$(\dirname "${BASH_SOURCE[0]}")" && \pwd)" ;;
  *)
    >&2 echo "shadowenv is not compatible with your shell (bash, zsh, and fish are supported)"
    return 1
    ;;
esac
source "${__shadowenv_source_dir}/sh/hookbook/hookbook.sh"

shadowenv-auto() {
  local have_sum want_sum
  # >&2 echo $'\x1b[38;5;241mshadowenv-auto/'$$' '${PWD}$'\x1b[0m'
  have_sum="${__shadowenv_data//:*/}"

  local dir; dir="${PWD}/"
  until [[ -z "${dir}" ]]; do
    dir="${dir%/*}"
    if [[ -f "${dir}/.shadowenv" ]]; then
      want_sum="$(md5sum ${dir}/.shadowenv)"
      want_sum="${want_sum// */}"
      break
    fi
  done

  # >&2 echo 'H:'$have_sum';W:'${want_sum}$'\x1b[0m'

  if [[ "${have_sum}" != "${want_sum}" ]]; then
    eval "$(
      ruby --disable-gems \
        "${__shadowenv_source_dir}/bin/shadowenv" posix "${__shadowenv_data}"
    )"
  fi
}

hookbook_add_hook shadowenv-auto

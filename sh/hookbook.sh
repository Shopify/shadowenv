# Hookbook (https://github.com/Shopify/hookbook)
#
# Copyright 2019 Shopify Inc.
#
# Permission is hereby granted, free of charge, to any person obtaining a copy of
# this software and associated documentation files (the "Software"), to deal in
# the Software without restriction, including without limitation the rights to
# use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
# the Software, and to permit persons to whom the Software is furnished to do so,
# subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
# FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
# COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
# IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
# CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

__hookbook_shell="$(\ps -p $$ | \awk 'NR > 1 { sub(/^-/, "", $4); print $4 }')"
__hookbook_shellname="$(basename "${__hookbook_shell}")"

__hookbook_array_contains() {
  local seeking=$1; shift
  local in=1
  for element; do
    if [[ $element == $seeking ]]; then
      in=0
      break
    fi
  done
  return $in
}

case "${__hookbook_shellname}" in
  zsh)
    hookbook_add_hook() {
      local fn=$1

      eval "
        __hookbook_${fn}_preexec() { ${fn} zsh-preexec }
        __hookbook_${fn}_chpwd()   { ${fn} zsh-chpwd }
        __hookbook_${fn}_precmd()  { ${fn} zsh-precmd }
      "

      __hookbook_array_contains "__hookbook_${fn}_preexec" "${preexec_functions[@]}" \
        || preexec_functions+=("__hookbook_${fn}_preexec")

      __hookbook_array_contains "__hookbook_${fn}_chpwd" "${chpwd_functions[@]}" \
        || chpwd_functions+=("__hookbook_${fn}_chpwd")

      __hookbook_array_contains "__hookbook_${fn}_precmd" "${precmd_functions[@]}" \
        || precmd_functions+=("__hookbook_${fn}_precmd")
    }

    ;;
  bash)
    if [[ ! -v __hookbook_functions ]]; then
      __hookbook_functions=()
    fi

    __hookbook_debug_handler() {
      # shellcheck disable=SC2068
      for fn in ${__hookbook_functions[@]}; do
        ${fn} bash-debug
      done
    }

    trap \
      '{ __hookbook_underscore=$_; if [[ $- =~ x ]]; then set +x; __hookbook_debug_handler 2>&3; set -x; else __hookbook_debug_handler 2>&3; : "$__hookbook_underscore"; fi; } 4>&2 2>/dev/null 3>&4' \
      DEBUG

    hookbook_add_hook() {
      local fn=$1

      if [[ ! "${PROMPT_COMMAND}" == *" $fn "* ]]; then
        # This is essentially:
        #   PROMPT_COMMAND="${fn}; ${PROMPT_COMMAND}"
        # ...except with weird magic to toggle off `-x` if it's set.
        PROMPT_COMMAND="{ if [[ \$- =~ x ]]; then set +x; ${fn} bash-prompt 2>&3; set -x; else ${fn} bash-prompt 2>&3; fi; } 4>&2 2>/dev/null 3>&4; ${PROMPT_COMMAND}"
      fi

      __hookbook_array_contains "${fn}" "${__hookbook_functions[@]}" \
        || __hookbook_functions+=("${fn}")
    }
    ;;
  *)
    >&2 \echo "hookbook is not compatible with your shell (${__hookbook_shell})"
    \return 1
    ;;
esac

unset __hookbook_shell
unset __hookbook_shellname

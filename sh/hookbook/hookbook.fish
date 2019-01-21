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

set -q __hookbook_functions; or set __hookbook_functions ()

function hookbook_add_hook --argument fn
  contains -i $fn $__hookbook_functions
  or set __hookbook_functions $__hookbook_functions $fn
end

function __hookbook_prompt_hooks --on-event fish_prompt
  for fn in $__hookbook_functions
    eval "$fn fish-prompt"
  end
end

function __hookbook_chpwd_hooks --on-variable PWD
  for fn in $__hookbook_functions
    eval "$fn fish-chpwd"
  end
end

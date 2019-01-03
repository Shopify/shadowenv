require('shadowenv/lang')

module Shadowenv
  Error = Class.new(StandardError)

  autoload(:Env,             'shadowenv/env')
  autoload(:ExportFormatter, 'shadowenv/export_formatter')
  autoload(:Runner,          'shadowenv/runner')
  autoload(:Serialize,       'shadowenv/serialize')
  autoload(:VERSION,         'shadowenv/version')

  module Lang
    autoload(:Eval,  'shadowenv/lang/eval')
    autoload(:Frame, 'shadowenv/lang/frame')
    autoload(:Lib,   'shadowenv/lang/lib')
    autoload(:Loop,  'shadowenv/lang/loop')
    autoload(:Read,  'shadowenv/lang/read')
  end
end

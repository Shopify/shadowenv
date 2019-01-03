require('shadowenv')

module Shadowenv
  module Runner
    def self.call(format:, shadowenv_data:, program_source: resolve(ENV, Dir.pwd), env: ENV)
      shadowenv = Shadowenv::Env.new(ENV, shadowenv_data)
      lib       = Shadowenv::Lang::Lib.build(shadowenv)
      frame     = Shadowenv::Lang::Frame.new(lib, nil)
      reader    = Shadowenv::Lang::Read.new(program_source)
      evaluator = Shadowenv::Lang::Eval.new(frame)
      Shadowenv::Lang::Loop.call(reader, evaluator)
      ret = Shadowenv::ExportFormatter.call(
        shadowenv.changes,
        shadowenv.shadowenv_data(program_source),
        format: format,
      )
      action = program_source.nil? || program_source.empty? ? 'deactivated' : 'activated'
      STDERR.puts(
        "\x1b[1;34m#{action} \x1b[38;5;249ms\x1b[38;5;248mh\x1b[38;5;247ma" +
        "\x1b[38;5;246md\x1b[38;5;245mo\x1b[38;5;244mw\x1b[38;5;243me" +
        "\x1b[38;5;242mn\x1b[38;5;241mv\x1b[38;5;240m.\x1b[0m"
      )
      ret
    end

    def self.resolve(env, dir, path_suffix: '.shadowenv')
      loop do
        return '' if dir == '/'
        file = File.join(dir, path_suffix)
        return File.read(file) if File.exist?(file)
        dir = File.dirname(dir)
      end
    end
  end
end

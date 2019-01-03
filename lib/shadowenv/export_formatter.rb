require('shadowenv')

module Shadowenv
  module ExportFormatter
    def self.call(changes, shadowenv_data, format:)
      case format
      when 'posix'
        posix(changes, shadowenv_data)
      when 'fish'
        fish(changes, shadowenv_data)
      else
        raise("unacceptable format: #{format}")
      end
    end

    def self.posix(changes, shadowenv_data)
      ret = ""

      changes.each do |name, value|
        if value.nil?
          ret << "unset #{name}\n"
        else
          ret << "export #{name}=#{value.inspect}\n"
        end
      end

      if shadowenv_data.nil?
        ret << "unset __shadowenv_data\n"
      else
        ret << "__shadowenv_data=#{shadowenv_data.inspect}\n"
      end

      ret
    end

    def self.fish(changes, shadowenv_data)
      ret = ""

      changes.each do |name, value|
        if value.nil?
          ret << "set -e #{name}\n"
        else
          ret << "set -x #{name} #{value.inspect}\n"
        end
      end

      if shadowenv_data.nil?
        ret << "set -e __shadowenv_data\n"
      else
        ret << "set __shadowenv_data #{shadowenv_data.inspect}\n"
      end

      ret
    end
  end
end

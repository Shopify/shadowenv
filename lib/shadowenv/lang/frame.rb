require('shadowenv')

module Shadowenv
  module Lang
    class Frame
      attr_reader(:env, :parent)

      def initialize(env, parent)
        @env = env
        @parent = parent
      end

      def [](k)
        fetch(k)
      rescue KeyError
        nil
      end

      def fetch(k)
        frame = self
        until frame.nil?
          return frame.env[k] if frame.env.key?(k)
          frame = frame.parent
        end
        raise(KeyError, "no name #{k}")
      end

      def []=(k, v)
        env[k] = v
      end
    end
  end
end

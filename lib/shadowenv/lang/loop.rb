require('shadowenv')

module Shadowenv
  module Lang
    module Loop
      def self.call(reader, evaluator)
        while (form = reader.read) != Shadowenv::Lang::Read::NO_MORE_TOKENS
          evaluator.eval(form)
        end
      end
    end
  end
end

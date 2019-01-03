require('shadowenv')

module Shadowenv
  module Lang
    class Read #:nodoc:
      NO_MORE_TOKENS = :no_more_tokens

      def initialize(expr)
        @tokens = expr.scan(Regexp.union(
          /"(?:\\.|[^"\\])*"/,   # dquoted string
          /[()]/,                # '(' or ')'
          /\w[\w\-\/\*]*/,       # identifier/symbol
          %r{/(?:[^/\\]|\\.)*/}, # regexp
          /'.*?'/,               # quoted string
        ))
      end

      # TODO enumerator?
      def read
        case token = @tokens.shift
        when nil       then NO_MORE_TOKENS
        when 'nil'     then nil
        when '('       then read_list
        when /^['"].*/ then token[1..-2] # string
        when %r{^/.*}  then eval(token)  # regexp
        when /\d+/     then token.to_i   # int
        else           token.to_sym
        end
      end

      def read_list
        list = []
        until @tokens.first == ')'
          token = read
          raise "unterminated list" if token == NO_MORE_TOKENS
          list << token
        end
        @tokens.shift
        list
      end
    end
  end
end

require('shadowenv')

module Shadowenv
  module Lang
    class Eval #:nodoc:
      def initialize(top_frame)
        @top_frame = top_frame
      end

      def self.apply(fn, args, ctx)
        return ctx[fn].call(args, ctx) if ctx[fn].respond_to?(:call)

        if ctx[fn].nil?
          raise(NameError, "not a function: #{fn}")
        end

        Eval.eval(ctx[fn][2], ctx.merge(Hash[*ctx[fn][1].zip(args).flatten(1)]))
      end

      def self.eval(sexpr, ctx)
        case sexpr
        when Symbol
          ctx.fetch(sexpr)
        when Array
          fn, *args = sexpr
          if ctx[fn].is_a?(Array) || (ctx[fn].respond_to?(:lambda?) && ctx[fn].lambda?)
            args = args.map { |a| eval(a, ctx) }
          end
          apply(fn, args, ctx)
        else
          sexpr
        end
      end

      def eval(sexpr, ctx = @top_frame)
        Eval.eval(sexpr, ctx)
      end
    end
  end
end

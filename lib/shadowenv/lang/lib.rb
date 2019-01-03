require('shadowenv')

module Shadowenv
  module Lang
    module Lib
      def self.build(shadowenv)
        eval = ->(sexpr, ctx) { Shadowenv::Lang::Eval.eval(sexpr, ctx) }
        {
          # basic scheme  stuff
          define: proc { |(name, val), ctx| ctx[name] = eval.call(val, ctx) },
          lambda: proc do |(params, *sexprs), _|
            ->(args, ctx) do
              ctx = Frame.new(params.zip(args).to_h, ctx)
              sexprs.reduce(nil) { |_, s| eval.call(s, ctx) }
            end
          end,
          comment: proc { |(*), _| nil },
          if: proc { |(cond, t, f), ctx| eval.call(cond, ctx) ? eval.call(t, ctx) : eval.call(f, ctx) },
          when: proc { |(cond, *sexprs), ctx| sexprs.reduce(nil) { |_, s| eval.call(s, ctx) } if eval.call(cond, ctx) },
          quote: proc { |sexpr, _| sexpr[0] },
          progn: proc { |(*sexprs), ctx| sexprs.reduce(nil) { |_, s| eval.call(s, ctx) } },
          let: proc do |(assigns, *sexprs), ctx|
            descend = Frame.new({}, ctx)
            assigns.each { |name, expr| descend[name] = eval.call(expr, ctx) }
            sexprs.reduce(nil) { |_, s| eval.call(s, descend) }
          end,
          'let*': proc do |(assigns, *sexprs), ctx|
            ctx = assigns.reduce(ctx) do |subctx, (name, expr)|
              Frame.new({ name => eval.call(expr, subctx) }, subctx)
            end
            sexprs.reduce(nil) { |_, s| eval.call(s, ctx) }
          end,
          'when-let': proc do |(assigns, *sexprs), ctx|
            raise 'too many assigns' if assigns.size > 1
            name, expr = *assigns[0]
            if val = eval.call(expr, ctx)
              f = Frame.new({ name => val }, ctx)
              sexprs.reduce(nil) { |_, s| eval.call(s, f) }
            end
          end,
          car: ->((list), _) { list[0] },
          cdr: ->((list), _) { list.drop(1) },
          cons: ->((e, cell), _) { [e] + cell },
          eq: ->((l, r), ctx) { eval.call(l, ctx) == eval.call(r, ctx) },
          not: ->((e), _) { !e },
          atom: ->((sexpr), _) { !sexpr.is_a?(Array) },
          concat: ->((*strs), _) { strs.each_with_object("") { |s, o| o << s } },

          # utility
          'path-concat': ->((*strs), _) { File.join(*strs) },
          'expand-path': ->((str), _) { File.expand_path(str) },

          'provide': ->((feature, version), _) { shadowenv.provide(feature, version) },

          # environment manipulation
          'env/get': ->((var), _) { shadowenv.get(var) },
          'env/set': ->((var, val), _) { shadowenv.set(var, val) },
          'env/prepend-to-pathlist': ->((var, val), _) { shadowenv.prepend_to_pathlist(var, val) },
          'env/remove-from-pathlist': ->((var, val), _) { shadowenv.remove_from_pathlist(var, val) },
        }
      end
    end
  end
end

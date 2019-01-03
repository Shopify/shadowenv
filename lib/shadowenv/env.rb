require('shadowenv')
require('set')
require('json')

module Shadowenv
  class Env
    TYPE_LIST   = :list
    TYPE_SCALAR = :scalar

    ORIGINAL = 'o'
    CURRENT  = 'c'
    MISSING  = 'm'
    NEW      = 'n'

    def initialize(current_env, shadowenv_data)
      @current_env = Hash.new.merge(current_env)
      @env = Hash.new.merge(current_env)
      @types = {}

      data = Shadowenv::Serialize.load(shadowenv_data)
      undo_shadowenv_data(data, @env)
      @unshadowed_env = Hash.new.merge(@env)
    end

    def undo_shadowenv_data(data, env)
      data.each do |k, v|
        if v.key?(ORIGINAL) # scalar
          if env[k] == v[CURRENT]
            set(k, v[ORIGINAL])
          end
        else # list
          v[NEW].each do |el|
            remove_from_pathlist(k, el)
          end
          v[MISSING].each do |el|
            prepend_to_pathlist(k, el) # TODO(burke): try to preserve ordering.
          end
        end
      end
    end

    #####
    # API used in Shadowenv::Lang::Lib

    def get(var)
      @env[var]
    end

    def set(var, val)
      @types[var] ||= TYPE_SCALAR

      @env[var] = val
    end

    def prepend_to_pathlist(var, val)
      @types[var] = TYPE_LIST

      @env[var] = @env           ### e.g. with val == 'c':
        .fetch(var, '')          # 'a:b:c'
        .split(':')              # ['a', 'b', 'c']
        .reject { |x| x == val } # ['a', 'b']
        .unshift(val)            # ['c, 'a', 'b']
        .join(':')               # 'c:a:b'
    end

    def remove_from_pathlist(var, val)
      @types[var] = TYPE_LIST

      @env[var] = @env           ### e.g. with val == 'c':
        .fetch(var, '')          # 'a:b:c'
        .split(/:/)              # ['a', 'b', 'c']
        .reject { |x| x == val } # ['a', 'b']
        .join(':')               # 'a:b'
    end

    def provide(feature, version)
      # TODO(burke): implement feature tracking.
    end

    # End of API used in Shadowenv::Lang::Lib
    #####

    def shadowenv_data(contents)
      keys = Set.new([*@env.keys, *@current_env.keys])

      changes = keys.each_with_object({}) do |key, acc|
        if @env[key] != @unshadowed_env[key]
          acc[key] = @env[key]
        end
      end
      data = {}

      changes.each do |change, curr|
        case @types[change]
        when TYPE_SCALAR
          data[change] = { 'o' => @unshadowed_env[change], 'c' => curr }
        when TYPE_LIST
          ap = @unshadowed_env.fetch(change, '').split(':')
          bp = curr.split(':')
          data[change] = { 'n' => bp - ap, 'm' => ap - bp }
        else
          raise('unreachable')
        end
      end

      Shadowenv::Serialize.dump(data, contents)
    end

    def changes
      keys = Set.new([*@env.keys, *@current_env.keys])

      keys.each_with_object({}) do |key, acc|
        if @env[key] != @current_env[key]
          acc[key] = @env[key]
        end
      end
    end

  end
end

require('shadowenv')
require('digest/md5')

module Shadowenv
  # Serialize is just JSON.parse/JSON.dump, except '{}' is replaced by `nil`.
  module Serialize
    def self.load(data)
      if data.nil? || data.empty?
        data = '0:{}'
      end
      JSON.parse(data.sub(/^[a-z0-9]+:/, ''))
    end

    def self.dump(obj, contents)
      data = JSON.dump(obj)
      if data == '{}' && (contents || '').empty?
        nil
      else
        Digest::MD5.hexdigest(contents) + ":" + data
      end
    end
  end
end

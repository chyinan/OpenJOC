-- SPDX-FileCopyrightText: 2026 OpenJOC contributors
-- SPDX-License-Identifier: Apache-2.0

-- Exercise the real settings-script file IO with LuaJIT and a Unicode path.
-- The mpv API is mocked, but io.open/os.rename/os.remove are not replaced.
-- Windows packaging CI passes an ASCII-only root because MinGW LuaJIT's
-- os.getenv does not reliably preserve non-ACP characters from Windows env
-- values. Construct the Unicode child path here so real LuaJIT CRT file
-- operations still receive UTF-8 bytes.
local test_root = assert(os.getenv('OPENJOC_SETTINGS_IO_TEST_ROOT'),
    'OPENJOC_SETTINGS_IO_TEST_ROOT must name a pre-created test root')
local config_dir = test_root .. '/OpenJOC Settings 日本語'
assert(config_dir:find(' ', 1, true), 'test config path must contain spaces')
assert(config_dir:find('日本語', 1, true), 'test config path must contain Japanese characters')
config_dir = config_dir:gsub('\\', '/')

local function path(name)
    return config_dir .. '/' .. name
end

local primary = path('openjoc-settings.json')
local temporary = primary .. '.tmp'
local backup = primary .. '.bak'
for _, filename in ipairs({ primary, temporary, backup }) do
    os.remove(filename)
end

local initial = '{"schema":1,"options":{"render_mode":"speaker","speaker_layout":"5.1"}}\n'
local seed = assert(io.open(primary, 'wb'), 'LuaJIT could not create a Unicode-path settings file')
assert(seed:write(initial))
assert(seed:close())

local bindings, hooks, observers = {}, {}, {}
local overlay = { data = '', res_x = 0, res_y = 0 }
function overlay:update() end
local last_parsed_layout

local function parse_json(contents)
    local layout = contents:match('"speaker_layout"%s*:%s*"([^"]+)"')
    if not layout or not contents:find('"schema"%s*:%s*1') then return nil end
    local mode = contents:match('"render_mode"%s*:%s*"([^"]+)"')
    last_parsed_layout = layout
    return { schema = 1, options = { render_mode = mode, speaker_layout = layout } }
end

local function quote(value)
    return '"' .. tostring(value):gsub('\\', '\\\\'):gsub('"', '\\"') .. '"'
end

local utils = {
    parse_json = parse_json,
    format_json = function(document)
        local parts = {}
        for key, value in pairs(document.options) do
            parts[#parts + 1] = quote(key) .. ':' .. quote(value)
        end
        table.sort(parts)
        return '{"schema":1,"options":{' .. table.concat(parts, ',') .. '}}'
    end,
    file_info = function() return nil, 'not found' end,
}
local input = {
    get = function() error('SOFA input is not part of the settings IO smoke') end,
}
package.preload['mp.utils'] = function() return utils end
package.preload['mp.input'] = function() return input end

mp = {
    command_native = function(command)
        local name, value = command[1], command[2]
        if name == 'escape-ass' then
            return value
        elseif name == 'expand-path' and value:sub(1, 3) == '~~/' then
            return config_dir .. '/' .. value:sub(4)
        elseif name == 'expand-path' or name == 'normalize-path' then
            return value
        end
        error('unexpected mp.command_native command: ' .. tostring(name))
    end,
    get_property = function(name, default)
        if name == 'current-tracks/audio/decoder' then return 'eac3' end
        if name == 'audio-params/hr-channels' then return '5.1' end
        if name == 'audio-out-params/hr-channels' then return 'Stereo' end
        return default
    end,
    get_property_native = function(name, default)
        if name == 'track-list' then return {} end
        return default
    end,
    get_property_number = function(_, default) return default end,
    get_osd_size = function() return 1280, 720 end,
    create_osd_overlay = function(kind)
        assert(kind == 'ass-events')
        return overlay
    end,
    add_key_binding = function(_, name, callback) bindings[name] = callback end,
    add_forced_key_binding = function(_, name, callback) bindings[name] = callback end,
    remove_key_binding = function(name) bindings[name] = nil end,
    add_hook = function(name, _, callback) hooks[name] = callback end,
    observe_property = function(name, _, callback) observers[name] = callback end,
    add_timeout = function()
        return { kill = function() end }
    end,
    osd_message = function() end,
    msg = { info = function() end, error = function(message) error(message) end },
}

local real_rename = os.rename
local backup_seen = false
local force_existing_destination_fallback = true
os.rename = function(old_path, new_path)
    -- Force the same fallback used by Windows CRTs when rename-over-existing
    -- is unsupported, then let the real LuaJIT CRT perform both Unicode moves.
    if force_existing_destination_fallback and old_path == temporary and new_path == primary then
        force_existing_destination_fallback = false
        return nil, 'simulated Windows rename-over-existing failure'
    end
    local ok, err = real_rename(old_path, new_path)
    if ok and new_path == backup then
        backup_seen = true
        local previous = assert(io.open(backup, 'rb'), 'backup was not readable after Unicode rename')
        local contents = previous:read('*a')
        assert(previous:close())
        assert(contents == initial, 'backup did not preserve the old settings bytes')
    end
    return ok, err
end

local script = 'integrations/mpv/openjoc-settings.lua'
dofile(script)
assert(type(bindings['openjoc-settings-toggle']) == 'function')
assert(type(hooks.on_preloaded) == 'function')
bindings['openjoc-settings-toggle']()
assert(overlay.data:find('Output policy', 1, true))
assert(overlay.data:find('5.1', 1, true), 'seeded settings were not loaded from the Unicode path')
bindings['openjoc-settings-right']()
assert(overlay.data:find('7.1', 1, true), 'draft change was not reflected in the panel')
for _ = 1, 5 do bindings['openjoc-settings-down']() end -- output row -> Save
bindings['openjoc-settings-enter']()

local written = assert(io.open(primary, 'rb'), 'primary settings file was not promoted')
local written_contents = written:read('*a')
assert(written:close())
assert(parse_json(written_contents).options.speaker_layout == '7.1',
    'primary file does not contain the saved output choice')
assert(backup_seen, 'backup fallback did not use a Unicode-path rename/read')
assert(not io.open(temporary, 'rb'), 'successful save left a temporary settings file')
assert(not io.open(backup, 'rb'), 'successful save left a backup settings file behind')
os.rename = real_rename

-- Reinitialize the actual menu script and verify it reads the promoted file.
for key in pairs(bindings) do bindings[key] = nil end
for key in pairs(hooks) do hooks[key] = nil end
for key in pairs(observers) do observers[key] = nil end
overlay.data = ''
dofile(script)
bindings['openjoc-settings-toggle']()
assert(overlay.data:find('Output policy', 1, true)
        and overlay.data:find('7.1', 1, true)
        and last_parsed_layout == '7.1',
    'reloaded menu did not retain the setting from the Unicode-path primary file')

for _, filename in ipairs({ primary, temporary, backup }) do
    os.remove(filename)
end
print('mpv OpenJOC LuaJIT settings IO Unicode-path roundtrip passed')

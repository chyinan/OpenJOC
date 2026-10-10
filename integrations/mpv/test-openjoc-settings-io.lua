-- SPDX-FileCopyrightText: 2026 OpenJOC contributors
-- SPDX-License-Identifier: Apache-2.0

-- Exercise the real settings-script file IO against a Unicode path. The mpv
-- API is mocked, but the same path-aware adapter used by the production script
-- performs every filesystem operation. On Windows that adapter uses LuaJIT FFI
-- and Win32 wide-path calls; on other platforms it uses native io/os functions.
-- Windows CI passes an ASCII-only root because MinGW's environment API does not
-- reliably preserve non-ACP characters. Construct the Unicode child path here
-- so the real Windows API receives UTF-8 bytes from the literal below.
local test_root = assert(os.getenv('OPENJOC_SETTINGS_IO_TEST_ROOT'),
    'OPENJOC_SETTINGS_IO_TEST_ROOT must name a pre-created test root')
local config_dir = test_root .. '/OpenJOC Settings 日本語'
assert(config_dir:find(' ', 1, true), 'test config path must contain spaces')
assert(config_dir:find('日本語', 1, true), 'test config path must contain Japanese characters')
config_dir = config_dir:gsub('\\', '/')

local function path(name)
    return config_dir .. '/' .. name
end

local script = 'integrations/mpv/openjoc-settings.lua'
local bindings, hooks, observers = {}, {}, {}
local logged_errors = {}
local overlay = { data = '', res_x = 0, res_y = 0 }
function overlay:update() end
local last_parsed_layout
local last_parsed_gain

local function parse_json(contents)
    local layout = contents:match('"speaker_layout"%s*:%s*"([^"]+)"')
    if not layout or not contents:find('"schema"%s*:%s*1') then return nil end
    local mode = contents:match('"render_mode"%s*:%s*"([^"]+)"')
    local gain = contents:match('"output_gain_tenths_db"%s*:%s*(-?%d+)')
    last_parsed_layout = layout
    last_parsed_gain = gain and tonumber(gain) or nil
    return { schema = 1, options = {
        render_mode = mode, speaker_layout = layout,
        output_gain_tenths_db = last_parsed_gain,
    } }
end

local function quote(value)
    return '"' .. tostring(value):gsub('\\', '\\\\'):gsub('"', '\\"') .. '"'
end

local utils = {
    parse_json = parse_json,
    format_json = function(document)
        local parts = {}
        for key, value in pairs(document.options) do
            local encoded = type(value) == 'number' and tostring(value) or quote(value)
            parts[#parts + 1] = quote(key) .. ':' .. encoded
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

local function reset_mpv_mocks()
    for key in pairs(bindings) do bindings[key] = nil end
    for key in pairs(hooks) do hooks[key] = nil end
    for key in pairs(observers) do observers[key] = nil end
    logged_errors = {}
    overlay.data = ''
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
        msg = {
            info = function() end,
            error = function(message) logged_errors[#logged_errors + 1] = message end,
        },
    }
end

-- Load once to initialize the production path-aware adapter, then use that
-- exact adapter to seed the Unicode-path fixture before reloading the script.
reset_mpv_mocks()
dofile(script)
local file_io = assert(package.loaded['openjoc.settings_file_io.v1'])
local primary = path('openjoc-settings.json')
local temporary = primary .. '.tmp'
local backup = primary .. '.bak'
for _, filename in ipairs({ primary, temporary, backup }) do
    file_io.remove(filename)
end

local initial = '{"schema":1,"options":{"render_mode":"speaker","speaker_layout":"5.1","output_gain_tenths_db":-137}}\n'
assert(file_io.write(primary, initial), 'LuaJIT could not seed a Unicode-path settings file')

if package.config:sub(1, 1) == '\\' then
    assert(file_io.write(temporary, 'replacement'), 'could not create rename probe')
    local replaced = file_io.move(temporary, primary)
    assert(not replaced, 'Windows MoveFileW unexpectedly replaced an existing destination')
    assert(file_io.read(primary) == initial, 'failed no-replace move changed the old file')
    assert(file_io.read(temporary) == 'replacement', 'failed no-replace move lost the temp file')
    assert(file_io.remove(temporary), 'could not remove rename probe')
end

local backup_seen = false
local force_existing_destination_fallback = true
local real_move = file_io.move
file_io.move = function(old_path, new_path)
    -- Force the same fallback used by Windows CRTs when rename-over-existing
    -- is unsupported, then let the production adapter perform both Unicode moves.
    if force_existing_destination_fallback and old_path == temporary and new_path == primary then
        force_existing_destination_fallback = false
        return nil, 'simulated Windows rename-over-existing failure'
    end
    local ok, err = real_move(old_path, new_path)
    if ok and new_path == backup then
        backup_seen = true
        local contents = assert(file_io.read(backup), 'backup was not readable after Unicode rename')
        assert(contents == initial, 'backup did not preserve the old settings bytes')
    end
    return ok, err
end

dofile(script)
assert(type(bindings['openjoc-settings-toggle']) == 'function')
assert(type(hooks.on_preloaded) == 'function')
bindings['openjoc-settings-toggle']()
assert(overlay.data:find('Output policy', 1, true))
assert(overlay.data:find('5.1', 1, true), 'seeded settings were not loaded from the Unicode path')
assert(overlay.data:find('−13.7 dB', 1, true), 'saved numeric gain was not loaded from the Unicode path')
bindings['openjoc-settings-right']()
assert(overlay.data:find('7.1', 1, true), 'draft change was not reflected in the panel')
for _ = 1, 6 do bindings['openjoc-settings-down']() end -- output row -> Save
bindings['openjoc-settings-enter']()

local written_contents = assert(file_io.read(primary), 'primary settings file was not promoted')
assert(parse_json(written_contents).options.speaker_layout == '7.1',
    'primary file does not contain the saved output choice')
assert(last_parsed_gain == -137,
    'explicit save did not preserve the backward-compatible numeric gain value')
assert(written_contents:find('"output_gain_tenths_db":-137', 1, true),
    'numeric live gain was serialized as a string or changed value')
assert(backup_seen, 'backup fallback did not use a Unicode-path rename/read')
assert(not file_io.read(temporary), 'successful save left a temporary settings file')
assert(not file_io.read(backup), 'successful save left a backup settings file behind')
file_io.move = real_move

-- Reinitialize the actual menu script and verify it reads the promoted file.
reset_mpv_mocks()
dofile(script)
bindings['openjoc-settings-toggle']()
assert(overlay.data:find('Output policy', 1, true)
        and overlay.data:find('7.1', 1, true)
        and last_parsed_layout == '7.1',
    'reloaded menu did not retain the setting from the Unicode-path primary file')

-- Emulate a Windows host whose Lua module lacks FFI. A Unicode path must be
-- reported as unreadable and Save must stay blocked; never turn that read
-- failure into empty defaults or replace the existing settings file.
local saved_bytes = assert(file_io.read(primary))
local original_package_config = package.config
local original_ffi_loaded = package.loaded.ffi
local original_ffi_preload = package.preload.ffi
package.config = '\\' .. '\n;\n?\n!\n-\n'
package.loaded.ffi = nil
package.preload.ffi = function() error('injected FFI unavailable') end
package.loaded['openjoc.settings_file_io.v1'] = nil
reset_mpv_mocks()
dofile(script)
assert(#logged_errors == 1 and logged_errors[1]:find('Unicode settings paths need LuaJIT FFI support', 1, true),
    'missing FFI on a Unicode path was not logged as a settings read failure')
bindings['openjoc-settings-toggle']()
assert(overlay.data:find('Settings need attention', 1, true)
        and overlay.data:find('Save disabled', 1, true),
    'unreadable Unicode settings were shown as ordinary defaults')
bindings['openjoc-settings-right']()
for _ = 1, 6 do bindings['openjoc-settings-down']() end -- output row -> disabled Save
bindings['openjoc-settings-enter']()
assert(overlay.data:find('Save disabled to protect the existing settings file', 1, true),
    'Save was not blocked after a Unicode-path read failure')
assert(file_io.read(primary) == saved_bytes, 'blocked Save changed the unreadable settings file')
assert(not file_io.read(temporary), 'blocked Save created a temporary settings file')
assert(not file_io.read(backup), 'blocked Save changed the recovery backup')
package.config = original_package_config
package.loaded.ffi = original_ffi_loaded
package.preload.ffi = original_ffi_preload
package.loaded['openjoc.settings_file_io.v1'] = file_io

-- If a valid recovery backup can be read but cannot be restored, Save must
-- not delete it while trying to replace the primary file.
local backup_bytes = '{"schema":1,"options":{"speaker_layout":"5.1"}}\n'
local recovery_backup = { [backup] = backup_bytes }
local recovery_writes, recovery_moves, recovery_removes = 0, 0, 0
local recovery_io = {
    read = function(filename)
        if filename == primary then return nil, 'not found', true end
        if filename == backup then return recovery_backup[filename], 'not found', false end
        return nil, 'not found', true
    end,
    write = function() recovery_writes = recovery_writes + 1; return true end,
    move = function(old_path, new_path)
        recovery_moves = recovery_moves + 1
        if old_path == backup and new_path == primary then
            return nil, 'injected backup restore failure'
        end
        recovery_backup[new_path] = recovery_backup[old_path]
        recovery_backup[old_path] = nil
        return true
    end,
    remove = function(filename)
        recovery_removes = recovery_removes + 1
        recovery_backup[filename] = nil
        return true
    end,
}
package.config = '\\' .. '\n;\n?\n!\n-\n'
package.loaded['openjoc.settings_file_io.v1'] = recovery_io
reset_mpv_mocks()
dofile(script)
assert(#logged_errors == 1 and logged_errors[1]:find('could not be restored', 1, true),
    'failed backup restoration was not logged')
bindings['openjoc-settings-toggle']()
assert(overlay.data:find('Settings need attention', 1, true)
        and overlay.data:find('Save disabled', 1, true),
    'failed backup restoration did not disable Save')
bindings['openjoc-settings-right']()
for _ = 1, 5 do bindings['openjoc-settings-down']() end
bindings['openjoc-settings-enter']()
assert(recovery_writes == 0 and recovery_removes == 0,
    'Save deleted or replaced the only recovery backup')
assert(recovery_backup[backup] == backup_bytes,
    'the only recovery backup was lost after a blocked Save')
package.config = original_package_config
package.loaded['openjoc.settings_file_io.v1'] = file_io

-- If the new path and recovery backup are truly absent but the legacy path
-- cannot be read, do not let Save shadow the only possible prior settings.
local legacy_path = config_dir .. '/../../config/openjoc-settings.json'
local legacy_bytes = '{"schema":1,"options":{"speaker_layout":"5.1"}}\n'
local legacy_store = { [legacy_path] = legacy_bytes }
local fake_writes, fake_moves, fake_removes = 0, 0, 0
local missing_legacy_io = {
    read = function(filename)
        if filename == primary or filename == backup then
            return nil, 'not found', true
        end
        if filename == legacy_path then
            return nil, 'injected Unicode legacy read error', false
        end
        error('unexpected test path: ' .. tostring(filename))
    end,
    write = function(filename, contents)
        fake_writes = fake_writes + 1
        if filename == legacy_path then legacy_store[filename] = contents end
        return true
    end,
    move = function(old_path, new_path)
        fake_moves = fake_moves + 1
        if old_path == legacy_path then
            legacy_store[new_path] = legacy_store[old_path]
            legacy_store[old_path] = nil
        end
        return true
    end,
    remove = function(filename)
        fake_removes = fake_removes + 1
        if filename == legacy_path then legacy_store[filename] = nil end
        return true
    end,
}
package.config = '\\' .. '\n;\n?\n!\n-\n'
package.loaded['openjoc.settings_file_io.v1'] = missing_legacy_io
reset_mpv_mocks()
dofile(script)
assert(#logged_errors == 1 and logged_errors[1]:find('Older saved settings could not be read', 1, true),
    'unreadable legacy settings were not surfaced')
bindings['openjoc-settings-toggle']()
assert(overlay.data:find('Settings need attention', 1, true)
        and overlay.data:find('Save disabled', 1, true),
    'unreadable legacy settings did not disable Save')
bindings['openjoc-settings-right']()
for _ = 1, 5 do bindings['openjoc-settings-down']() end
bindings['openjoc-settings-enter']()
assert(fake_writes == 0 and fake_moves == 0 and fake_removes == 0,
    'Save mutated paths after the legacy state could not be read')
assert(legacy_store[legacy_path] == legacy_bytes,
    'the legacy fixture changed unexpectedly')
package.config = original_package_config
package.loaded['openjoc.settings_file_io.v1'] = file_io

for _, filename in ipairs({ primary, temporary, backup }) do
    file_io.remove(filename)
end
print('mpv OpenJOC LuaJIT settings IO Unicode-path roundtrip passed')

-- SPDX-FileCopyrightText: 2026 OpenJOC contributors
-- SPDX-License-Identifier: Apache-2.0

-- Mocked mpv API smoke test for the bundled settings script. Run with LuaJIT.
local files = {}
local existing_paths = { ['/tmp/openjoc-custom.sofa'] = true }
local bindings = {}
local hooks = {}
local property_writes = {}
local input_request
local last_osd = ''
local saved_document
local include_user_options = false
local current_tracks = {}
local timeouts = {}
local fail_write = false
local fail_second_promote = false
local promote_attempts = 0
local settings_path = '~~/openjoc-settings.json'
local settings_temp_path = settings_path .. '.tmp'
local settings_backup_path = settings_path .. '.bak'

local real_io_open = io.open
io.open = function(path, mode)
    if mode == 'rb' then
        local contents = files[path]
        if not contents then
            return nil, 'not found'
        end
        local offset = 1
        return {
            read = function(_, format)
                assert(format == '*a')
                local result = contents:sub(offset)
                offset = #contents + 1
                return result
            end,
            close = function() return true end,
        }
    elseif mode == 'wb' then
        local buffer = {}
        return {
            write = function(_, ...)
                if fail_write then return nil, 'injected disk-full failure' end
                for index = 1, select('#', ...) do
                    buffer[#buffer + 1] = tostring(select(index, ...))
                end
                return true
            end,
            close = function()
                files[path] = table.concat(buffer)
                return true
            end,
        }
    end
    return real_io_open(path, mode)
end

local real_remove = os.remove
local real_rename = os.rename
os.remove = function(path)
    files[path] = nil
    return true
end
os.rename = function(old_path, new_path)
    if fail_second_promote and old_path == settings_temp_path then
        promote_attempts = promote_attempts + 1
        if promote_attempts == 2 then
            return nil, 'injected replacement failure'
        end
    end
    if not files[old_path] then
        return nil, 'not found'
    end
    if files[new_path] then
        return nil, 'target already exists'
    end
    files[new_path] = files[old_path]
    files[old_path] = nil
    return true
end

local utils = {
    parse_json = function() return nil end,
    format_json = function(document)
        saved_document = document
        return '{"mock":"settings"}'
    end,
    file_info = function(path)
        if existing_paths[path] then return { is_file = true, is_dir = false } end
        return nil, 'not found'
    end,
}
local input = {
    get = function(options) input_request = options end,
}
package.preload['mp.utils'] = function() return utils end
package.preload['mp.input'] = function() return input end

mp = {
    command_native = function(command)
        if command[1] == 'expand-path' or command[1] == 'normalize-path' then
            return command[2]
        end
        error('unexpected mp.command_native command: ' .. tostring(command[1]))
    end,
    get_property = function(name, default)
        if name == 'path' then return 'current-file.mkv' end
        if name == 'current-tracks/audio/decoder' then return 'libopenjoc' end
        if name == 'audio-params/hr-channels' then return '5.1' end
        if name == 'audio-out-params/hr-channels' then return 'Stereo' end
        return default
    end,
    get_property_native = function(name, default)
        if name == 'track-list' then return current_tracks end
        if include_user_options and name == 'ad-lavc-o' then
            return { unrelated_option = 'preserve-me', dialnorm = 'digital' }
        end
        return default
    end,
    set_property = function(name, value)
        property_writes[#property_writes + 1] = { name, value }
        return true
    end,
    set_property_native = function(name, value)
        property_writes[#property_writes + 1] = { name, value }
        return true
    end,
    add_key_binding = function(_, name, callback) bindings[name] = callback end,
    add_forced_key_binding = function(_, name, callback) bindings[name] = callback end,
    remove_key_binding = function(name) bindings[name] = nil end,
    add_hook = function(name, _, callback) hooks[name] = callback end,
    add_timeout = function(seconds, callback)
        local timeout = { seconds = seconds, callback = callback, active = true }
        function timeout:kill() self.active = false end
        timeouts[#timeouts + 1] = timeout
        return timeout
    end,
    observe_property = function() end,
    osd_message = function(text) last_osd = text end,
    msg = { info = function() end, error = function(message) error(message) end },
}

dofile('integrations/mpv/openjoc-settings.lua')
assert(type(bindings['openjoc-settings-toggle']) == 'function', 'toggle binding not registered')
assert(type(hooks.on_preloaded) == 'function', 'per-file hook not registered')

bindings['openjoc-settings-toggle']()
assert(last_osd:find('Output policy: 5.1', 1, true), 'menu did not show the default output selection')
assert(files[settings_path] == nil, 'menu initialization/open wrote settings without an explicit save')
assert(last_osd:find('status rows above are live mpv properties', 1, true), 'menu did not distinguish settings from live status rows')
assert(last_osd:find('Active decoder (read-only): libopenjoc', 1, true), 'menu omitted actual decoder status')
assert(last_osd:find('Decoder output channels (read-only): 5.1', 1, true), 'menu omitted decoder channel status')
assert(last_osd:find('Audio output channels (read-only): Stereo', 1, true), 'menu omitted AO channel status')
assert(not last_osd:find('Advanced decoder options', 1, true), 'unexpected advanced controls')

local output_sequence = {
    '7.1', '5.1.2', '5.1.4', '7.1.2', '7.1.4',
    'Stereo (Speakers)', 'Binaural (Headphones)', '5.1',
}
for _, label in ipairs(output_sequence) do
    bindings['openjoc-settings-right']()
    assert(last_osd:find('Output policy: ' .. label, 1, true),
        'output policy did not cycle to ' .. label)
end
for _ = 1, 8 do bindings['openjoc-settings-down']() end -- discard row
bindings['openjoc-settings-enter']()
assert(bindings['openjoc-settings-up'] == nil, 'discard did not close the menu')
bindings['openjoc-settings-toggle']()
assert(last_osd:find('Output policy: 5.1', 1, true), 'discard did not restore saved output')
assert(last_osd:find('settings may reach other tracks and log warnings', 1, true),
    'menu omitted the best-effort E-AC-3 gate note')

-- Move to the HRTF row, select D2, then choose and submit a local SOFA path.
local windows_sofa = 'C:\\Users\\tester\\OpenJOC\\custom,hrtf.sofa'
existing_paths[windows_sofa] = true
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-right']()
assert(last_osd:find('Binaural virtual layout (when selected): 9.1.6 (experimental)', 1, true),
    'experimental binaural virtual layout was not selectable')
bindings['openjoc-settings-left']()
assert(last_osd:find('Binaural virtual layout (when selected): 7.1.4', 1, true),
    'default binaural virtual layout was not selectable')
bindings['openjoc-settings-right']()
assert(last_osd:find('Binaural virtual layout (when selected): 9.1.6 (experimental)', 1, true),
    'experimental binaural virtual layout could not be restored')
bindings['openjoc-settings-up']()
bindings['openjoc-settings-right']()
assert(last_osd:find('SADIE II D2 / KEMAR', 1, true), 'D2 selection did not update the menu')
bindings['openjoc-settings-right']()
assert(type(input_request) == 'table', 'Custom SOFA did not request text input')
input_request.submit(windows_sofa)
input_request.closed()
assert(last_osd:find('Custom SOFA', 1, true), 'SOFA submission did not return to the menu')

-- Cancel and invalid paths leave the selected built-in preset untouched.
bindings['openjoc-settings-left']() -- custom -> D2
bindings['openjoc-settings-right']() -- D2 -> custom, then cancel
local cancelled_request = input_request
cancelled_request.closed()
assert(last_osd:find('SADIE II D2 / KEMAR', 1, true), 'cancelled SOFA input changed the HRTF selection')
bindings['openjoc-settings-right']() -- retry custom with an invalid path
input_request.submit('/tmp/openjoc-does-not-exist.sofa')
assert(last_osd:find('Choose an existing local SOFA file', 1, true), 'invalid SOFA path was not rejected')
assert(last_osd:find('SADIE II D2 / KEMAR', 1, true), 'invalid SOFA path changed the prior selection')
bindings['openjoc-settings-right']() -- valid Windows path with backslashes and comma
input_request.submit(windows_sofa)
input_request.closed()
assert(last_osd:find('Custom SOFA', 1, true), 'valid Windows SOFA path was not accepted')

-- Save explicitly, then verify the hook merges only E-AC-3 per-file decoder options.
assert(files[settings_path] == nil, 'menu edits were written before the Save row was activated')
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-enter']()
assert(saved_document and saved_document.schema == 1, 'save did not serialize settings')
assert(saved_document.options.render_mode == nil, 'unchanged output mode was unexpectedly written')
assert(saved_document.options.hrtf == 'd2', 'D2 choice was not saved')
assert(saved_document.options.sofa == windows_sofa, 'Windows SOFA path with a comma was not saved exactly')
assert(saved_document.options.virtual_layout == '9.1.6', 'virtual layout choice was not saved')

-- Resaving an existing config exercises the Windows-compatible backup path.
bindings['openjoc-settings-up']()
bindings['openjoc-settings-up']()
bindings['openjoc-settings-up']()
bindings['openjoc-settings-up']()
bindings['openjoc-settings-up']()
bindings['openjoc-settings-up']()
bindings['openjoc-settings-right']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-enter']()
assert(saved_document.options.dialnorm == 'analog', 'second save did not persist Dialnorm')

include_user_options = true
current_tracks = { { type = 'audio', codec = 'flac' } }
hooks.on_preloaded()
assert(#property_writes == 0, 'OpenJOC decoder options leaked to ordinary FLAC')
current_tracks = { { type = 'audio', codec = 'pcm_s16le' } }
hooks.on_preloaded()
assert(#property_writes == 0, 'OpenJOC decoder options leaked to PCM')
current_tracks = { { type = 'audio', codec = 'eac3' } }
hooks.on_preloaded()
local applied = {}
for _, write in ipairs(property_writes) do
    assert(write[1] == 'file-local-options/ad-lavc-o',
        'menu changed a global option or mpv audio output mapping')
    assert(type(write[2]) == 'table', 'decoder options were not set as a native map')
    applied = write[2]
end
assert(applied.hrtf == 'd2' and applied.sofa == windows_sofa
        and applied.dialnorm == 'analog',
    'saved HRTF settings were not applied on the per-file hook')
assert(applied.unrelated_option == 'preserve-me', 'unrelated decoder options were not preserved')
assert(applied['audio-channels'] == nil, 'menu must leave mpv output mapping unchanged')
assert(applied.ad == nil, 'menu must not force decoder selection')
local writes_before_multitrack = #property_writes
current_tracks = {
    { type = 'audio', codec = 'aac', selected = true },
    { type = 'audio', codec = 'eac3', selected = false },
}
hooks.on_preloaded()
assert(#property_writes == writes_before_multitrack + 1,
    'multitrack E-AC-3 file did not receive the best-effort per-file settings map')
assert(property_writes[#property_writes][1] == 'file-local-options/ad-lavc-o',
    'multitrack hook changed a global setting or selected an audio track')
assert(property_writes[#property_writes][2].aid == nil
        and property_writes[#property_writes][2].ad == nil,
    'multitrack hook must not alter aid or force a decoder')

-- Exercise write failure, failed promotion and backup rollback, then retry.
local previous_file = files[settings_path]
for _ = 1, 7 do bindings['openjoc-settings-up']() end -- output row
bindings['openjoc-settings-right']() -- dirty the output policy
for _ = 1, 7 do bindings['openjoc-settings-down']() end -- save row
fail_write = true
bindings['openjoc-settings-enter']()
fail_write = false
assert(files[settings_path] == previous_file, 'failed write changed the saved settings file')
assert(files[settings_temp_path] == nil, 'failed write left a temporary file')
assert(last_osd:find('Settings not saved', 1, true), 'write failure was not reported')
fail_second_promote = true
promote_attempts = 0
bindings['openjoc-settings-enter']()
fail_second_promote = false
assert(files[settings_path] == previous_file, 'failed promotion did not restore the previous settings file')
assert(files[settings_backup_path] == nil, 'failed promotion left a backup file behind')
assert(files[settings_temp_path] == nil, 'failed promotion left a temporary file')
assert(last_osd:find('Settings not saved', 1, true), 'promotion failure was not reported')
bindings['openjoc-settings-enter']()
assert(saved_document.options.render_mode == 'speaker'
        and saved_document.options.speaker_layout == '7.1',
    'retry did not save the pending output selection')

-- The menu must be explicit about what it leaves outside the LAV parity scope.
bindings['openjoc-settings-toggle']()
bindings['openjoc-settings-toggle']()
assert(last_osd:find('LAV output gain', 1, true), 'gain parity limitation is not visible')

-- Idle timeout removes forced bindings and leaves the unsaved draft available.
bindings['openjoc-settings-right']()
local idle_timeout
for _, timeout in ipairs(timeouts) do
    if timeout.seconds == 15 and timeout.active then idle_timeout = timeout end
end
assert(idle_timeout, 'menu did not arm its idle close timer')
idle_timeout.callback()
assert(bindings['openjoc-settings-up'] == nil, 'idle close left forced menu bindings installed')
assert(last_osd:find('draft retained', 1, true), 'idle close did not preserve the unsaved draft')
bindings['openjoc-settings-toggle']()
bindings['openjoc-settings-escape']()
assert(last_osd:find('Output policy:', 1, true)
        and last_osd:find('Status: Unsaved edits. Choose Save or Discard', 1, true),
    'Escape warning hid the still-active menu: ' .. last_osd)
bindings['openjoc-settings-toggle']()
assert(last_osd:find('Output policy:', 1, true)
        and last_osd:find('Status: Unsaved edits. Choose Save or Discard', 1, true),
    'toggle warning hid the still-active menu')

-- Restore global primitives so this file is safe if reused by another test.
io.open = real_io_open
os.remove = real_remove
os.rename = real_rename
print('mpv OpenJOC settings Lua mock checks passed')

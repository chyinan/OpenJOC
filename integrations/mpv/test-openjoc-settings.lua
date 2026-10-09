-- SPDX-FileCopyrightText: 2026 OpenJOC contributors
-- SPDX-License-Identifier: Apache-2.0

-- Mocked mpv API interaction test for the bundled settings panel. Run with LuaJIT.
local files = {}
local existing_paths = { ['/tmp/openjoc-custom.sofa'] = true }
local bindings = {}
local hooks = {}
local observers = {}
local property_writes = {}
local input_request
local last_osd = ''
local last_overlay = ''
local overlay_res_x, overlay_res_y = 0, 0
local screen_w, screen_h = 1280, 720
local mouse_x, mouse_y = 0, 0
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
local legacy_settings_path = '~~/../../config/openjoc-settings.json'
local real_io_open = io.open -- only used for the opt-in ASS preview capture below

-- This interaction test must never write to the host filesystem. Inject the
-- same settings-file IO seam used by production, with every operation backed
-- by the in-memory fixture. The separate test-openjoc-settings-io.lua exercises
-- the real OS adapter (including Windows Unicode paths).
local file_io_module = 'openjoc.settings_file_io.v1'
local previous_file_io = package.loaded[file_io_module]
package.loaded[file_io_module] = {
    read = function(path)
        local contents = files[path]
        if contents == nil then return nil, 'not found', true end
        return contents
    end,
    write = function(path, contents)
        if fail_write then return nil, 'injected disk-full failure' end
        files[path] = contents
        return true
    end,
    remove = function(path)
        files[path] = nil
        return true
    end,
    move = function(old_path, new_path)
        if fail_second_promote and old_path == settings_temp_path then
            promote_attempts = promote_attempts + 1
            if promote_attempts == 2 then
                return nil, 'injected replacement failure'
            end
        end
        if files[old_path] == nil then return nil, 'not found' end
        if files[new_path] ~= nil then return nil, 'target already exists' end
        files[new_path] = files[old_path]
        files[old_path] = nil
        return true
    end,
}

local utils = {
    parse_json = function(contents)
        if contents == 'legacy-settings' then
            return { schema = 1, options = { render_mode = 'binaural', hrtf = 'd2' } }
        end
        return nil
    end,
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
        if command[1] == 'escape-ass' then return command[2] end
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
    get_property_number = function(_, default) return default end,
    get_osd_size = function() return screen_w, screen_h end,
    get_mouse_pos = function() return mouse_x, mouse_y end,
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
    observe_property = function(name, _, callback) observers[name] = callback end,
    create_osd_overlay = function(kind)
        assert(kind == 'ass-events', 'settings panel must use mpv ASS overlay rendering')
        local created = { data = '', res_x = 0, res_y = 0 }
        function created:update()
            last_overlay = self.data
            overlay_res_x, overlay_res_y = self.res_x, self.res_y
        end
        return created
    end,
    osd_message = function(text) last_osd = text end,
    msg = { info = function() end, error = function(message) error(message) end },
}

local function click(x, y)
    mouse_x, mouse_y = x, y
    assert(type(bindings['openjoc-settings-mouse']) == 'function', 'mouse binding is not active')
    bindings['openjoc-settings-mouse']()
end

local function capture_preview(name)
    local directory = os.getenv('OPENJOC_ASS_PREVIEW_DIR')
    if not directory then return end
    local file = real_io_open(directory .. '/' .. name .. '.ass-events', 'wb')
    assert(file, 'could not write requested ASS test capture')
    file:write(last_overlay)
    file:close()
end

local function output_selector_value()
    -- The wide-window first-row value is emitted as its own ASS Dialogue
    -- event at this documented design-grid coordinate.
    for event in last_overlay:gmatch('[^\n]+') do
        if event:find('\\pos(744,198)', 1, true) then
            return event:match('}([^}]*)$')
        end
    end
    return nil
end

dofile('integrations/mpv/openjoc-settings.lua')
assert(type(bindings['openjoc-settings-toggle']) == 'function', 'toggle binding not registered')
assert(type(hooks.on_preloaded) == 'function', 'per-file hook not registered')

bindings['openjoc-settings-toggle']()
assert(last_overlay:find('OpenJOC settings', 1, true), 'panel title was not rendered')
assert(last_overlay:find('Output policy', 1, true) and last_overlay:find('5.1', 1, true),
    'panel did not show the default output selection')
assert(output_selector_value() == '5.1', 'output selector did not show the default value')
assert(files[settings_path] == nil, 'menu initialization/open wrote settings without an explicit save')
assert(last_overlay:find('LIVE PLAYER', 1, true)
        and last_overlay:find('Active decoder', 1, true)
        and last_overlay:find('libopenjoc', 1, true),
    'panel omitted actual decoder status')
assert(last_overlay:find('Decoder output channels', 1, true)
        and last_overlay:find('Audio output channels', 1, true),
    'panel omitted live mpv channel status')
assert(last_overlay:find('Live rows are mpv properties, not JOC diagnostics', 1, true),
    'panel did not distinguish settings from live status')
assert(last_overlay:find('Save', 1, true) and last_overlay:find('Cancel', 1, true),
    'fixed Save and Cancel buttons were not rendered')
assert(last_overlay:find('Click controls or use Up/Down', 1, true),
    'keyboard and mouse help was not rendered')
assert(overlay_res_x == 1280 and overlay_res_y == 720, 'ASS overlay did not use current OSD dimensions')
assert(#last_overlay < 14000, 'panel ASS unexpectedly expanded into a full-screen text page')
capture_preview('openjoc-settings-1280x720')

-- Tab/Shift+Tab remain usable, and the custom-path row is mouse clickable.
bindings['openjoc-settings-tab']()
bindings['openjoc-settings-shift-tab']()
-- At 1280x720 the custom SOFA action is centered at x=991, y=332.
click(991, 332)
assert(type(input_request) == 'table' and last_overlay == '',
    'clicking Set path did not open the path prompt')
bindings['openjoc-settings-toggle']()
assert(last_overlay == '' and bindings['openjoc-settings-mouse'] == nil,
    'toggling during the SOFA modal reopened menu bindings')
input_request.closed()
assert(last_overlay:find('No file selected', 1, true), 'cancelling Set path hid the panel')
for _ = 1, 3 do bindings['openjoc-settings-up']() end -- return focus to output row

local output_sequence = {
    '7.1', '5.1.2', '5.1.4', '7.1.2', '7.1.4',
    'Stereo (Speakers)', 'Binaural (Headphones)', '5.1',
}
for _, label in ipairs(output_sequence) do
    bindings['openjoc-settings-right']()
    assert(output_selector_value() == label,
        'output selector did not cycle to ' .. label .. ': ' .. tostring(output_selector_value()))
end
-- The panel reflows to the compact OSD size and the rendered hitboxes track it.
screen_w, screen_h = 640, 480
observers['osd-dimensions']('osd-dimensions', { w = screen_w, h = screen_h })
assert(overlay_res_x == screen_w and overlay_res_y == screen_h, 'resize did not update ASS PlayRes')
assert(last_overlay:find('LIVE PLAYER', 1, true) and last_overlay:find('Decoder:', 1, true),
    'compact panel omitted a concise live status line')
capture_preview('openjoc-settings-640x480')
local panel_x, panel_y = last_overlay:match('\\pos%((%d+),(%d+)%)')
assert(panel_x and panel_y and tonumber(panel_x) >= 0 and tonumber(panel_y) >= 0
        and tonumber(panel_x) < screen_w and tonumber(panel_y) < screen_h,
    'compact panel background started outside the OSD')
-- At 640x480 the output right-arrow hitbox is centered at x=590, y=124.
click(590, 124)
assert(last_overlay:find('7.1', 1, true), 'compact output hitbox did not advance the selection')
click(375, 124)
assert(last_overlay:find('5.1', 1, true), 'compact output hitbox did not reverse the selection')

-- Clicking outside closes without discarding a dirty draft; Cancel discards it.
click(8, 8)
assert(last_overlay == '', 'outside click did not close the panel')
assert(last_osd:find('Draft kept', 1, true), 'outside click did not preserve the dirty draft')
bindings['openjoc-settings-toggle']()
assert(last_overlay:find('7.1', 1, true) and last_overlay:find('UNSAVED DRAFT', 1, true),
    'reopening did not retain the unsaved output draft')
-- At 640x480 the Cancel button center is x=559, y=379.
click(559, 379)
assert(last_overlay == '', 'Cancel did not close the panel')
assert(files[settings_path] == nil, 'Cancel unexpectedly persisted the draft')
screen_w, screen_h = 1280, 720
observers['osd-dimensions']('osd-dimensions', { w = screen_w, h = screen_h })
bindings['openjoc-settings-toggle']()
assert(last_overlay:find('5.1', 1, true) and not last_overlay:find('UNSAVED DRAFT', 1, true),
    'Cancel did not restore the saved/default output')

-- HRTF presets, the separate custom-file button, modal cancel, and path errors.
local windows_sofa = 'C:\\Users\\tester\\OpenJOC\\custom,hrtf.sofa'
existing_paths[windows_sofa] = true
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-right']()
assert(last_overlay:find('SADIE II D2 / KEMAR', 1, true), 'D2 selection did not update the panel')
bindings['openjoc-settings-right']()
assert(type(input_request) == 'table' and last_overlay == '',
    'custom SOFA did not open the mpv path prompt and hide the panel')
assert(bindings['openjoc-settings-up'] == nil and bindings['openjoc-settings-mouse'] == nil,
    'modal path input left panel key or mouse bindings active')
input_request.submit(windows_sofa)
input_request.closed()
assert(last_overlay:find('Custom SOFA', 1, true)
        and last_overlay:find('custom,hrtf.sofa', 1, true),
    'valid Windows SOFA path did not return to the graphical panel')

-- The file row reopens the prompt; cancel does not change the chosen source.
bindings['openjoc-settings-down']()
bindings['openjoc-settings-enter']()
local cancelled_request = input_request
cancelled_request.closed()
assert(last_overlay:find('Custom SOFA', 1, true), 'cancelled SOFA prompt lost the prior path')
bindings['openjoc-settings-enter']() -- retry with an invalid path
input_request.submit('/tmp/openjoc-does-not-exist.sofa')
input_request.closed()
assert(last_overlay:find('Choose an existing local SOFA file', 1, true),
    'invalid SOFA path was not rejected')
assert(last_overlay:find('Custom SOFA', 1, true), 'invalid SOFA path changed the prior selection')

-- Selecting a built-in preset clears the custom override; it can be restored.
bindings['openjoc-settings-up']()
bindings['openjoc-settings-left']()
assert(last_overlay:find('SADIE II D2 / KEMAR', 1, true),
    'built-in HRTF selection did not replace the custom source')
assert(last_overlay:find('No file selected', 1, true), 'built-in selection did not clear the SOFA path')
bindings['openjoc-settings-right']() -- D2 -> custom prompt
input_request.submit(windows_sofa)
input_request.closed()
assert(last_overlay:find('Custom SOFA', 1, true), 'valid Windows SOFA path was not accepted')

-- Virtual layout cycles only between 7.1.4 and the experimental 9.1.6.
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']()
bindings['openjoc-settings-right']()
assert(last_overlay:find('9.1.6 (experimental)', 1, true),
    'experimental binaural virtual layout was not selectable')
bindings['openjoc-settings-left']()
assert(last_overlay:find('7.1.4', 1, true), 'default binaural virtual layout was not selectable')
bindings['openjoc-settings-right']()
assert(last_overlay:find('9.1.6 (experimental)', 1, true),
    'experimental binaural virtual layout could not be restored')

-- Save explicitly, then verify the hook merges only E-AC-3 per-file decoder options.
assert(files[settings_path] == nil, 'menu edits were written before the Save row was activated')
bindings['openjoc-settings-down']() -- Save action
-- At 1280x720 the fixed Save button is centered at x=855, y=592.
click(855, 592)
assert(saved_document and saved_document.schema == 1, 'save did not serialize settings')
assert(saved_document.options.render_mode == nil, 'unchanged output mode was unexpectedly written')
assert(saved_document.options.hrtf == 'd2', 'D2 choice was not saved')
assert(saved_document.options.sofa == windows_sofa, 'Windows SOFA path with a comma was not saved exactly')
assert(saved_document.options.virtual_layout == '9.1.6', 'virtual layout choice was not saved')

-- Resaving an existing config exercises the Windows-compatible backup path.
for _ = 1, 4 do bindings['openjoc-settings-up']() end -- Save -> HRTF -> Dialnorm
bindings['openjoc-settings-right']()
for _ = 1, 4 do bindings['openjoc-settings-down']() end -- back to Save
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
for _ = 1, 5 do bindings['openjoc-settings-up']() end -- output row
bindings['openjoc-settings-right']() -- dirty the output policy
for _ = 1, 5 do bindings['openjoc-settings-down']() end -- save row
fail_write = true
bindings['openjoc-settings-enter']()
fail_write = false
assert(files[settings_path] == previous_file, 'failed write changed the saved settings file')
assert(files[settings_temp_path] == nil, 'failed write left a temporary file')
assert(last_overlay:find('Settings not saved', 1, true), 'write failure was not reported in the panel')
fail_second_promote = true
promote_attempts = 0
bindings['openjoc-settings-enter']()
fail_second_promote = false
assert(files[settings_path] == previous_file, 'failed promotion did not restore the previous settings file')
assert(files[settings_backup_path] == nil, 'failed promotion left a backup file behind')
assert(files[settings_temp_path] == nil, 'failed promotion left a temporary file')
assert(last_overlay:find('Settings not saved', 1, true), 'promotion failure was not reported in the panel')
bindings['openjoc-settings-enter']()
assert(saved_document.options.render_mode == 'speaker'
        and saved_document.options.speaker_layout == '7.1',
    'retry did not save the pending output selection')

-- The menu must be explicit about what it leaves outside the LAV parity scope.
bindings['openjoc-settings-toggle']()
bindings['openjoc-settings-toggle']()
assert(last_overlay:find('LAV output gain', 1, true), 'gain parity limitation is not visible')

-- Idle timeout removes forced bindings and leaves the unsaved draft available.
bindings['openjoc-settings-right']()
local idle_timeout
for _, timeout in ipairs(timeouts) do
    if timeout.seconds == 15 and timeout.active then idle_timeout = timeout end
end
assert(idle_timeout, 'menu did not arm its idle close timer')
idle_timeout.callback()
assert(bindings['openjoc-settings-up'] == nil, 'idle close left forced menu bindings installed')
assert(last_overlay == '', 'idle close left the graphical panel visible')
assert(last_osd:find('draft retained', 1, true), 'idle close did not preserve the unsaved draft')
bindings['openjoc-settings-toggle']()
bindings['openjoc-settings-escape']()
assert(last_overlay == '', 'Escape did not close the graphical panel')
assert(last_osd:find('Draft kept', 1, true), 'Escape did not preserve the dirty draft')
bindings['openjoc-settings-toggle']()
assert(last_overlay:find('5.1.2', 1, true) and last_overlay:find('UNSAVED DRAFT', 1, true),
    'reopening after Escape did not restore the latest draft')
bindings['openjoc-settings-toggle']()
assert(last_overlay == '', 'toggle did not close the panel')

-- First-launch migration reads the old root/config file without moving it.
files[settings_path] = nil
files[settings_backup_path] = nil
files[legacy_settings_path] = 'legacy-settings'
dofile('integrations/mpv/openjoc-settings.lua')
bindings['openjoc-settings-toggle']()
assert(last_overlay:find('Binaural (Headphones)', 1, true)
        and last_overlay:find('SADIE II D2 / KEMAR', 1, true),
    'legacy settings were not read when the new portable config was absent')
assert(files[legacy_settings_path] == 'legacy-settings', 'migration modified the legacy settings file')
bindings['openjoc-settings-right']()
for _ = 1, 5 do bindings['openjoc-settings-down']() end
bindings['openjoc-settings-enter']()
assert(files[settings_path] ~= nil, 'explicit Save did not write into the new config directory')
assert(files[legacy_settings_path] == 'legacy-settings', 'explicit Save changed the legacy file')

-- Restore the injected module so this file is safe if reused by another test.
package.loaded[file_io_module] = previous_file_io
print('mpv OpenJOC settings Lua mock checks passed')

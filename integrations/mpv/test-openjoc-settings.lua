-- SPDX-FileCopyrightText: 2026 OpenJOC contributors
-- SPDX-License-Identifier: Apache-2.0

-- Mocked mpv API interaction test for the bundled settings panel. Run with LuaJIT.
local files = {}
local existing_paths = { ['/tmp/openjoc-custom.sofa'] = true }
local bindings = {}
local hooks = {}
local observers = {}
local property_writes = {}
local command_calls = {}
local input_request
local last_osd = ''
local last_overlay = ''
local overlay_res_x, overlay_res_y = 0, 0
local screen_w, screen_h = 1280, 720
local mouse_x, mouse_y = 0, 0
local saved_document
local include_user_options = false
local current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 1 } }
local current_decoder = 'libopenjoc'
local current_path = 'current-file.mkv'
local pause_value = false
local active_ad_options = {}
local af_filters = {}
local defer_af_add_visibility = false
local pending_af_filters = {}
local registered_events = {}
local fail_af_command_count = 0
local fail_af_add_count = 0
local fail_af_remove_count = 0
local fail_property_write_count = 0
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
        if contents == 'saved-plus01' then
            return { schema = 1, options = {
                render_mode = 'speaker', speaker_layout = '5.1',
                output_gain_tenths_db = 1,
            } }
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
        if name == 'path' then return current_path end
        if name == 'current-tracks/audio/decoder' then return current_decoder end
        if name == 'audio-params/hr-channels' then return '5.1' end
        if name == 'audio-out-params/hr-channels' then return 'Stereo' end
        return default
    end,
    get_property_native = function(name, default)
        if name == 'track-list' then return current_tracks end
        if name == 'pause' then return pause_value end
        if name == 'af' then return af_filters end
        if name == 'file-local-options/ad-lavc-o' then return active_ad_options end
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
        if name == 'file-local-options/ad-lavc-o' and fail_property_write_count > 0 then
            fail_property_write_count = fail_property_write_count - 1
            return nil, 'injected decoder option write failure'
        end
        property_writes[#property_writes + 1] = { name, value }
        if name == 'file-local-options/ad-lavc-o' then active_ad_options = value end
        return true
    end,
    commandv = function(...)
        local args = { ... }
        command_calls[#command_calls + 1] = args
        if args[1] == 'af' and args[2] == 'add' then
            if fail_af_add_count > 0 then
                fail_af_add_count = fail_af_add_count - 1
                return nil, 'injected AF add error'
            end
            local graph = args[3]:match('^@openjoc_gain:lavfi=%[(.*)%]$')
            assert(graph, 'gain AF graph did not use the named lavfi syntax')
            local filter = {
                name = 'lavfi', label = 'openjoc_gain', enabled = true,
                params = { graph = graph },
            }
            if defer_af_add_visibility then
                pending_af_filters[#pending_af_filters + 1] = filter
            else
                af_filters[#af_filters + 1] = filter
            end
            return true
        elseif args[1] == 'af' and args[2] == 'remove' and args[3] == '@openjoc_gain' then
            if fail_af_remove_count > 0 then
                fail_af_remove_count = fail_af_remove_count - 1
                return nil, 'injected AF remove error'
            end
            for index = #af_filters, 1, -1 do
                if af_filters[index].label == 'openjoc_gain' then table.remove(af_filters, index) end
            end
            return true
        elseif args[1] == 'af-command' then
            assert(args[2] == 'openjoc_gain' and args[3] == 'volume',
                'gain update targeted a non-volume filter')
            assert(args[5] == 'volume', 'gain update used the wrong inner AF command target')
            if fail_af_command_count > 0 then
                fail_af_command_count = fail_af_command_count - 1
                return nil, 'injected af-command error'
            end
            local factor = args[4]
            local found = false
            for _, filter in ipairs(af_filters) do
                if filter.label == 'openjoc_gain' then
                    filter.params.graph = filter.params.graph:gsub(
                        'volume@openjoc_gain=volume=[^:]+',
                        'volume@openjoc_gain=volume=' .. factor)
                    found = true
                end
            end
            if not found then return nil, 'filter not found' end
            return true
        end
        return nil, 'unexpected command: ' .. tostring(args[1])
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
    register_event = function(name, callback) registered_events[name] = callback end,
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

local function run_timeout_with_delay(seconds)
    for _, timeout in ipairs(timeouts) do
        if timeout.active and timeout.seconds == seconds then
            timeout.active = false
            timeout.callback()
            return true
        end
    end
    return false
end

local function drain_short_timeouts(limit)
    limit = limit or 24
    local count = 0
    while count < limit do
        local selected
        for _, timeout in ipairs(timeouts) do
            if timeout.active and timeout.seconds <= 1
                and (not selected or timeout.seconds < selected.seconds) then
                selected = timeout
            end
        end
        if not selected then break end
        selected.active = false
        selected.callback()
        count = count + 1
    end
    return count
end

local function reveal_pending_af_filters()
    for _, filter in ipairs(pending_af_filters) do
        af_filters[#af_filters + 1] = filter
    end
    pending_af_filters = {}
end

local function count_af_adds_since(first_call)
    local count = 0
    for index = first_call, #command_calls do
        local args = command_calls[index]
        if args[1] == 'af' and args[2] == 'add' then count = count + 1 end
    end
    return count
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
    -- The selected first-row value is emitted as its own ASS Dialogue event.
    for event in last_overlay:gmatch('[^\n]+') do
        local value = event:match('}([^}]*)$')
        if value == '5.1' or value == '7.1' or value == '5.1.2'
            or value == '5.1.4' or value == '7.1.2' or value == '7.1.4'
            or value == 'Stereo (Speakers)' or value == 'Binaural (Headphones)' then return value end
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
assert(last_overlay:find('Live gain:', 1, true)
        and last_overlay:find('selected libopenjoc E-AC-3', 1, true),
    'panel did not distinguish live gain eligibility from decoder status')
assert(last_overlay:find('Save', 1, true) and last_overlay:find('Apply Current', 1, true)
        and last_overlay:find('Cancel', 1, true),
    'fixed Save, Apply Current and Cancel buttons were not rendered')
assert(last_overlay:find('Click controls or use Up/Down', 1, true),
    'keyboard and mouse help was not rendered')
assert(overlay_res_x == 1280 and overlay_res_y == 720, 'ASS overlay did not use current OSD dimensions')
assert(#last_overlay < 14000, 'panel ASS unexpectedly expanded into a full-screen text page')
capture_preview('openjoc-settings-1280x720')

-- The live gain stage is independent of decoder AVOptions and uses the
-- post-render volume command with the exact double factor text.
local function find_gain_filter()
    for _, filter in ipairs(af_filters) do
        if filter.label == 'openjoc_gain' then return filter end
    end
    return nil
end
local unity_filter = assert(find_gain_filter(), 'active OpenJOC did not receive its named gain filter')
assert(unity_filter.params.graph == 'volume@openjoc_gain=volume=1:precision=float',
    '0 dB was not inserted as exact unity')
assert(not active_ad_options.output_gain_tenths_db,
    'live gain was incorrectly added to the decoder option map')
for _ = 1, 5 do bindings['openjoc-settings-down']() end -- focus gain row
bindings['openjoc-settings-right']() -- preview +0.1 dB
assert(last_overlay:find('+0.1 dB', 1, true), 'gain row did not preview a 0.1 dB step')
local gain_settle
for _, timeout in ipairs(timeouts) do
    if timeout.active and timeout.seconds == 0.35 then gain_settle = timeout end
end
assert(gain_settle, 'new filter command did not use a deferred readiness check')
gain_settle.active = false
gain_settle.callback()
local plus_01 = string.format('%.17g', math.pow(10.0, 1 / 200.0))
assert(command_calls[#command_calls][1] == 'af-command'
        and command_calls[#command_calls][2] == 'openjoc_gain'
        and command_calls[#command_calls][3] == 'volume'
        and command_calls[#command_calls][4] == plus_01
        and command_calls[#command_calls][5] == 'volume',
    'live +0.1 dB did not use the exact runtime volume command contract')
assert(find_gain_filter().params.graph:find('volume=' .. plus_01, 1, true),
    'live command did not update the named gain graph')
assert(not active_ad_options.output_gain_tenths_db,
    'live gain preview wrote an FFmpeg decoder option')
for _ = 1, 3 do bindings['openjoc-settings-down']() end -- Save, Apply, Cancel
bindings['openjoc-settings-enter']()
assert(last_overlay == '', 'Cancel did not close after restoring a gain preview')
assert(find_gain_filter().params.graph == 'volume@openjoc_gain=volume=1:precision=float',
    'Cancel did not restore the original unity gain')
assert(files[settings_path] == nil, 'gain preview or Cancel unexpectedly persisted settings')
bindings['openjoc-settings-toggle']()

-- Sweep every 0.1 dB value from -20.0 through +20.0 while the draft output
-- policy is binaural. Gain stays a post-render command, independent of policy.
bindings['openjoc-settings-left']() -- 5.1 -> Binaural draft
assert(last_overlay:find('Binaural (Headphones)', 1, true),
    'could not select binaural draft for the independent gain sweep')
for _ = 1, 5 do bindings['openjoc-settings-down']() end
local sweep_start = #command_calls + 1
for _ = 1, 200 do bindings['openjoc-settings-left']() end
for _ = 1, 400 do bindings['openjoc-settings-right']() end
local seen_gain_factors = {}
for index = sweep_start, #command_calls do
    local call = command_calls[index]
    if call[1] == 'af-command' and call[2] == 'openjoc_gain'
        and call[3] == 'volume' and call[5] == 'volume' then
        seen_gain_factors[call[4]] = true
    end
end
for tenths_db = -200, 200 do
    local factor = tenths_db == 0 and '1'
        or string.format('%.17g', math.pow(10.0, tenths_db / 200.0))
    assert(seen_gain_factors[factor],
        'runtime gain sweep omitted exact factor for tenths-dB value ' .. tostring(tenths_db))
end
assert(find_gain_filter().params.graph:find('volume=' .. string.format('%.17g', 10.0), 1, true),
    'gain sweep did not reach the +20 dB upper bound')
for _ = 1, 3 do bindings['openjoc-settings-down']() end -- Save, Apply, Cancel
bindings['openjoc-settings-enter']()
assert(find_gain_filter().params.graph == 'volume@openjoc_gain=volume=1:precision=float',
    'Cancel after the exhaustive sweep did not restore exact unity')
assert(files[settings_path] == nil, 'uncommitted gain sweep was persisted')
bindings['openjoc-settings-toggle']()

-- Tab/Shift+Tab remain usable, and the custom-path row is mouse clickable.
bindings['openjoc-settings-tab']()
bindings['openjoc-settings-shift-tab']()
-- At 1280x720 the custom SOFA action is centered at x=991, y=301.
click(991, 301)
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
-- Compact gain controls remain in-bounds: increment and Reset by mouse.
click(537, 251)
assert(last_overlay:find('+0.1 dB', 1, true), 'compact gain increment hitbox did not preview')
click(580, 251)
assert(last_overlay:find('0.0 dB', 1, true), 'compact gain Reset hitbox did not restore unity')
assert(find_gain_filter().params.graph == 'volume@openjoc_gain=volume=1:precision=float',
    'compact mouse Reset did not restore exact unity')

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
bindings['openjoc-settings-down']()
bindings['openjoc-settings-down']() -- move from virtual layout through gain to Save
-- At 1280x720 the fixed Save button is centered at x=745, y=603.
click(745, 603)
assert(saved_document and saved_document.schema == 1, 'save did not serialize settings')
assert(saved_document.options.render_mode == nil, 'unchanged output mode was unexpectedly written')
assert(saved_document.options.hrtf == 'd2', 'D2 choice was not saved')
assert(saved_document.options.sofa == windows_sofa, 'Windows SOFA path with a comma was not saved exactly')
assert(saved_document.options.virtual_layout == '9.1.6', 'virtual layout choice was not saved')

-- Resaving an existing config exercises the Windows-compatible backup path.
for _ = 1, 5 do bindings['openjoc-settings-up']() end -- Save -> Dialnorm
bindings['openjoc-settings-right']()
for _ = 1, 5 do bindings['openjoc-settings-down']() end -- back to Save
bindings['openjoc-settings-enter']()
assert(saved_document.options.dialnorm == 'analog', 'second save did not persist Dialnorm')

include_user_options = true
current_tracks = { { type = 'audio', codec = 'flac', selected = true, id = 2 } }
observers['track-list']('track-list', current_tracks)
local writes_before_ineligible_apply = #property_writes
bindings['openjoc-settings-down']() -- Apply Current
bindings['openjoc-settings-enter']()
assert(#property_writes == writes_before_ineligible_apply
        and last_overlay:find('requires an active libopenjoc E-AC-3', 1, true),
    'Apply Current wrote options or claimed success for ordinary FLAC')
bindings['openjoc-settings-up']() -- restore focus to Save for the later row tests
hooks.on_preloaded()
assert(#property_writes == 0, 'OpenJOC decoder options leaked to ordinary FLAC')
assert(not find_gain_filter(), 'named gain filter remained on ordinary FLAC')
current_tracks = { { type = 'audio', codec = 'pcm_s16le', selected = true, id = 3 } }
observers['track-list']('track-list', current_tracks)
hooks.on_preloaded()
assert(#property_writes == 0, 'OpenJOC decoder options leaked to PCM')
assert(not find_gain_filter(), 'named gain filter remained on ordinary PCM')
current_decoder = 'spdif'
current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 4 } }
observers['current-tracks/audio/decoder']('current-tracks/audio/decoder', current_decoder)
assert(not find_gain_filter(), 'named gain filter remained on E-AC-3 passthrough')
current_decoder = 'libopenjoc'
observers['current-tracks/audio/decoder']('current-tracks/audio/decoder', current_decoder)
assert(run_timeout_with_delay(0.20), 'track lifecycle did not defer gain reinsertion until decoder state settled')
assert(find_gain_filter(), 'named gain filter was not restored for confirmed OpenJOC')
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
assert(applied.output_gain_tenths_db == nil,
    'saved live gain was incorrectly passed as an FFmpeg decoder option')
local writes_before_multitrack = #property_writes
current_tracks = {
    { type = 'audio', codec = 'aac', selected = true, id = 5 },
    { type = 'audio', codec = 'eac3', selected = false, id = 6 },
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
for _ = 1, 6 do bindings['openjoc-settings-up']() end -- output row
bindings['openjoc-settings-right']() -- dirty the output policy
for _ = 1, 6 do bindings['openjoc-settings-down']() end -- save row
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

-- The live gain controls and Apply Current action are present and explicit.
bindings['openjoc-settings-toggle']()
bindings['openjoc-settings-toggle']()
assert(last_overlay:find('Live output gain', 1, true)
        and last_overlay:find('Reset', 1, true)
        and last_overlay:find('Apply Current', 1, true),
    'gain controls or Apply Current action are missing')

-- Idle timeout removes forced bindings and leaves the unsaved draft available.
bindings['openjoc-settings-right']()
assert(last_overlay:find('UNSAVED DRAFT', 1, true), 'output change was not retained as a draft')
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

-- Reinitialize from a nonzero saved gain to cover pending Save, failed preview
-- rollback, pause/resume latest-value behavior, Apply Current, and cleanup.
files[settings_path] = 'saved-plus01'
current_path = 'gain-lifecycle.mkv'
current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 21 } }
current_decoder = 'libopenjoc'
active_ad_options = {}
include_user_options = false
af_filters = {}
dofile('integrations/mpv/openjoc-settings.lua')
bindings['openjoc-settings-toggle']()
for _ = 1, 5 do bindings['openjoc-settings-down']() end -- gain row
bindings['openjoc-settings-right']() -- saved +0.1 -> requested +0.2
bindings['openjoc-settings-down']() -- Save while the new filter is settling
bindings['openjoc-settings-enter']()
assert(last_overlay:find('live gain is still queued and not yet confirmed', 1, true),
    'Save falsely claimed a queued live gain preview had applied')
drain_short_timeouts()
local plus_02 = string.format('%.17g', math.pow(10.0, 2 / 200.0))
assert(find_gain_filter().params.graph:find('volume=' .. plus_02, 1, true),
    'queued live preview did not apply after the filter initialized')
assert(saved_document.options.output_gain_tenths_db == 2,
    'independent gain setting was not saved in the JSON options')
assert(not active_ad_options.output_gain_tenths_db,
    'saved gain leaked into the decoder-option map')

-- A failed af-command must not report success or discard a nonzero rollback
-- baseline. Cancel restores the previous value even after all retries fail.
bindings['openjoc-settings-up']() -- gain row
fail_af_command_count = 8
bindings['openjoc-settings-right']() -- request +0.3 dB
drain_short_timeouts()
assert(last_overlay:find('could not be confirmed after bounded retries', 1, true),
    'failed live update did not remain visibly unconfirmed')
local plus_03 = string.format('%.17g', math.pow(10.0, 3 / 200.0))
assert(find_gain_filter().params.graph:find('volume=' .. plus_02, 1, true),
    'failed af-command changed the known active gain unexpectedly')
for _ = 1, 3 do bindings['openjoc-settings-down']() end -- Save, Apply, Cancel
bindings['openjoc-settings-enter']()
assert(find_gain_filter().params.graph:find('volume=' .. plus_02, 1, true),
    'Cancel failed to preserve the original nonzero gain after a preview error')

-- Save after a failed preview must report the current live gain as unknown or
-- unconfirmed rather than claiming playback was unchanged.
bindings['openjoc-settings-toggle']()
for _ = 1, 5 do bindings['openjoc-settings-down']() end
fail_af_command_count = 8
bindings['openjoc-settings-right']()
drain_short_timeouts()
bindings['openjoc-settings-down']() -- Save
bindings['openjoc-settings-enter']()
assert(last_overlay:find('Saved for the next file; live gain is not confirmed', 1, true),
    'Save after failed preview made a false unchanged-playback claim')
assert(saved_document.options.output_gain_tenths_db == 3,
    'Save after a failed preview did not persist the next-file gain choice')

-- While paused, multiple gain moves queue only the latest target and do not
-- spend retry deadlines; resume applies the most recent value.
bindings['openjoc-settings-toggle']()
bindings['openjoc-settings-toggle']()
pause_value = true
observers['pause']('pause', true)
for _ = 1, 5 do bindings['openjoc-settings-down']() end
bindings['openjoc-settings-right']() -- +0.4 dB
bindings['openjoc-settings-right']() -- latest +0.5 dB
assert(drain_short_timeouts() == 0, 'paused live gain burned a retry timer')
pause_value = false
observers['pause']('pause', false)
drain_short_timeouts()
local plus_05 = string.format('%.17g', math.pow(10.0, 5 / 200.0))
assert(find_gain_filter().params.graph:find('volume=' .. plus_05, 1, true),
    'resume did not apply the latest queued gain value')
for _ = 1, 3 do bindings['openjoc-settings-down']() end
bindings['openjoc-settings-enter']()
assert(find_gain_filter().params.graph:find('volume=' .. plus_02, 1, true),
    'Cancel after resume did not restore the pre-preview nonzero baseline')

-- Cancel before resume cancels the queued preview epoch; changing files before
-- resume must also prevent a stale gain command from reaching ordinary audio.
bindings['openjoc-settings-toggle']()
pause_value = true
observers['pause']('pause', true)
for _ = 1, 5 do bindings['openjoc-settings-down']() end
local command_count_before_paused_cancel = #command_calls
bindings['openjoc-settings-right']() -- queued +0.4 dB
for _ = 1, 3 do bindings['openjoc-settings-down']() end
bindings['openjoc-settings-enter']()
assert(#command_calls == command_count_before_paused_cancel,
    'Cancel while paused issued a stale preview command')
pause_value = false
observers['pause']('pause', false)
drain_short_timeouts()
assert(find_gain_filter().params.graph:find('volume=' .. plus_02, 1, true),
    'cancelled paused preview applied after resume')

bindings['openjoc-settings-toggle']()
pause_value = true
observers['pause']('pause', true)
for _ = 1, 5 do bindings['openjoc-settings-down']() end
local commands_before_file_change = #command_calls
bindings['openjoc-settings-right']() -- pending +0.4 dB
current_path = 'ordinary-next.mkv'
current_tracks = { { type = 'audio', codec = 'flac', selected = true, id = 22 } }
current_decoder = 'eac3'
observers['path']('path', current_path)
observers['track-list']('track-list', current_tracks)
observers['current-tracks/audio/decoder']('current-tracks/audio/decoder', current_decoder)
drain_short_timeouts()
assert(not find_gain_filter(), 'gain filter remained attached to a following FLAC file')
pause_value = false
observers['pause']('pause', false)
drain_short_timeouts()
for index = commands_before_file_change + 1, #command_calls do
    local call = command_calls[index]
    assert(not (call[1] == 'af-command' and call[2] == 'openjoc_gain'
        and call[4] == string.format('%.17g', math.pow(10.0, 4 / 200.0))),
        'stale queued gain reached the next ordinary file')
end
for _ = 1, 3 do bindings['openjoc-settings-down']() end -- Cancel stale file-change draft
bindings['openjoc-settings-enter']()
bindings['openjoc-settings-toggle']() -- reopen at the output row

-- Apply Current writes only the file-local decoder option map, leaves unrelated
-- settings intact, suspends the filter, and restores live gain after validation.
current_path = 'apply-current.mkv'
current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 23 } }
current_decoder = 'libopenjoc'
active_ad_options = { unrelated_option = 'preserve-me' }
observers['path']('path', current_path)
observers['track-list']('track-list', current_tracks)
observers['current-tracks/audio/decoder']('current-tracks/audio/decoder', current_decoder)
drain_short_timeouts()
assert(find_gain_filter(), 'OpenJOC gain filter was not restored for the new confirmed track')
bindings['openjoc-settings-right']() -- 5.1 -> 7.1 draft
for _ = 1, 7 do bindings['openjoc-settings-down']() end -- Apply Current
local writes_before_apply = #property_writes
bindings['openjoc-settings-enter']()
assert(#property_writes == writes_before_apply + 1,
    'Apply Current did not perform one file-local decoder-option update')
local apply_write = property_writes[#property_writes]
assert(apply_write[1] == 'file-local-options/ad-lavc-o'
        and apply_write[2].speaker_layout == '7.1'
        and apply_write[2].unrelated_option == 'preserve-me',
    'Apply Current failed to merge the target layout or preserve unrelated options')
assert(apply_write[2].ad == nil and apply_write[2].aid == nil
        and apply_write[2]['audio-channels'] == nil
        and apply_write[2].output_gain_tenths_db == nil,
    'Apply Current changed decoder selection, mpv routing, or leaked gain into AVOptions')
assert(not find_gain_filter(), 'Apply Current did not suspend its labeled gain stage')
assert(run_timeout_with_delay(0.75), 'Apply Current did not arm bounded reinit validation')
assert(find_gain_filter().params.graph:find('volume=' .. plus_03, 1, true),
    'Apply Current did not restore the requested live gain after decoder validation')
assert(last_overlay:find('Applied to current playback', 1, true),
    'Apply Current reported no success after validated reinitialization')
drain_short_timeouts() -- complete strict post-add AF readiness probing before the next action

-- A failed file-local write must not claim Apply succeeded; restore the filter
-- with its known runtime level so the operation fails without stale UI state.
fail_property_write_count = 1
bindings['openjoc-settings-enter']()
assert(last_overlay:find('Apply Current failed', 1, true),
    'failed Apply Current write did not report an error')
assert(find_gain_filter().params.graph:find('volume=' .. plus_03, 1, true),
    'failed Apply Current did not restore the known live gain stage')

-- Ordinary-track cleanup retries removal. If remove fails, neutralize this
-- filter first; a double failure must remain visible until cleanup succeeds.
fail_af_remove_count = 1
current_tracks = { { type = 'audio', codec = 'flac', selected = true, id = 24 } }
observers['track-list']('track-list', current_tracks)
assert(find_gain_filter().params.graph == 'volume@openjoc_gain=volume=1:precision=float',
    'failed removal did not use exact-unity fallback')
assert(last_overlay:find('unity fallback is active', 1, true),
    'unity fallback warning was not visible')
assert(run_timeout_with_delay(0.15), 'failed removal did not schedule a bounded cleanup retry')
assert(not find_gain_filter(), 'cleanup retry left the gain filter on FLAC')

current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 25 } }
current_decoder = 'libopenjoc'
observers['track-list']('track-list', current_tracks)
observers['current-tracks/audio/decoder']('current-tracks/audio/decoder', current_decoder)
drain_short_timeouts()
assert(find_gain_filter(), 'confirmed OpenJOC track did not get a new gain stage')
fail_af_remove_count = 1
fail_af_command_count = 1
current_tracks = { { type = 'audio', codec = 'flac', selected = true, id = 26 } }
observers['track-list']('track-list', current_tracks)
assert(last_overlay:find('unity fallback also failed', 1, true),
    'double cleanup failure did not remain visibly warned')
drain_short_timeouts()
assert(not find_gain_filter(), 'bounded double-failure cleanup retry did not remove the filter')

-- First-launch migration reads the old root/config file without moving it.
files[settings_path] = nil
files[settings_backup_path] = nil
files[legacy_settings_path] = 'legacy-settings'
current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 7 } }
current_decoder = 'libopenjoc'
include_user_options = false
active_ad_options = {}
dofile('integrations/mpv/openjoc-settings.lua')
bindings['openjoc-settings-toggle']()
assert(last_overlay:find('Binaural (Headphones)', 1, true)
        and last_overlay:find('SADIE II D2 / KEMAR', 1, true),
    'legacy settings were not read when the new portable config was absent')
assert(files[legacy_settings_path] == 'legacy-settings', 'migration modified the legacy settings file')
bindings['openjoc-settings-right']()
for _ = 1, 6 do bindings['openjoc-settings-down']() end
bindings['openjoc-settings-enter']()
assert(files[settings_path] ~= nil, 'explicit Save did not write into the new config directory')
assert(files[legacy_settings_path] == 'legacy-settings', 'explicit Save changed the legacy file')

-- Model mpv acknowledging `af add` before the named filter is observable.
-- Multiple slider requests must coalesce onto that single insertion and the
-- latest requested factor must win once the filter becomes visible.
for _, timeout in ipairs(timeouts) do timeout.active = false end
files[settings_path] = 'saved-plus01'
files[legacy_settings_path] = nil
current_path = 'async-af-add-latest.mkv'
current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 31 } }
current_decoder = 'libopenjoc'
active_ad_options = {}
af_filters = {}
pending_af_filters = {}
defer_af_add_visibility = true
local async_add_begin = #command_calls + 1
dofile('integrations/mpv/openjoc-settings.lua')
assert(count_af_adds_since(async_add_begin) == 1 and #pending_af_filters == 1,
    'startup did not retain exactly one not-yet-visible gain insertion')
bindings['openjoc-settings-toggle']()
for _ = 1, 5 do bindings['openjoc-settings-down']() end
bindings['openjoc-settings-right']() -- saved +0.1 -> latest +0.2 dB
bindings['openjoc-settings-right']() -- latest +0.3 dB
assert(count_af_adds_since(async_add_begin) == 1 and #pending_af_filters == 1,
    'gain previews duplicated an in-flight AF add')
for _ = 1, 2 do bindings['openjoc-settings-down']() end -- Apply Current
local writes_before_async_apply = #property_writes
bindings['openjoc-settings-enter']()
assert(#property_writes == writes_before_async_apply,
    'Apply Current wrote decoder options while the accepted gain insertion was invisible')
assert(last_overlay:find('waiting for the live gain filter to initialize', 1, true),
    'Apply Current did not explain that it was waiting for gain-filter initialization')
assert(count_af_adds_since(async_add_begin) == 1 and #pending_af_filters == 1,
    'blocked Apply Current discarded or duplicated the pending insertion')
reveal_pending_af_filters()
defer_af_add_visibility = false
drain_short_timeouts()
local async_plus_03 = string.format('%.17g', math.pow(10.0, 3 / 200.0))
assert(#af_filters == 1 and find_gain_filter().params.graph:find('volume=' .. async_plus_03, 1, true),
    'latest coalesced gain did not apply after the filter became visible')
assert(count_af_adds_since(async_add_begin) == 1,
    'async filter initialization required a duplicate AF add')
bindings['openjoc-settings-enter']() -- retry Apply Current after initialization
assert(#property_writes == writes_before_async_apply + 1,
    'Apply Current retry did not proceed after filter initialization')
assert(run_timeout_with_delay(0.75), 'ready Apply Current retry did not arm reinit validation')
assert(find_gain_filter().params.graph:find('volume=' .. async_plus_03, 1, true),
    'ready Apply Current retry did not restore the latest gain')

-- Cancel while the first insertion is still invisible replaces the queued
-- preview with the saved baseline. Once visible, no stale preview is applied
-- and no orphan duplicate filter remains in the chain.
for _, timeout in ipairs(timeouts) do timeout.active = false end
files[settings_path] = 'saved-plus01'
current_path = 'async-af-add-cancel.mkv'
current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 32 } }
current_decoder = 'libopenjoc'
af_filters = {}
pending_af_filters = {}
defer_af_add_visibility = true
local async_cancel_begin = #command_calls + 1
dofile('integrations/mpv/openjoc-settings.lua')
bindings['openjoc-settings-toggle']()
for _ = 1, 5 do bindings['openjoc-settings-down']() end
bindings['openjoc-settings-right']() -- preview +0.2 dB while startup AF is pending
for _ = 1, 3 do bindings['openjoc-settings-down']() end -- Cancel
bindings['openjoc-settings-enter']()
assert(count_af_adds_since(async_cancel_begin) == 1 and #pending_af_filters == 1,
    'Cancel duplicated or discarded the in-flight AF add')
reveal_pending_af_filters()
defer_af_add_visibility = false
drain_short_timeouts()
local async_saved_plus_01 = string.format('%.17g', math.pow(10.0, 1 / 200.0))
assert(#af_filters == 1 and find_gain_filter().params.graph:find('volume=' .. async_saved_plus_01, 1, true),
    'Cancel did not restore the saved gain when the filter became visible')
assert(count_af_adds_since(async_cancel_begin) == 1,
    'Cancel-after-pending-add left a duplicate gain filter')
for index = async_cancel_begin, #command_calls do
    local args = command_calls[index]
    assert(not (args[1] == 'af-command' and args[4]
        == string.format('%.17g', math.pow(10.0, 2 / 200.0))),
        'Cancel applied its stale queued preview after the filter became visible')
end

-- If the selected decoder changes while the original add is still invisible,
-- retain a cleanup tombstone. A later AF visibility event removes only our
-- label and leaves unrelated user filters untouched.
for _, timeout in ipairs(timeouts) do timeout.active = false end
files[settings_path] = 'saved-plus01'
current_path = 'async-af-add-before-codec-change.mkv'
current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 33 } }
current_decoder = 'libopenjoc'
local unrelated_filter = {
    name = 'lavfi', label = 'user_equalizer', enabled = true,
    params = { graph = 'volume=0.8' },
}
af_filters = { unrelated_filter }
pending_af_filters = {}
defer_af_add_visibility = true
local async_cleanup_begin = #command_calls + 1
dofile('integrations/mpv/openjoc-settings.lua')
assert(count_af_adds_since(async_cleanup_begin) == 1 and #pending_af_filters == 1,
    'path-change cleanup test did not create one hidden gain filter')
current_path = 'async-af-add-now-flac.mkv'
current_tracks = { { type = 'audio', codec = 'flac', selected = true, id = 34 } }
current_decoder = 'ffmpeg'
observers['path']('path', current_path)
observers['track-list']('track-list', current_tracks)
observers['current-tracks/audio/decoder']('current-tracks/audio/decoder', current_decoder)
assert(run_timeout_with_delay(0.20),
    'codec-change lifecycle did not leave a pending-add cleanup timer')
reveal_pending_af_filters()
defer_af_add_visibility = false
fail_af_remove_count = 1
observers['af']('af', af_filters)
assert(find_gain_filter().params.graph == 'volume@openjoc_gain=volume=1:precision=float',
    'late cleanup remove failure did not neutralize the owned filter to unity')
assert(last_osd:find('unity fallback is active', 1, true),
    'late cleanup remove failure did not report its unity fallback while the menu was closed')
assert(run_timeout_with_delay(0.15),
    'late cleanup remove failure did not transfer to the bounded removal retry')
assert(not find_gain_filter(), 'late AF visibility leaked the pending gain filter onto FLAC')
assert(#af_filters == 1 and af_filters[1].label == 'user_equalizer',
    'late cleanup removed or changed an unrelated user audio filter')
for index = async_cleanup_begin, #command_calls do
    local args = command_calls[index]
    assert(not (args[1] == 'af' and args[2] == 'remove'
        and args[3] ~= '@openjoc_gain'),
        'late cleanup attempted to remove a filter other than the owned gain stage')
end

-- A visible unity-stage filter is not considered initialized until a strict
-- same-factor command succeeds. Apply stays blocked through two transient
-- command errors, then succeeds after bounded readiness probing without any
-- gain slider interaction.
for _, timeout in ipairs(timeouts) do timeout.active = false end
files[settings_path] = 'saved-plus01'
current_path = 'startup-filter-readiness.mkv'
current_tracks = { { type = 'audio', codec = 'eac3', selected = true, id = 35 } }
current_decoder = 'libopenjoc'
active_ad_options = {}
af_filters = {}
pending_af_filters = {}
defer_af_add_visibility = false
fail_af_command_count = 2
local startup_ready_begin = #command_calls + 1
dofile('integrations/mpv/openjoc-settings.lua')
assert(find_gain_filter(), 'startup did not add the unity/nonzero initial gain graph')
bindings['openjoc-settings-toggle']()
for _ = 1, 7 do bindings['openjoc-settings-down']() end -- Apply Current
local writes_before_startup_apply = #property_writes
bindings['openjoc-settings-enter']()
assert(#property_writes == writes_before_startup_apply,
    'Apply Current ran before the startup gain filter proved command readiness')
assert(last_overlay:find('waiting for the live gain filter to initialize', 1, true),
    'startup Apply Current did not report that gain initialization was pending')
assert(run_timeout_with_delay(0.05), 'startup gain readiness probe did not start')
assert(run_timeout_with_delay(0.05), 'startup readiness retry after first command error was missing')
assert(#property_writes == writes_before_startup_apply,
    'Apply Current proceeded after only failed readiness probes')
assert(run_timeout_with_delay(0.10), 'startup readiness retry after second command error was missing')
local startup_plus_01 = string.format('%.17g', math.pow(10.0, 1 / 200.0))
assert(find_gain_filter().params.graph:find('volume=' .. startup_plus_01, 1, true),
    'same-factor readiness probe changed the configured startup gain')
bindings['openjoc-settings-enter']() -- ready, no preview interaction required
assert(#property_writes == writes_before_startup_apply + 1,
    'Apply Current did not unlock after a strict startup readiness command succeeded')
assert(run_timeout_with_delay(0.75), 'startup-ready Apply Current did not validate')
assert(find_gain_filter().params.graph:find('volume=' .. startup_plus_01, 1, true),
    'startup-ready Apply Current did not restore saved gain')
drain_short_timeouts()

-- Restore the injected module so this file is safe if reused by another test.
package.loaded[file_io_module] = previous_file_io
print('mpv OpenJOC settings Lua mock checks passed')

-- SPDX-FileCopyrightText: 2026 OpenJOC contributors
-- SPDX-License-Identifier: Apache-2.0

-- Small OpenJOC settings menu for the project-provided mpv bundle. Decoder
-- AVOptions are fixed when libopenjoc is initialized, so changes are saved
-- explicitly and applied from on_preloaded before decoder creation. This script
-- never reloads, seeks, or changes the currently playing file.

local utils = require 'mp.utils'
local input = require 'mp.input'

local CONFIG_PATH = mp.command_native({ 'expand-path', '~~/openjoc-settings.json' })
local CONFIG_TEMP_PATH = CONFIG_PATH .. '.tmp'
local CONFIG_BACKUP_PATH = CONFIG_PATH .. '.bak'

local DEFAULTS = {
    render_mode = 'speaker',
    speaker_layout = '5.1',
    virtual_layout = '7.1.4',
    hrtf = 'd1',
    sofa = '',
    dialnorm = 'default',
}

local OUTPUTS = {
    {
        label = 'Stereo (Speakers)',
        render_mode = 'stereo',
        speaker_layout = '2.0',
    },
    {
        label = 'Binaural (Headphones)',
        render_mode = 'binaural',
    },
    {
        label = '5.1',
        render_mode = 'speaker',
        speaker_layout = '5.1',
    },
    {
        label = '7.1',
        render_mode = 'speaker',
        speaker_layout = '7.1',
    },
    {
        label = '5.1.2',
        render_mode = 'speaker',
        speaker_layout = '5.1.2',
    },
    {
        label = '5.1.4',
        render_mode = 'speaker',
        speaker_layout = '5.1.4',
    },
    {
        label = '7.1.2',
        render_mode = 'speaker',
        speaker_layout = '7.1.2',
    },
    {
        label = '7.1.4',
        render_mode = 'speaker',
        speaker_layout = '7.1.4',
    },
}

local VALUE_SETS = {
    render_mode = { speaker = true, stereo = true, binaural = true },
    speaker_layout = {
        ['2.0'] = true, ['5.1'] = true, ['7.1'] = true,
        ['5.1.2'] = true, ['5.1.4'] = true, ['7.1.2'] = true, ['7.1.4'] = true,
    },
    virtual_layout = { ['7.1.4'] = true, ['9.1.6'] = true },
    hrtf = { d1 = true, d2 = true },
    dialnorm = { default = true, digital = true, analog = true },
}

local function empty_state()
    return { options = {} }
end

local function copy_state(value)
    local copy = empty_state()
    for key, item in pairs(value.options or {}) do
        copy.options[key] = item
    end
    return copy
end

local function sanitize_state(value)
    local safe = empty_state()
    if type(value) ~= 'table' or value.schema ~= 1 or type(value.options) ~= 'table' then
        return safe
    end
    for key, item in pairs(value.options) do
        if VALUE_SETS[key] and VALUE_SETS[key][item] then
            safe.options[key] = item
        elseif key == 'sofa' and type(item) == 'string' and #item <= 4096
            and not item:find('[\0\r\n]') then
            safe.options.sofa = item
        end
    end
    return safe
end

local function load_state()
    local file = io.open(CONFIG_PATH, 'rb')
    if not file then
        -- Recover the previous settings if Windows stopped between moving the
        -- old file aside and installing the newly written temporary file.
        local backup = io.open(CONFIG_BACKUP_PATH, 'rb')
        if backup then
            backup:close()
            os.rename(CONFIG_BACKUP_PATH, CONFIG_PATH)
            file = io.open(CONFIG_PATH, 'rb') or io.open(CONFIG_BACKUP_PATH, 'rb')
        end
    end
    if not file then
        return empty_state()
    end
    local contents = file:read('*a')
    file:close()
    local parsed = utils.parse_json(contents)
    return sanitize_state(parsed)
end

local saved = load_state()
local draft = copy_state(saved)
local dirty = false
local row = 1
local menu_open = false
local menu_timeout
local status_timeout
local status_message

local function key_value_table(raw)
    local result = {}
    if type(raw) == 'table' then
        for key, value in pairs(raw) do
            if type(key) == 'string' and value ~= nil then
                result[key] = tostring(value)
            end
        end
    elseif type(raw) == 'string' then
        -- Older mpv builds may expose a key/value list as text. Settings are
        -- written as a native key/value map so commas in SOFA paths survive.
        for entry in raw:gmatch('[^,]+') do
            local key, value = entry:match('^%s*([^=]+)%s*=%s*(.-)%s*$')
            if key and value then
                result[key] = value
            end
        end
    end
    return result
end

local function effective_options()
    local result = key_value_table(mp.get_property_native('ad-lavc-o', {}))
    local local_values = key_value_table(mp.get_property_native('file-local-options/ad-lavc-o', nil))
    for key, value in pairs(local_values) do
        result[key] = value
    end
    return result
end

local function effective_value(key, fallback)
    if draft.options[key] ~= nil then
        return draft.options[key]
    end
    local current = effective_options()[key]
    if current ~= nil then
        return current
    end
    return fallback or DEFAULTS[key]
end

local function shown_option(key, fallback, labels)
    local value = effective_value(key, fallback)
    if labels and labels[value] then
        return labels[value]
    end
    return value or 'Player default'
end

local function current_output_index()
    local mode = effective_value('render_mode', 'speaker')
    local layout = effective_value('speaker_layout', '5.1')
    for index, output in ipairs(OUTPUTS) do
        if output.render_mode == mode
            and (mode ~= 'speaker' or output.speaker_layout == layout) then
            return index
        end
    end
    return nil
end

local function current_output_label()
    local index = current_output_index()
    if index then
        return OUTPUTS[index].label
    end
    local mode = effective_value('render_mode', 'speaker')
    if mode == 'speaker' then
        return 'Other speaker target (' .. effective_value('speaker_layout', 'unknown') .. ')'
    end
    return 'Other output policy (' .. mode .. ')'
end

local function hrtf_source_index()
    local sofa = effective_value('sofa', '')
    if sofa ~= '' then
        return 3
    end
    if effective_value('hrtf', 'd1') == 'd2' then
        return 2
    end
    return 1
end

local function option_index(key, values, fallback)
    local current = effective_value(key, fallback)
    for index, value in ipairs(values) do
        if current == value then
            return index
        end
    end
    return 1
end

local draw
local change_selected
local remove_menu_keys
local install_menu_keys
local close_menu

local function show_status(message, duration)
    status_message = message
    if status_timeout then
        status_timeout:kill()
    end
    status_timeout = mp.add_timeout(duration or 4, function()
        status_timeout = nil
        status_message = nil
        draw()
    end)
    if menu_open then
        draw()
    end
end

local function reset_menu_timeout()
    if menu_timeout then
        menu_timeout:kill()
    end
    menu_timeout = mp.add_timeout(15, function()
        menu_timeout = nil
        if menu_open then
            local had_unsaved_edits = dirty
            close_menu()
            if had_unsaved_edits then
                mp.osd_message('Menu closed; draft retained. Press Ctrl+Alt+J to continue.', 4)
            end
        end
    end)
end

local function read_only_property(name, fallback)
    local value = mp.get_property(name, nil)
    if type(value) == 'string' and value ~= '' then
        return value
    end
    return fallback or 'Unavailable'
end

local function mark_changed()
    dirty = true
end

local function set_option(key, value)
    draft.options[key] = value
    mark_changed()
end

local function change_output(direction)
    local index = current_output_index()
    if index then
        index = ((index - 1 + direction) % #OUTPUTS) + 1
    else
        index = direction > 0 and 1 or #OUTPUTS
    end
    local output = OUTPUTS[index]
    draft.options.render_mode = output.render_mode
    if output.speaker_layout then
        draft.options.speaker_layout = output.speaker_layout
    end
    mark_changed()
end

local function save_state()
    local document = {
        schema = 1,
        options = draft.options,
    }
    local encoded = utils.format_json(document)
    if type(encoded) ~= 'string' then
        return false, 'could not encode settings'
    end
    local file, error_message = io.open(CONFIG_TEMP_PATH, 'wb')
    if not file then
        return false, error_message or 'settings directory is not writable'
    end
    local ok, write_error = file:write(encoded, '\n')
    local close_ok, close_error = file:close()
    if not ok or close_ok == nil then
        os.remove(CONFIG_TEMP_PATH)
        return false, write_error or close_error or 'could not write settings'
    end
    local renamed, rename_error = os.rename(CONFIG_TEMP_PATH, CONFIG_PATH)
    if not renamed then
        -- C runtimes on Windows may refuse rename-over-existing. Keep the
        -- previous file recoverable while promoting the already-written temp.
        os.remove(CONFIG_BACKUP_PATH)
        local existing = io.open(CONFIG_PATH, 'rb')
        local moved_old = false
        if existing then
            existing:close()
            moved_old, rename_error = os.rename(CONFIG_PATH, CONFIG_BACKUP_PATH)
            if not moved_old then
                os.remove(CONFIG_TEMP_PATH)
                return false, rename_error or 'could not preserve previous settings'
            end
        end
        renamed, rename_error = os.rename(CONFIG_TEMP_PATH, CONFIG_PATH)
        if not renamed then
            if moved_old then
                os.rename(CONFIG_BACKUP_PATH, CONFIG_PATH)
            end
            os.remove(CONFIG_TEMP_PATH)
            return false, rename_error or 'could not replace settings file'
        end
        if moved_old then
            os.remove(CONFIG_BACKUP_PATH)
        end
    end
    saved = copy_state(draft)
    dirty = false
    return true
end

local function label_for_hrtf()
    local source = hrtf_source_index()
    if source == 2 then
        return 'SADIE II D2 / KEMAR'
    elseif source == 3 then
        return 'Custom SOFA'
    end
    return 'SADIE II D1 / KU100'
end

local function main_rows()
    local dialnorm = effective_value('dialnorm', 'default')
    local dialnorm_label = ({
        default = 'Calibrated (recommended)',
        analog = 'Unity / Compatibility',
        digital = 'Digital (advanced)',
    })[dialnorm] or dialnorm
    local virtual = shown_option('virtual_layout', '7.1.4')
    if virtual == '9.1.6' then
        virtual = '9.1.6 (experimental)'
    end
    return {
        { title = 'Output policy', value = current_output_label(), change = change_output },
        {
            title = 'Dialnorm', value = dialnorm_label,
            change = function(direction)
                local choices = { 'default', 'analog' }
                local index
                local current = effective_value('dialnorm', 'default')
                for choice_index, choice in ipairs(choices) do
                    if choice == current then
                        index = choice_index
                        break
                    end
                end
                if not index then
                    index = direction > 0 and 1 or #choices
                end
                index = ((index - 1 + direction) % #choices) + 1
                set_option('dialnorm', choices[index])
            end,
        },
        {
            title = 'Binaural HRTF (when selected)', value = label_for_hrtf(),
            change = function(direction)
                local current = hrtf_source_index()
                local next_index = ((current - 1 + direction) % 3) + 1
                if next_index == 3 then
                    local current_path = effective_value('sofa', '')
                    if menu_open then
                        menu_open = false
                        if remove_menu_keys then
                            remove_menu_keys()
                        end
                        mp.osd_message('', 0.1)
                    end
                    input.get({
                        prompt = 'Custom SOFA file path (local file): ',
                        default_text = current_path,
                        id = 'openjoc-sofa-path',
                        closed = function()
                            -- Return to the menu for either Enter or cancel.
                            if menu_open == false then
                                menu_open = true
                                if install_menu_keys then install_menu_keys() end
                                draw()
                            end
                        end,
                        submit = function(path)
                            if type(path) ~= 'string' or path:match('^%s*$') then
                                show_status('No SOFA path entered.', 3)
                                return
                            end
                            local expanded = mp.command_native({ 'expand-path', path })
                            local normalized = expanded and mp.command_native({ 'normalize-path', expanded })
                            local info = normalized and utils.file_info(normalized)
                            if not info or not info.is_file then
                                if menu_open == false then
                                    menu_open = true
                                    if install_menu_keys then install_menu_keys() end
                                end
                                show_status('Choose an existing local SOFA file.', 3)
                                draw()
                            else
                                draft.options.sofa = normalized
                                mark_changed()
                                if menu_open == false then
                                    menu_open = true
                                    if install_menu_keys then install_menu_keys() end
                                end
                                draw()
                            end
                        end,
                    })
                elseif next_index == 1 then
                    draft.options.hrtf = 'd1'
                    draft.options.sofa = ''
                    mark_changed()
                else
                    draft.options.hrtf = 'd2'
                    draft.options.sofa = ''
                    mark_changed()
                end
            end,
        },
        {
            title = 'Binaural virtual layout (when selected)', value = virtual,
            change = function(direction)
                local choices = { '7.1.4', '9.1.6' }
                local index = option_index('virtual_layout', choices, '7.1.4')
                index = ((index - 1 + direction) % #choices) + 1
                set_option('virtual_layout', choices[index])
            end,
        },
        { title = 'Active decoder (read-only)', value = read_only_property('current-tracks/audio/decoder') },
        { title = 'Decoder output channels (read-only)', value =
            read_only_property('audio-params/hr-channels', read_only_property('audio-params/channels')) },
        { title = 'Audio output channels (read-only)', value =
            read_only_property('audio-out-params/hr-channels', read_only_property('audio-out-params/channels')) },
        { title = 'Save selection for next OpenJOC file', value = dirty and 'Unsaved changes' or 'No unsaved changes', action = function()
            local ok, err = save_state()
            if not ok then
                show_status('Settings not saved: ' .. tostring(err), 4)
                return
            end
            local path = mp.get_property('path', '') or ''
            if path ~= '' then
                show_status('Selection saved. Current decoder is unchanged; applies when the next file opens.', 4)
            else
                show_status('Selection saved. Applies when the next file opens.', 4)
            end
        end },
        { title = 'Discard edits and close', value = '', action = function()
            draft = copy_state(saved)
            dirty = false
            close_menu()
        end },
    }
end

draw = function()
    if not menu_open then
        return
    end
    local rows = main_rows()
    if row > #rows then
        row = 1
    elseif row < 1 then
        row = #rows
    end
    local lines = {
        'OpenJOC settings' .. (dirty and '  • unsaved' or ''),
        'Up/Down: choose   Left/Right: change   Enter: select   Esc: close',
    }
    for index, item in ipairs(rows) do
        local marker = index == row and '> ' or '  '
        lines[#lines + 1] = marker .. item.title .. (item.value ~= '' and (': ' .. item.value) or '')
    end
    lines[#lines + 1] = 'Settings are for the next file; status rows above are live mpv properties, not JOC diagnostics.'
    lines[#lines + 1] = 'Best-effort E-AC-3 gate; settings may reach other tracks and log warnings.'
    lines[#lines + 1] = 'mpv keeps its normal audio output mapping; it may adapt channels to the AO.'
    lines[#lines + 1] = 'LAV output gain / live JOC page are unavailable; mpv volume is separate.'
    if status_message then
        lines[#lines + 1] = 'Status: ' .. status_message
    end
    mp.osd_message(table.concat(lines, '\n'), 15)
    reset_menu_timeout()
end

change_selected = function(direction)
    local rows = main_rows()
    local item = rows[row]
    if not item then
        return
    end
    if item.change then
        item.change(direction)
    elseif item.action then
        item.action()
    end
    draw()
end

close_menu = function()
    menu_open = false
    if menu_timeout then
        menu_timeout:kill()
        menu_timeout = nil
    end
    if status_timeout then
        status_timeout:kill()
        status_timeout = nil
    end
    status_message = nil
    if remove_menu_keys then remove_menu_keys() end
    mp.osd_message('', 0.1)
end

remove_menu_keys = function()
    for _, name in ipairs({ 'up', 'down', 'left', 'right', 'enter', 'escape' }) do
        mp.remove_key_binding('openjoc-settings-' .. name)
    end
end

install_menu_keys = function()
    mp.add_forced_key_binding('UP', 'openjoc-settings-up', function()
        row = row - 1
        draw()
    end, { repeatable = true })
    mp.add_forced_key_binding('DOWN', 'openjoc-settings-down', function()
        row = row + 1
        draw()
    end, { repeatable = true })
    mp.add_forced_key_binding('LEFT', 'openjoc-settings-left', function()
        change_selected(-1)
    end, { repeatable = true })
    mp.add_forced_key_binding('RIGHT', 'openjoc-settings-right', function()
        change_selected(1)
    end, { repeatable = true })
    mp.add_forced_key_binding('ENTER', 'openjoc-settings-enter', function()
        change_selected(1)
    end)
    mp.add_forced_key_binding('ESC', 'openjoc-settings-escape', function()
        if dirty then
            show_status('Unsaved edits. Choose Save or Discard before closing.', 3)
        else
            close_menu()
        end
    end)
end

local function toggle_menu()
    if menu_open then
        if dirty then
            show_status('Unsaved edits. Choose Save or Discard before closing.', 3)
            return
        end
        close_menu()
        return
    end
    if not dirty then
        draft = copy_state(saved)
        row = 1
    end
    menu_open = true
    install_menu_keys()
    draw()
end

local function apply_saved_settings()
    local tracks = mp.get_property_native('track-list', {}) or {}
    local has_eac3_audio = false
    for _, track in ipairs(tracks) do
        if track.type == 'audio' and track.codec == 'eac3' then
            has_eac3_audio = true
            break
        end
    end
    if not has_eac3_audio or next(saved.options) == nil then
        return
    end

    local options = effective_options()
    for key, value in pairs(saved.options) do
        -- Merge the user's existing key/value list, replacing only selected
        -- OpenJOC keys, then set it through mpv's per-file native map API.
        options[key] = value
    end
    local ok, result, err = pcall(mp.set_property_native,
        'file-local-options/ad-lavc-o', options)
    if not ok or not result then
        mp.msg.error('OpenJOC could not apply the saved per-file decoder options: '
            .. tostring(err or result))
    else
        mp.msg.info('OpenJOC saved decoder options applied for an E-AC-3 file')
    end
end

mp.add_hook('on_preloaded', 50, apply_saved_settings)
mp.add_key_binding('Ctrl+Alt+j', 'openjoc-settings-toggle', toggle_menu)
for _, property in ipairs({
    'current-tracks/audio/decoder',
    'audio-params/hr-channels',
    'audio-out-params/hr-channels',
}) do
    mp.observe_property(property, 'string', function()
        if menu_open then draw() end
    end)
end
mp.msg.info('OpenJOC settings menu loaded (Ctrl+Alt+J)')

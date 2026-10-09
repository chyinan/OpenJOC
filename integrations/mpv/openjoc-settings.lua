-- SPDX-FileCopyrightText: 2026 OpenJOC contributors
-- SPDX-License-Identifier: Apache-2.0

-- Small OpenJOC settings menu for the project-provided mpv bundle. Decoder
-- AVOptions are fixed when libopenjoc is initialized, so ordinary edits are
-- saved for the next file. Apply Current explicitly updates this file-local
-- decoder map and lets the pinned mpv rebuild the audio chain; it never seeks
-- or reloads the file. Output gain is a separate live post-render AF stage.

local utils = require 'mp.utils'
local input = require 'mp.input'

local CONFIG_PATH = mp.command_native({ 'expand-path', '~~/openjoc-settings.json' })
local CONFIG_TEMP_PATH = CONFIG_PATH .. '.tmp'
local CONFIG_BACKUP_PATH = CONFIG_PATH .. '.bak'
-- The bundled launcher now uses bin/portable_config as mpv's config dir.
-- Read the former root/config location as a fallback, but leave it in place;
-- future explicit saves always target CONFIG_PATH above.
local LEGACY_CONFIG_PATH = mp.command_native({
    'expand-path', '~~/../../config/openjoc-settings.json',
})

-- Lua's standard io/os file functions pass narrow paths to the Windows CRT.
-- mpv's Lua 5.1 host opens the standard libraries unchanged, so UTF-8 paths
-- (including a portable bundle installed below a non-ASCII user directory)
-- need Win32's wide-character file APIs. Keep this adapter in the same file:
-- mpv does not add the directory of a single-file script to package.path.
local FILE_IO_MODULE = 'openjoc.settings_file_io.v1'
local MAX_SETTINGS_BYTES = 65536

local function get_settings_file_io()
    local cached = package.loaded[FILE_IO_MODULE]
    if type(cached) == 'table' then return cached end

    local file_io = {}
    local is_windows = type(package.config) == 'string'
        and package.config:sub(1, 1) == '\\'
    local ffi, win32, ffi_error

    if is_windows then
        local ok, result = pcall(require, 'ffi')
        if ok then
            ffi = result
            local cdef_ok, cdef_error = pcall(ffi.cdef, [[
                typedef unsigned short oj_wchar;
                typedef unsigned long oj_dword;
                typedef unsigned int oj_uint;
                typedef void *oj_handle;
                typedef const unsigned short *oj_lpcwstr;
                typedef unsigned short *oj_lpwstr;
                typedef const char *oj_lpcch;
                typedef const void *oj_lpcvoid;
                typedef void *oj_lpvoid;
                oj_handle __stdcall CreateFileW(oj_lpcwstr, oj_dword, oj_dword,
                    void *, oj_dword, oj_dword, oj_handle);
                int __stdcall ReadFile(oj_handle, oj_lpvoid, oj_dword,
                    oj_dword *, void *);
                int __stdcall WriteFile(oj_handle, oj_lpcvoid, oj_dword,
                    oj_dword *, void *);
                int __stdcall CloseHandle(oj_handle);
                int __stdcall MoveFileW(oj_lpcwstr, oj_lpcwstr);
                int __stdcall DeleteFileW(oj_lpcwstr);
                oj_dword __stdcall GetLastError(void);
                int __stdcall MultiByteToWideChar(oj_uint, oj_dword, oj_lpcch,
                    int, oj_lpwstr, int);
            ]])
            if cdef_ok then
                local load_ok, library = pcall(ffi.load, 'kernel32')
                if load_ok then
                    win32 = library
                else
                    ffi_error = tostring(library)
                end
            else
                ffi_error = tostring(cdef_error)
            end
        else
            ffi_error = tostring(result)
        end
    end

    local function has_non_ascii(path)
        return path:find('[\\128-\\255]') ~= nil
    end

    local function use_wide_api(...)
        if not is_windows then return false end
        if win32 then return true end
        for index = 1, select('#', ...) do
            if has_non_ascii(select(index, ...)) then
                local detail = ffi_error and (': ' .. ffi_error) or ''
                return nil, 'Unicode settings paths need LuaJIT FFI support' .. detail
            end
        end
        return false
    end

    local function win_error(operation)
        local code = tonumber(win32.GetLastError())
        return operation .. ' failed (Windows error ' .. tostring(code) .. ')', code
    end

    local function missing_error(error_message)
        local message = tostring(error_message or ''):lower()
        return message:find('no such file', 1, true) ~= nil
            or message:find('file not found', 1, true) ~= nil
            or message:find('path not found', 1, true) ~= nil
            or message:find('not found', 1, true) ~= nil
            or message:find('cannot find the file', 1, true) ~= nil
            or message:find('cannot find the path', 1, true) ~= nil
    end

    local function to_wide(path)
        if path == '' or path:find('\0', 1, true) then
            return nil, 'invalid settings file path'
        end
        -- Explicitly allocate and NUL-terminate the UTF-8 input, then ask
        -- Windows to reject malformed UTF-8 rather than silently replace it.
        local input_bytes = ffi.new('char[?]', #path + 1)
        ffi.copy(input_bytes, path, #path)
        local needed = win32.MultiByteToWideChar(65001, 8, input_bytes, #path, nil, 0)
        if needed <= 0 then
            local message = win_error('UTF-8 path conversion')
            return nil, message
        end
        local output = ffi.new('oj_wchar[?]', needed + 1)
        local converted = win32.MultiByteToWideChar(65001, 8, input_bytes,
            #path, output, needed)
        if converted ~= needed then
            local message = win_error('UTF-8 path conversion')
            return nil, message
        end
        output[needed] = 0
        return output
    end

    local function win_open(path, access, creation)
        local wide, err = to_wide(path)
        if not wide then return nil, err end
        local handle = win32.CreateFileW(wide, access, 7, nil, creation, 128, nil)
        if handle == ffi.cast('oj_handle', -1) or handle == ffi.NULL then
            local message, code = win_error('Opening settings file')
            return nil, message, code == 2 or code == 3
        end
        return handle
    end

    local function win_read(path)
        local handle, err, missing = win_open(path, 0x80000000, 3) -- GENERIC_READ, OPEN_EXISTING
        if not handle then return nil, err, missing end
        local buffer = ffi.new('char[16384]')
        local count = ffi.new('oj_dword[1]')
        local chunks, total = {}, 0
        while true do
            if win32.ReadFile(handle, buffer, 16384, count, nil) == 0 then
                local read_error = win_error('Reading settings file')
                win32.CloseHandle(handle)
                return nil, read_error, false
            end
            local size = tonumber(count[0])
            if size == 0 then break end
            total = total + size
            if total > MAX_SETTINGS_BYTES then
                win32.CloseHandle(handle)
                return nil, 'settings file exceeds 64 KiB', false
            end
            chunks[#chunks + 1] = ffi.string(buffer, size)
        end
        if win32.CloseHandle(handle) == 0 then
            local close_error = win_error('Closing settings file')
            return nil, close_error, false
        end
        return table.concat(chunks)
    end

    local function win_write(path, contents)
        local handle, err = win_open(path, 0x40000000, 2) -- GENERIC_WRITE, CREATE_ALWAYS
        if not handle then return nil, err end
        local bytes = ffi.new('char[?]', math.max(#contents, 1))
        if #contents > 0 then ffi.copy(bytes, contents, #contents) end
        local pointer = ffi.cast('const char *', bytes)
        local offset = 0
        local count = ffi.new('oj_dword[1]')
        while offset < #contents do
            local requested = math.min(#contents - offset, 2147483647)
            if win32.WriteFile(handle, pointer + offset, requested, count, nil) == 0 then
                local write_error = win_error('Writing settings file')
                win32.CloseHandle(handle)
                return nil, write_error
            end
            local size = tonumber(count[0])
            if size <= 0 or size > requested then
                win32.CloseHandle(handle)
                return nil, 'could not complete settings file write'
            end
            offset = offset + size
        end
        if win32.CloseHandle(handle) == 0 then
            return nil, win_error('Closing settings file')
        end
        return true
    end

    local function read(path)
        local wide, err = use_wide_api(path)
        if wide == nil then return nil, err, false end
        if wide then return win_read(path) end
        local file, open_error = io.open(path, 'rb')
        if not file then return nil, open_error, missing_error(open_error) end
        local contents, read_error = file:read('*a')
        local close_ok, close_error = file:close()
        if not contents then return nil, read_error, false end
        if close_ok == nil then return nil, close_error, false end
        if #contents > MAX_SETTINGS_BYTES then
            return nil, 'settings file exceeds 64 KiB', false
        end
        return contents
    end

    local function write(path, contents)
        local wide, err = use_wide_api(path)
        if wide == nil then return nil, err end
        if wide then return win_write(path, contents) end
        local file, open_error = io.open(path, 'wb')
        if not file then return nil, open_error end
        local ok, write_error = file:write(contents)
        local close_ok, close_error = file:close()
        if not ok or close_ok == nil then
            return nil, write_error or close_error or 'could not write settings'
        end
        return true
    end

    local function move(old_path, new_path)
        local wide, err = use_wide_api(old_path, new_path)
        if wide == nil then return nil, err end
        if wide then
            local old_wide, old_error = to_wide(old_path)
            if not old_wide then return nil, old_error end
            local new_wide, new_error = to_wide(new_path)
            if not new_wide then return nil, new_error end
            -- MoveFileW is same-volume, atomic, and deliberately does not
            -- replace an existing destination, matching os.rename fallback.
            if win32.MoveFileW(old_wide, new_wide) == 0 then
                return nil, win_error('Moving settings file')
            end
            return true
        end
        return os.rename(old_path, new_path)
    end

    local function remove(path)
        local wide, err = use_wide_api(path)
        if wide == nil then return nil, err end
        if wide then
            local wide_path, path_error = to_wide(path)
            if not wide_path then return nil, path_error end
            if win32.DeleteFileW(wide_path) == 0 then
                return nil, win_error('Removing settings file')
            end
            return true
        end
        return os.remove(path)
    end

    file_io.read = read
    file_io.write = write
    file_io.move = move
    file_io.remove = remove
    package.loaded[FILE_IO_MODULE] = file_io
    return file_io
end

local settings_file_io = get_settings_file_io()

local DEFAULTS = {
    render_mode = 'speaker',
    speaker_layout = '5.1',
    virtual_layout = '7.1.4',
    hrtf = 'd1',
    sofa = '',
    dialnorm = 'default',
    output_gain_tenths_db = 0,
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
        elseif key == 'output_gain_tenths_db' and type(item) == 'number'
            and item == math.floor(item) and item >= -200 and item <= 200 then
            safe.options.output_gain_tenths_db = item
        elseif key == 'sofa' and type(item) == 'string' and #item <= 4096
            and not item:find('[\0\r\n]') then
            safe.options.sofa = item
        end
    end
    return safe
end

local state_load_error
local state_save_blocked = false

local function parse_state_contents(contents, source, block_save)
    local parsed = utils.parse_json(contents)
    if type(parsed) ~= 'table' or parsed.schema ~= 1 or type(parsed.options) ~= 'table' then
        state_load_error = source .. ' is invalid or uses an unsupported schema.'
        state_save_blocked = state_save_blocked or block_save
        mp.msg.error('OpenJOC settings: ' .. state_load_error)
        return nil
    end
    return sanitize_state(parsed)
end

local function load_state()
    local contents, read_error, missing = settings_file_io.read(CONFIG_PATH)
    if contents then
        return parse_state_contents(contents, 'Saved settings', true) or empty_state()
    end
    if not missing then
        state_load_error = 'Saved settings could not be read: ' .. tostring(read_error)
        state_save_blocked = true
        mp.msg.error('OpenJOC settings: ' .. state_load_error)
        return empty_state()
    end

    -- Recover the previous settings if Windows stopped between moving the
    -- old file aside and installing the newly written temporary file.
    local backup_contents, backup_error, backup_missing = settings_file_io.read(CONFIG_BACKUP_PATH)
    if backup_contents then
        local backup_state = parse_state_contents(backup_contents, 'Settings recovery backup', true)
        if not backup_state then return empty_state() end
        local restored, restore_error = settings_file_io.move(CONFIG_BACKUP_PATH, CONFIG_PATH)
        if not restored then
            state_load_error = 'Recovered saved settings from the backup; it could not be restored: '
                .. tostring(restore_error)
            state_save_blocked = true
            mp.msg.error('OpenJOC settings: ' .. state_load_error)
        end
        return backup_state
    end
    if not backup_missing then
        state_load_error = 'Settings recovery backup could not be read: ' .. tostring(backup_error)
        state_save_blocked = true
        mp.msg.error('OpenJOC settings: ' .. state_load_error)
        return empty_state()
    end

    -- Portable-config migration is read-only. Do not rename, delete, or
    -- rewrite the legacy file; the next explicit Save creates the new one.
    local legacy_contents, legacy_error, legacy_missing = settings_file_io.read(LEGACY_CONFIG_PATH)
    if legacy_contents then
        return parse_state_contents(legacy_contents, 'Older saved settings', true) or empty_state()
    end
    if not legacy_missing then
        state_load_error = 'Older saved settings could not be read; the original file was left untouched: '
            .. tostring(legacy_error)
        state_save_blocked = true
        mp.msg.error('OpenJOC settings: ' .. state_load_error)
    end
    return empty_state()
end

local saved = load_state()
local draft = copy_state(saved)
local dirty = false
local row = 1
local menu_open = false
local sofa_input_open = false
local menu_timeout
local status_timeout
local status_message
local active_gain_tenths_db = saved.options.output_gain_tenths_db or 0
local overlay = mp.create_osd_overlay('ass-events')
overlay.z = 50

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
local open_sofa_prompt
local reconcile_live_gain
local apply_current_playback
local mark_changed

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

-- mpv's commandv returns true on success and nil, error on failure. Keep this
-- strict wrapper for AF changes: Lua truthiness would incorrectly accept nil.
local function run_mpv_command(...)
    if type(mp.commandv) ~= 'function' then
        return nil, 'mp.commandv is unavailable in this mpv build'
    end
    local ok, result, detail = pcall(mp.commandv, ...)
    if not ok then return nil, tostring(result) end
    if result ~= true then
        return nil, tostring(detail or ('mpv command returned ' .. tostring(result)))
    end
    return true
end

local GAIN_FILTER_LABEL = 'openjoc_gain'
local GAIN_RETRY_DELAYS = { 0.05, 0.10, 0.15, 0.25, 0.40, 0.50 }
local GAIN_MAX_TENTHS_DB = 200
local gain_epoch = 0
local gain_retry_timer
local gain_cleanup_timer
local apply_current_timer
local runtime_reconcile_timer
local gain_pending
local gain_add_pending = false
local gain_add_cleanup_pending = false
local gain_add_cleanup_attempt = 0
local gain_cleanup_in_progress = false
local gain_filter_settling = false
local gain_runtime_error
local gain_cleanup_warning
local gain_preview_touched = false
local gain_restore_owed = false
local gain_restore_value
local runtime_signature
local runtime_apply_token = 0
local pending_apply

local function clamp_gain(value)
    if type(value) ~= 'number' then return 0 end
    value = value >= 0 and math.floor(value + 0.5) or math.ceil(value - 0.5)
    return math.max(-GAIN_MAX_TENTHS_DB, math.min(GAIN_MAX_TENTHS_DB, value))
end

local function gain_factor(value)
    value = clamp_gain(value)
    if value == 0 then return '1' end
    return string.format('%.17g', math.pow(10.0, value / 200.0))
end

local function gain_label(value)
    value = clamp_gain(value)
    local sign = value > 0 and '+' or (value < 0 and '−' or '')
    local absolute = math.abs(value)
    return sign .. tostring(math.floor(absolute / 10)) .. '.' .. tostring(absolute % 10) .. ' dB'
end

local function kill_timer(timer)
    if timer and type(timer.kill) == 'function' then timer:kill() end
end

local function cancel_gain_retry()
    kill_timer(gain_retry_timer)
    gain_retry_timer = nil
end

local function selected_audio_track()
    local tracks = mp.get_property_native('track-list', {}) or {}
    for _, track in ipairs(tracks) do
        if track.type == 'audio' and track.selected == true then return track end
    end
    return nil
end

local function openjoc_track()
    local decoder = mp.get_property('current-tracks/audio/decoder', '')
    local track = selected_audio_track()
    local codec = type(track) == 'table' and tostring(track.codec or ''):lower() or ''
    if decoder ~= 'libopenjoc' or not track
        or (codec ~= 'eac3' and codec ~= 'e-ac-3') then
        return false, decoder, track
    end
    return true, decoder, track
end

local function current_decoder_options()
    local options = key_value_table(mp.get_property_native('ad-lavc-o', {}))
    local local_options = key_value_table(mp.get_property_native('file-local-options/ad-lavc-o', nil))
    for key, value in pairs(local_options) do options[key] = value end
    return options
end

local function current_runtime_signature()
    local _, decoder, track = openjoc_track()
    local path = tostring(mp.get_property('path', '') or '')
    local track_id = type(track) == 'table' and tostring(track.id or '') or ''
    local track_codec = type(track) == 'table' and tostring(track.codec or '') or ''
    local options = current_decoder_options()
    local mode = table.concat({
        options.render_mode or '', options.speaker_layout or '',
        options.hrtf or '', options.sofa or '', options.virtual_layout or '',
    }, '\30')
    return table.concat({ path, track_id, track_codec, tostring(decoder or ''), mode }, '\31')
end

local function named_gain_filter()
    local filters = mp.get_property_native('af', {})
    if type(filters) ~= 'table' then return nil end
    for _, filter in ipairs(filters) do
        if type(filter) == 'table' and filter.name == 'lavfi'
            and filter.label == GAIN_FILTER_LABEL then
            return filter
        end
    end
    return nil
end

local function filter_gain_value(filter)
    local graph = type(filter) == 'table' and type(filter.params) == 'table'
        and filter.params.graph or nil
    if type(graph) ~= 'string' then return nil end
    local volume = graph:match('volume@openjoc_gain=volume=([^:]+)')
    if not volume then return nil end
    for value = -GAIN_MAX_TENTHS_DB, GAIN_MAX_TENTHS_DB do
        if gain_factor(value) == volume then return value end
    end
    local numeric = tonumber(volume)
    if numeric and numeric > 0 then
        local db = 200 * (math.log(numeric) / math.log(10))
        return clamp_gain(db)
    end
    return nil
end

local function set_runtime_error(message)
    gain_runtime_error = tostring(message)
    if menu_open then draw() end
end

local function clear_runtime_error()
    gain_runtime_error = nil
    if menu_open then draw() end
end

local function set_cleanup_warning(message)
    gain_cleanup_warning = tostring(message)
    if menu_open then
        draw()
    elseif type(mp.osd_message) == 'function' then
        mp.osd_message('OpenJOC audio cleanup warning: ' .. gain_cleanup_warning, 5)
    end
end

local function clear_cleanup_warning()
    gain_cleanup_warning = nil
    if menu_open then draw() end
end

local function add_gain_filter(initial_value)
    local factor = gain_factor(initial_value)
    local graph = 'volume@openjoc_gain=volume=' .. factor .. ':precision=float'
    local spec = '@openjoc_gain:lavfi=[' .. graph .. ']'
    local ok, err = run_mpv_command('af', 'add', spec)
    if not ok then
        return nil, 'could not add the live OpenJOC gain filter: ' .. tostring(err)
    end
    local filter = named_gain_filter()
    if filter then
        gain_add_pending = false
        gain_filter_settling = true
        active_gain_tenths_db = clamp_gain(initial_value)
        return true
    end
    gain_add_pending = true
    gain_filter_settling = true
    return true, 'pending'
end

local function filter_exists()
    return named_gain_filter() ~= nil
end

local function clear_preview_restoration_if_current(value)
    if gain_restore_owed and gain_restore_value ~= nil
        and clamp_gain(value) == clamp_gain(gain_restore_value) then
        gain_restore_owed = false
        gain_preview_touched = false
        gain_restore_value = nil
    end
end

local function finish_gain_update(value)
    active_gain_tenths_db = clamp_gain(value)
    gain_pending = nil
    gain_add_pending = false
    gain_filter_settling = false
    cancel_gain_retry()
    clear_runtime_error()
    clear_cleanup_warning()
    clear_preview_restoration_if_current(value)
    if menu_open then draw() end
end

local function fail_gain_update(message)
    gain_pending = nil
    cancel_gain_retry()
    set_runtime_error(message)
end

local function gain_retry_is_current(pending)
    if pending.epoch ~= gain_epoch or pending.signature ~= current_runtime_signature() then return false end
    local active = openjoc_track()
    return active
end

local function schedule_gain_retry(pending, delay)
    if mp.get_property_native('pause', false) == true then
        -- Pause time is excluded from the bounded filter-init deadline. Keep
        -- only the latest requested value and resume retries on pause=false.
        gain_pending = pending
        cancel_gain_retry()
        return
    end
    cancel_gain_retry()
    gain_pending = pending
    gain_retry_timer = mp.add_timeout(delay, function()
        gain_retry_timer = nil
        if gain_pending ~= pending or not gain_retry_is_current(pending) then
            if gain_pending == pending then gain_pending = nil end
            return
        end
        if mp.get_property_native('pause', false) == true then return end

        local filter = named_gain_filter()
        if not filter then
            if pending.wait_for_add then
                pending.wait_for_add = true
            else
                local added, add_state = add_gain_filter(pending.value)
                if not added then
                    fail_gain_update(add_state)
                    return
                end
                if add_state ~= 'pending' then
                    finish_gain_update(pending.value)
                    return
                end
                pending.wait_for_add = true
            end
        else
            pending.wait_for_add = false
            local filter_value = filter_gain_value(filter)
            if filter_value == pending.value and not gain_filter_settling then
                finish_gain_update(pending.value)
                return
            end
            local ok, err = run_mpv_command('af-command', GAIN_FILTER_LABEL,
                'volume', gain_factor(pending.value), 'volume')
            if ok then
                finish_gain_update(pending.value)
                return
            end
            pending.last_error = err
        end

        pending.attempt = pending.attempt + 1
        local next_delay = GAIN_RETRY_DELAYS[pending.attempt]
        if not next_delay then
            fail_gain_update('Live gain could not be confirmed after bounded retries: '
                .. tostring(pending.last_error or 'the named filter did not become ready'))
            return
        end
        schedule_gain_retry(pending, next_delay)
    end)
end

local function request_runtime_gain(value, reason)
    value = clamp_gain(value)
    local active, decoder = openjoc_track()
    if not active then
        gain_pending = nil
        cancel_gain_retry()
        if reason == 'preview' then
            gain_preview_touched = false
            gain_restore_owed = false
            gain_restore_value = nil
        end
        set_runtime_error('Live preview needs an active libopenjoc E-AC-3 track; this value is saved for a later OpenJOC file.')
        return nil, 'live OpenJOC decoder is unavailable (' .. tostring(decoder) .. ')'
    end

    if reason == 'preview' and not gain_preview_touched then
        gain_restore_value = active_gain_tenths_db
        gain_preview_touched = true
        gain_restore_owed = true
    elseif reason == 'restore' then
        gain_restore_owed = true
        if gain_restore_value == nil then gain_restore_value = value end
    end

    gain_epoch = gain_epoch + 1
    cancel_gain_retry()
    local pending = {
        epoch = gain_epoch,
        signature = current_runtime_signature(),
        value = value,
        reason = reason,
        attempt = 0,
        wait_for_add = gain_add_pending,
    }
    gain_pending = pending

    local filter = named_gain_filter()
    if not filter then
        if gain_add_pending then
            -- The `af add` command succeeded, but mpv may not expose the
            -- constructed filter until a later AF update. Coalesce newer
            -- slider/Cancel requests onto that in-flight insertion rather
            -- than adding a duplicate named filter.
            pending.wait_for_add = true
            schedule_gain_retry(pending, GAIN_RETRY_DELAYS[1])
            return nil, 'gain filter insertion is pending'
        end
        local added, add_state = add_gain_filter(value)
        if not added then
            fail_gain_update(add_state)
            return nil, add_state
        end
        if add_state ~= 'pending' then
            if not gain_filter_settling then
                finish_gain_update(value)
                return true
            end
            schedule_gain_retry(pending, GAIN_RETRY_DELAYS[1])
            return nil, 'gain filter is initializing'
        end
        pending.wait_for_add = true
        schedule_gain_retry(pending, GAIN_RETRY_DELAYS[1])
        return nil, 'gain filter insertion is pending'
    end

    local filter_value = filter_gain_value(filter)
    if filter_value == value and not gain_filter_settling and not gain_add_pending then
        finish_gain_update(value)
        return true
    end
    if mp.get_property_native('pause', false) == true then
        pending.attempt = 0
        schedule_gain_retry(pending, GAIN_RETRY_DELAYS[1])
        return nil, 'gain update queued until playback resumes'
    end
    if gain_filter_settling then
        -- The AF chain needs a short initialization window after add. Use a
        -- bounded retry queue instead of issuing a same-tick command.
        schedule_gain_retry(pending, 0.35)
        return nil, 'gain filter is initializing'
    end
    local ok, err = run_mpv_command('af-command', GAIN_FILTER_LABEL,
        'volume', gain_factor(value), 'volume')
    if ok then
        finish_gain_update(value)
        return true
    end
    pending.last_error = err
    schedule_gain_retry(pending, GAIN_RETRY_DELAYS[1])
    return nil, err
end

local remove_gain_filter_for_non_openjoc

local function schedule_pending_add_cleanup()
    if not gain_add_cleanup_pending or gain_cleanup_timer then return end
    local delays = { 0.15, 0.3, 0.6, 0.9 }
    gain_add_cleanup_attempt = gain_add_cleanup_attempt + 1
    local delay = delays[gain_add_cleanup_attempt]
    if not delay then
        set_cleanup_warning('A pending OpenJOC gain-filter insertion could not be confirmed or removed; cleanup will retry if mpv reports the named filter.')
        return
    end
    gain_cleanup_timer = mp.add_timeout(delay, function()
        gain_cleanup_timer = nil
        if not gain_add_cleanup_pending then return end
        if openjoc_track() then
            -- The selected track is OpenJOC again. Retain the pending add and
            -- let the ordinary reconciliation coalesce the saved target.
            gain_add_cleanup_pending = false
            gain_add_cleanup_attempt = 0
            clear_cleanup_warning()
            reconcile_live_gain('pending-af-add-on-openjoc')
        elseif filter_exists() then
            gain_add_cleanup_pending = false
            gain_add_cleanup_attempt = 0
            remove_gain_filter_for_non_openjoc()
        else
            schedule_pending_add_cleanup()
        end
    end)
end

remove_gain_filter_for_non_openjoc = function()
    cancel_gain_retry()
    gain_epoch = gain_epoch + 1
    gain_pending = nil
    local filter = named_gain_filter()
    if not filter then
        if gain_add_pending then
            -- `af add` succeeded but its named filter has not surfaced yet.
            -- Keep a tombstone until visibility lets us remove only our label.
            gain_preview_touched = false
            gain_restore_owed = false
            gain_restore_value = nil
            kill_timer(gain_cleanup_timer)
            gain_cleanup_timer = nil
            gain_add_cleanup_pending = true
            set_cleanup_warning('A pending OpenJOC gain filter is awaiting removal from the current non-OpenJOC track.')
            schedule_pending_add_cleanup()
            return nil, 'gain filter insertion is still pending'
        end
        gain_add_pending = false
        gain_add_cleanup_pending = false
        gain_add_cleanup_attempt = 0
        gain_filter_settling = false
        clear_cleanup_warning()
        gain_preview_touched = false
        gain_restore_owed = false
        gain_restore_value = nil
        return true
    end
    if gain_add_cleanup_pending then
        -- Transfer ownership away from the pending-add retry before starting
        -- the ordinary remove/retry path. Otherwise the stale tombstone timer
        -- can block scheduling a retry after an injected/transient remove error.
        kill_timer(gain_cleanup_timer)
        gain_cleanup_timer = nil
    end
    gain_add_pending = false
    gain_add_cleanup_pending = false
    gain_add_cleanup_attempt = 0
    local removed, remove_error = run_mpv_command('af', 'remove', '@openjoc_gain')
    if removed and not filter_exists() then
        gain_filter_settling = false
        clear_cleanup_warning()
        gain_preview_touched = false
        gain_restore_owed = false
        gain_restore_value = nil
        return true
    end

    -- If mpv refuses to remove the filter during a decoder handoff, neutralize
    -- our own post-render stage before retrying. Never touch unrelated filters.
    local neutralized, neutral_error = run_mpv_command('af-command', GAIN_FILTER_LABEL,
        'volume', '1', 'volume')
    if neutralized then active_gain_tenths_db = 0 end
    set_cleanup_warning((neutralized
        and 'Gain cleanup failed; unity fallback is active while removal retries.'
        or 'Gain cleanup failed; unity fallback also failed; removal retries continue.')
        .. ' Remove: ' .. tostring(remove_error)
        .. (neutral_error and ('; unity: ' .. tostring(neutral_error)) or ''))
    if not gain_cleanup_timer then
        local cleanup_attempt = 0
        local function retry_cleanup()
            gain_cleanup_timer = nil
            local active = openjoc_track()
            if active then return end -- new valid OpenJOC playback owns its stage
            local current = named_gain_filter()
            if not current then
                clear_cleanup_warning()
                gain_filter_settling = false
                gain_preview_touched = false
                gain_restore_owed = false
                gain_restore_value = nil
                return
            end
            local ok = run_mpv_command('af', 'remove', '@openjoc_gain')
            if ok and not filter_exists() then
                clear_cleanup_warning()
                gain_filter_settling = false
                gain_preview_touched = false
                gain_restore_owed = false
                gain_restore_value = nil
                return
            end
            cleanup_attempt = cleanup_attempt + 1
            local delay = ({ 0.15, 0.3, 0.6, 0.9 })[cleanup_attempt]
            if delay then
                gain_cleanup_timer = mp.add_timeout(delay, retry_cleanup)
            else
                set_cleanup_warning('The named OpenJOC gain filter could not be removed after bounded retries; playback may still be affected.')
            end
        end
        gain_cleanup_timer = mp.add_timeout(0.15, retry_cleanup)
    end
    return nil, remove_error
end

local function ensure_saved_gain_filter()
    local active = openjoc_track()
    if not active then return remove_gain_filter_for_non_openjoc() end
    local wanted = saved.options.output_gain_tenths_db or 0
    if not filter_exists() then
        if gain_add_pending then
            -- Reuse a filter insertion that is still crossing the AF update
            -- boundary instead of issuing another `af add`.
            request_runtime_gain(wanted, 'startup')
            return true
        end
        local ok, state = add_gain_filter(wanted)
        if not ok then
            set_runtime_error(state)
            return nil, state
        end
        if state == 'pending' or gain_filter_settling then
            local pending = {
                epoch = gain_epoch, signature = current_runtime_signature(), value = wanted,
                reason = 'startup', attempt = 0, wait_for_add = (state == 'pending'),
            }
            gain_pending = pending
            schedule_gain_retry(pending, GAIN_RETRY_DELAYS[1])
        else
            clear_runtime_error()
        end
        return true
    end
    local current = filter_gain_value(named_gain_filter())
    if current ~= nil then active_gain_tenths_db = current end
    if current ~= wanted then
        request_runtime_gain(wanted, 'startup')
    else
        clear_runtime_error()
    end
    return true
end

local function decoder_options_for_state(state)
    local options = current_decoder_options()
    for key, value in pairs(state.options or {}) do
        if key ~= 'output_gain_tenths_db' then options[key] = value end
    end
    return options
end

local function options_match(expected)
    local actual = key_value_table(mp.get_property_native('file-local-options/ad-lavc-o', nil))
    for key, value in pairs(expected) do
        if actual[key] ~= tostring(value) then return false end
    end
    return true
end

local function apply_current_is_current(pending)
    if not pending or pending.token ~= runtime_apply_token then return false end
    if tostring(mp.get_property('path', '') or '') ~= pending.path then return false end
    local active, _, track = openjoc_track()
    if not active or not track or tostring(track.id or '') ~= pending.track_id then return false end
    return true
end

local function finish_apply_current(pending)
    if pending_apply ~= pending or not apply_current_is_current(pending)
        or not options_match(pending.expected) then
        return false
    end
    pending_apply = nil
    kill_timer(apply_current_timer)
    apply_current_timer = nil
    runtime_signature = current_runtime_signature()
    local ok, err = request_runtime_gain(pending.gain, 'apply-current')
    if ok then
        show_status('Applied to current playback. A brief audio gap may occur.', 5)
    elseif gain_pending then
        show_status('Applied to current playback; live gain restoration is queued.', 5)
    else
        show_status('Decoder settings applied, but live gain is unavailable: ' .. tostring(err), 5)
    end
    return true
end

local function check_apply_current(pending)
    if not pending or pending_apply ~= pending or pending.token ~= runtime_apply_token then return end
    apply_current_timer = nil
    if tostring(mp.get_property('path', '') or '') ~= pending.path then
        pending_apply = nil
        set_runtime_error('Apply Current stopped because the playing file changed; no stale gain filter was restored.')
        reconcile_live_gain('path-changed-during-apply')
        return
    end
    local track = selected_audio_track()
    if not track or tostring(track.id or '') ~= pending.track_id then
        pending_apply = nil
        set_runtime_error('Apply Current stopped because the selected audio track changed; no stale gain filter was restored.')
        reconcile_live_gain('track-changed-during-apply')
        return
    end
    if apply_current_is_current(pending) and options_match(pending.expected) then
        if finish_apply_current(pending) then return end
    end
    if mp.get_property_native('pause', false) == true then
        apply_current_timer = mp.add_timeout(0.5, function() check_apply_current(pending) end)
        return
    end
    pending.attempt = pending.attempt + 1
    local delay = ({ 0.10, 0.20, 0.35, 0.50, 0.75, 1.00 })[pending.attempt]
    if delay then
        apply_current_timer = mp.add_timeout(delay, function() check_apply_current(pending) end)
    else
        pending_apply = nil
        set_runtime_error('Apply Current did not confirm the requested OpenJOC decoder settings; the gain filter remains suspended.')
    end
end

apply_current_playback = function()
    local active, decoder, track = openjoc_track()
    if not active then
        show_status('Apply Current requires an active libopenjoc E-AC-3 audio track ('
            .. tostring(decoder) .. ').', 4)
        return
    end
    local path = tostring(mp.get_property('path', '') or '')
    if path == '' then
        show_status('Apply Current is unavailable because mpv has no active file path.', 4)
        return
    end

    if gain_add_pending or gain_filter_settling then
        show_status('Apply Current is waiting for the live gain filter to initialize; retry shortly.', 5)
        return
    end

    local filter = named_gain_filter()
    if filter then
        local removed, remove_error = run_mpv_command('af', 'remove', '@openjoc_gain')
        if not removed or filter_exists() then
            show_status('Apply Current stopped because the live gain filter could not be safely suspended: '
                .. tostring(remove_error or 'filter is still present'), 5)
            return
        end
    end
    cancel_gain_retry()
    gain_epoch = gain_epoch + 1
    gain_pending = nil
    gain_add_pending = false
    gain_filter_settling = false

    local expected = {}
    local options = decoder_options_for_state(draft)
    for key, value in pairs(draft.options) do
        if key ~= 'output_gain_tenths_db' then expected[key] = value end
    end
    runtime_apply_token = runtime_apply_token + 1
    local pending = {
        token = runtime_apply_token,
        path = path,
        track_id = tostring(track.id or ''),
        expected = expected,
        gain = clamp_gain(draft.options.output_gain_tenths_db or active_gain_tenths_db),
        attempt = 0,
    }
    pending_apply = pending
    local ok, result, err = pcall(mp.set_property_native,
        'file-local-options/ad-lavc-o', options)
    if not ok or result ~= true then
        pending_apply = nil
        -- The filter was removed before the option write. Try to restore the
        -- exact current live gain without pretending Apply succeeded.
        request_runtime_gain(active_gain_tenths_db, 'apply-rollback')
        show_status('Apply Current failed; decoder options were not confirmed: '
            .. tostring(err or result), 5)
        return
    end

    show_status('Applying to current playback; a brief audio gap may occur.', 5)
    apply_current_timer = mp.add_timeout(0.75, function() check_apply_current(pending) end)
end

reconcile_live_gain = function(reason)
    if pending_apply then return end
    local active = openjoc_track()
    local signature = current_runtime_signature()
    if runtime_signature and signature ~= runtime_signature then
        -- File/track/mode transitions invalidate queued previews. Tear down the
        -- old labeled filter, then only restore after fresh decoder evidence.
        cancel_gain_retry()
        gain_epoch = gain_epoch + 1
        gain_pending = nil
        gain_preview_touched = false
        gain_restore_owed = false
        gain_restore_value = nil
        if filter_exists() then remove_gain_filter_for_non_openjoc() end
        runtime_signature = signature
        kill_timer(runtime_reconcile_timer)
        runtime_reconcile_timer = mp.add_timeout(0.20, function()
            runtime_reconcile_timer = nil
            reconcile_live_gain('settled-lifecycle')
        end)
        if menu_open then draw() end
        return
    end
    runtime_signature = signature
    if active then ensure_saved_gain_filter() else remove_gain_filter_for_non_openjoc() end
    if menu_open then draw() end
end

local function preview_gain(value)
    value = clamp_gain(value)
    draft.options.output_gain_tenths_db = value
    mark_changed()
    local ok, err = request_runtime_gain(value, 'preview')
    if ok then
        show_status('Live gain preview updated: ' .. gain_label(value) .. '.', 3)
    elseif gain_pending then
        show_status('Live gain preview queued; it will apply when the filter is ready or playback resumes.', 4)
    else
        show_status('Gain saved as a draft; live preview unavailable: ' .. tostring(err), 4)
    end
end

local function apply_gain_restore()
    if not gain_restore_owed or gain_restore_value == nil then return true end
    local target = gain_restore_value
    local ok, err = request_runtime_gain(target, 'restore')
    if ok then
        gain_preview_touched = false
        gain_restore_owed = false
        gain_restore_value = nil
        return true
    end
    if gain_pending then
        show_status('Cancel queued restoration to ' .. gain_label(target) .. '; waiting for the live filter.', 4)
        return nil, err
    end
    set_runtime_error('Cancel could not restore the prior live gain (' .. gain_label(target)
        .. '): ' .. tostring(err))
    return nil, err
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

local function states_equal(left, right)
    for key, value in pairs(left.options) do
        if right.options[key] ~= value then return false end
    end
    for key, value in pairs(right.options) do
        if left.options[key] ~= value then return false end
    end
    return true
end

mark_changed = function()
    dirty = not states_equal(draft, saved)
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
    if state_save_blocked then
        return false, (state_load_error or 'saved settings could not be read')
            .. ' Save is disabled to protect the existing file.'
    end
    local document = {
        schema = 1,
        options = draft.options,
    }
    local encoded = utils.format_json(document)
    if type(encoded) ~= 'string' then
        return false, 'could not encode settings'
    end
    local ok, write_error = settings_file_io.write(CONFIG_TEMP_PATH, encoded .. '\n')
    if not ok then
        settings_file_io.remove(CONFIG_TEMP_PATH)
        return false, write_error or 'settings directory is not writable'
    end
    local renamed, rename_error = settings_file_io.move(CONFIG_TEMP_PATH, CONFIG_PATH)
    if not renamed then
        -- Keep the previous file recoverable when the path-safe move backend
        -- cannot replace an existing destination in one operation.
        settings_file_io.remove(CONFIG_BACKUP_PATH)
        local existing = settings_file_io.read(CONFIG_PATH)
        local moved_old = false
        if existing then
            moved_old, rename_error = settings_file_io.move(CONFIG_PATH, CONFIG_BACKUP_PATH)
            if not moved_old then
                settings_file_io.remove(CONFIG_TEMP_PATH)
                return false, rename_error or 'could not preserve previous settings'
            end
        end
        renamed, rename_error = settings_file_io.move(CONFIG_TEMP_PATH, CONFIG_PATH)
        if not renamed then
            if moved_old then
                settings_file_io.move(CONFIG_BACKUP_PATH, CONFIG_PATH)
            end
            settings_file_io.remove(CONFIG_TEMP_PATH)
            return false, rename_error or 'could not replace settings file'
        end
        if moved_old then
            settings_file_io.remove(CONFIG_BACKUP_PATH)
        end
    end
    saved = copy_state(draft)
    dirty = false
    state_load_error = nil
    state_save_blocked = false
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

local function osd_dimensions()
    local width, height
    if type(mp.get_osd_size) == 'function' then
        width, height = mp.get_osd_size()
    end
    if type(width) ~= 'number' or width < 1 then
        width = mp.get_property_number and mp.get_property_number('osd-width', 1280) or 1280
    end
    if type(height) ~= 'number' or height < 1 then
        height = mp.get_property_number and mp.get_property_number('osd-height', 720) or 720
    end
    if type(width) ~= 'number' or width < 1 then width = 1280 end
    if type(height) ~= 'number' or height < 1 then height = 720 end
    return width, height
end

local function geometry()
    local screen_w, screen_h = osd_dimensions()
    local compact = screen_h < 500
    local base_w = compact and 760 or 850
    local base_h = compact and 470 or 620
    local scale = math.min(1, screen_w * 0.94 / base_w, screen_h * 0.94 / base_h)
    local panel_w = base_w * scale
    local panel_h = base_h * scale
    local x = (screen_w - panel_w) / 2
    local y = (screen_h - panel_h) / 2
    local pad = (compact and 22 or 30) * scale
    local row_h = (compact and 32 or 42) * scale
    local first_row_y = y + (compact and 72 or 106) * scale
    local row_rects = {}
    local controls_x = x + panel_w * 0.57
    local controls_right = x + panel_w - pad
    local arrow_w = 34 * scale
    for index = 1, 6 do
        local row_y = first_row_y + (index - 1) * row_h
        row_rects[index] = {
            x = x + pad, y = row_y, w = panel_w - pad * 2, h = row_h - 3 * scale,
            control_x = controls_x, control_right = controls_right,
            arrow_w = arrow_w,
            choose_x = controls_right - 88 * scale,
            choose_w = 88 * scale,
        }
        if index == 4 then
            row_rects[index].choose = {
                x = controls_right - 88 * scale,
                y = row_y + 3 * scale,
                w = 88 * scale,
                h = row_h - 9 * scale,
            }
        end
        if index == 6 then
            row_rects[index].gain_reset = {
                x = controls_right - 58 * scale,
                y = row_y + 3 * scale,
                w = 58 * scale,
                h = row_h - 6 * scale,
            }
        end
    end
    local button_h = 42 * scale
    local button_w = 108 * scale
    local button_y = y + (compact and 390 or 532) * scale
    local button_gap = 10 * scale
    local cancel = { x = x + panel_w - pad - button_w, y = button_y, w = button_w, h = button_h }
    local apply = { x = cancel.x - button_gap - button_w, y = button_y, w = button_w, h = button_h }
    local save = { x = apply.x - button_gap - button_w, y = button_y, w = button_w, h = button_h }
    return {
        width = screen_w, height = screen_h, x = x, y = y,
        w = panel_w, h = panel_h, scale = scale, compact = compact,
        rows = row_rects, save = save, apply = apply, cancel = cancel,
        help_y = y + (compact and 350 or 582) * scale,
        status_y = y + (compact and 315 or 466) * scale,
        live_title_y = y + (compact and 270 or 365) * scale,
        live_rows_y = y + 390 * scale,
    }
end

local function ass_escape(value)
    local text = tostring(value or ''):gsub('[\r\n]', ' ')
    local ok, escaped = pcall(mp.command_native, { 'escape-ass', text })
    if ok and type(escaped) == 'string' then return escaped end
    text = text:gsub('\\', '\\\\')
    return text:gsub('{', '\\{'):gsub('}', '\\}')
end

local function shorten(value, limit)
    local text = tostring(value or '')
    local has_wide_char = false
    for index = 1, #text do
        if text:byte(index) >= 0xE0 then has_wide_char = true; break end
    end
    local max_chars = has_wide_char and math.max(5, math.floor(limit / 2)) or limit
    local index, count = 1, 0
    while index <= #text and count < max_chars do
        local first = text:byte(index)
        local length = first < 0x80 and 1 or (first < 0xE0 and 2 or (first < 0xF0 and 3 or 4))
        local valid = index + length - 1 <= #text
        if valid and length > 1 then
            for offset = 1, length - 1 do
                local continuation = text:byte(index + offset)
                if continuation < 0x80 or continuation > 0xBF then valid = false; break end
            end
        end
        index = index + (valid and length or 1)
        count = count + 1
    end
    if index > #text then return text end
    if count < max_chars then return text end
    return text:sub(1, index - 1) .. '...'
end

local function add_rect(events, x, y, width, height, color, alpha)
    local x0, y0 = math.floor(x), math.floor(y)
    local w, h = math.max(1, math.floor(width)), math.max(1, math.floor(height))
    events[#events + 1] = string.format(
        '{\\an7\\pos(%d,%d)\\bord0\\shad0\\p1\\1c&H%s&\\1a&H%02X&}' ..
        'm 0 0 l %d 0 %d %d 0 %d{\\p0}\n',
        x0, y0, color, alpha or 0, w, w, h, h)
end

local function add_text(events, x, y, value, size, color, alpha)
    events[#events + 1] = string.format(
        '{\\an7\\pos(%d,%d)\\bord0\\shad0\\q2\\fnArial\\fs%d\\1c&H%s&\\1a&H%02X&}%s\n',
        math.floor(x), math.floor(y), math.max(8, math.floor(size)), color,
        alpha or 0, ass_escape(value))
end

local function selected_hrtf(direction)
    local current = hrtf_source_index()
    local next_index = ((current - 1 + direction) % 3) + 1
    if next_index == 3 then
        open_sofa_prompt()
    else
        draft.options.hrtf = next_index == 1 and 'd1' or 'd2'
        draft.options.sofa = ''
        mark_changed()
    end
end

local function main_rows()
    local dialnorm = effective_value('dialnorm', 'default')
    local dialnorm_label = ({
        default = 'Calibrated (recommended)',
        analog = 'Unity / Compatibility',
        digital = 'Digital (advanced)',
    })[dialnorm] or dialnorm
    local virtual = shown_option('virtual_layout', '7.1.4')
    if virtual == '9.1.6' then virtual = '9.1.6 (experimental)' end
    local sofa = effective_value('sofa', '')
    local display_path = sofa:gsub('\\', '/')
    local sofa_name = display_path:match('([^/]+)$') or display_path
    local sofa_label = sofa ~= '' and shorten(sofa_name, 36) or 'No file selected'
    return {
        { title = 'Output policy', value = current_output_label(), change = change_output },
        {
            title = 'Dialnorm', value = dialnorm_label,
            change = function(direction)
                local choices = { 'default', 'analog' }
                local current = effective_value('dialnorm', 'default')
                local index
                for choice_index, choice in ipairs(choices) do
                    if choice == current then index = choice_index; break end
                end
                if not index then index = direction > 0 and 1 or #choices end
                index = ((index - 1 + direction) % #choices) + 1
                set_option('dialnorm', choices[index])
            end,
        },
        { title = 'Binaural HRTF (when selected)', value = label_for_hrtf(), change = selected_hrtf },
        { title = 'Custom SOFA file', value = sofa_label, action = open_sofa_prompt },
        {
            title = 'Binaural virtual layout (when selected)', value = virtual,
            change = function(direction)
                local choices = { '7.1.4', '9.1.6' }
                local index = option_index('virtual_layout', choices, '7.1.4')
                index = ((index - 1 + direction) % #choices) + 1
                set_option('virtual_layout', choices[index])
            end,
        },
        {
            title = 'Live output gain',
            value = gain_label(effective_value('output_gain_tenths_db', 0)),
            change = function(direction)
                local current = clamp_gain(effective_value('output_gain_tenths_db', 0))
                preview_gain(current + direction)
            end,
            activate = function() preview_gain(0) end,
        },
    }
end

local function show_overlay(events, dimensions)
    overlay.res_x = math.max(1, math.floor(dimensions.width))
    overlay.res_y = math.max(1, math.floor(dimensions.height))
    overlay.data = table.concat(events)
    overlay:update()
end

draw = function()
    if not menu_open then return end
    local g = geometry()
    local s, events = g.scale, {}
    local rows = main_rows()
    row = math.max(1, math.min(row, #rows + 3))

    -- A high-opacity charcoal panel remains readable over bright or moving
    -- video while keeping a narrow border of the current picture visible.
    add_rect(events, g.x, g.y, g.w, g.h, '151A22', 12)
    add_rect(events, g.x, g.y, g.w, 3 * s, 'B5D869', 0)
    add_text(events, g.x + 30 * s, g.y + 20 * s, 'OpenJOC settings', 28 * s, 'F4F7F8')
    local badge = dirty and 'UNSAVED DRAFT' or 'READY FOR NEXT FILE'
    local badge_color = dirty and '72C8F4' or 'B5D869'
    add_text(events, g.x + g.w - 210 * s, g.y + 28 * s, badge, 13 * s, badge_color)
    local summary = state_save_blocked
        and 'Settings need attention. Save is disabled to protect your data.'
        or (dirty
            and 'Save for the next file, or use Apply Current for a brief-gap decoder update.'
            or 'Saved settings target the next file; Apply Current is explicit and may briefly pause audio.')
    add_text(events, g.x + 30 * s, g.y + 56 * s, summary, 14 * s, 'C3CDD4')
    if not g.compact then
        add_text(events, g.x + 30 * s, g.y + 84 * s,
            'OUTPUT AND BINAURAL OPTIONS', 12 * s, '83A4B4')
    end

    for index, item in ipairs(rows) do
        local r = g.rows[index]
        local focused = row == index
        add_rect(events, r.x, r.y, r.w, r.h,
            focused and '293641' or '202832', focused and 0x08 or 0x18)
        add_text(events, r.x + 14 * s, r.y + 12 * s, item.title,
            16 * s, focused and 'F5FAFC' or 'D8E0E5')
        if index == 4 then
            local choose = { x = r.choose_x, y = r.y + 3 * s, w = r.choose_w, h = r.h - 6 * s }
            add_text(events, r.control_x + 8 * s, r.y + 12 * s,
                shorten(item.value, g.compact and 22 or 26), 15 * s, 'BBC9D2')
            add_rect(events, choose.x, choose.y, choose.w, choose.h,
                focused and '456352' or '34444E', 0)
            add_text(events, choose.x + 14 * s, choose.y + 8 * s,
                effective_value('sofa', '') ~= '' and 'Change path...' or 'Set path...',
                13 * s, 'F4F7F8')
            r.choose = choose
        elseif index == 6 then
            local left = { x = r.control_x, y = r.y + 3 * s, w = r.arrow_w, h = r.h - 6 * s }
            local reset = r.gain_reset
            local right = { x = reset.x - 8 * s - r.arrow_w, y = left.y,
                w = r.arrow_w, h = left.h }
            add_rect(events, left.x, left.y, left.w, left.h, '34434D', 0)
            add_rect(events, right.x, right.y, right.w, right.h, '34434D', 0)
            add_text(events, left.x + 10 * s, left.y + 7 * s, '−', 15 * s, 'F4F7F8')
            add_text(events, right.x + 10 * s, right.y + 7 * s, '+', 15 * s, 'F4F7F8')
            add_text(events, left.x + left.w + 9 * s, r.y + 12 * s,
                item.value, 15 * s, focused and 'B5D869' or 'E8EDF0')
            add_rect(events, reset.x, reset.y, reset.w, reset.h,
                focused and '456352' or '34444E', 0)
            add_text(events, reset.x + 9 * s, reset.y + 7 * s, 'Reset', 12 * s, 'F4F7F8')
            r.left, r.right, r.gain_reset = left, right, reset
        else
            local left = { x = r.control_x, y = r.y + 3 * s, w = r.arrow_w, h = r.h - 6 * s }
            local right = { x = r.control_right - r.arrow_w, y = left.y, w = r.arrow_w, h = left.h }
            add_rect(events, left.x, left.y, left.w, left.h, '34434D', 0)
            add_rect(events, right.x, right.y, right.w, right.h, '34434D', 0)
            add_text(events, left.x + 12 * s, left.y + 7 * s, '<', 15 * s, 'F4F7F8')
            add_text(events, right.x + 12 * s, right.y + 7 * s, '>', 15 * s, 'F4F7F8')
            add_text(events, left.x + left.w + 11 * s, r.y + 12 * s,
                shorten(item.value, g.compact and 28 or 34), 15 * s,
                focused and 'B5D869' or 'E8EDF0')
            r.left, r.right = left, right
        end
    end

    if g.compact then
        local decoder = shorten(read_only_property('current-tracks/audio/decoder'), 18)
        local input_channels = shorten(read_only_property('audio-params/hr-channels',
            read_only_property('audio-params/channels')), 12)
        local output_channels = shorten(read_only_property('audio-out-params/hr-channels',
            read_only_property('audio-out-params/channels')), 12)
        add_text(events, g.x + 30 * s, g.live_title_y, 'LIVE PLAYER', 12 * s, '83A4B4')
        add_text(events, g.x + 30 * s, g.live_title_y + 17 * s,
            'Decoder: ' .. decoder .. '   Input: ' .. input_channels .. '   Output: ' .. output_channels,
            12 * s, 'C3CDD4')
        local explicit_action_status = status_message and
            (status_message:find('Saved for the next file', 1, true)
                or status_message:find('Apply Current', 1, true)) and status_message
        local warning = gain_cleanup_warning or explicit_action_status
            or gain_runtime_error or status_message or state_load_error
        add_text(events, g.x + 30 * s, g.status_y,
            warning
                and shorten(warning, 80)
                or 'Best-effort E-AC-3 options; mpv audio routing is unchanged.',
            12 * s, warning and 'F0B8A2' or '99AAB4')
    else
        add_text(events, g.x + 30 * s, g.live_title_y, 'LIVE PLAYER  ·  READ ONLY', 12 * s, '83A4B4')
        local labels = {
            { 'Active decoder', read_only_property('current-tracks/audio/decoder') },
            { 'Decoder output channels', read_only_property('audio-params/hr-channels',
                read_only_property('audio-params/channels')) },
            { 'Audio output channels', read_only_property('audio-out-params/hr-channels',
                read_only_property('audio-out-params/channels')) },
        }
        for index, item in ipairs(labels) do
            local y = g.live_rows_y + (index - 1) * 21 * s
            add_text(events, g.x + 30 * s, y, item[1], 12 * s, '99AAB4')
            add_text(events, g.x + 230 * s, y, shorten(item[2], 52), 12 * s, 'DCE4E8')
        end
        local explicit_action_status = status_message and
            (status_message:find('Saved for the next file', 1, true)
                or status_message:find('Apply Current', 1, true)) and status_message
        local warning = gain_cleanup_warning or explicit_action_status
            or gain_runtime_error or status_message or state_load_error
        add_text(events, g.x + 30 * s, g.y + 466 * s,
            warning and shorten(warning, 96)
                or ('Live gain: ' .. gain_label(active_gain_tenths_db)
                    .. '  ·  only active on selected libopenjoc E-AC-3.'),
            12 * s, warning and 'F0B8A2' or '99AAB4')
    end

    local save_focused, apply_focused, cancel_focused =
        row == #rows + 1, row == #rows + 2, row == #rows + 3
    add_rect(events, g.save.x, g.save.y, g.save.w, g.save.h,
        state_save_blocked and '394047' or (save_focused and '427D67' or '355F51'), 0)
    local can_apply = openjoc_track()
    add_rect(events, g.apply.x, g.apply.y, g.apply.w, g.apply.h,
        can_apply and (apply_focused and '496E88' or '38576D') or '394047', 0)
    add_rect(events, g.cancel.x, g.cancel.y, g.cancel.w, g.cancel.h,
        cancel_focused and '58636B' or '303942', 0)
    add_text(events, g.save.x + 22 * s, g.save.y + 12 * s,
        state_save_blocked and 'Save disabled' or 'Save', 15 * s,
        state_save_blocked and 'AAB4BA' or 'FFFFFF')
    add_text(events, g.apply.x + 15 * s, g.apply.y + 12 * s, 'Apply Current', 13 * s,
        can_apply and 'FFFFFF' or 'AAB4BA')
    add_text(events, g.cancel.x + 30 * s, g.cancel.y + 12 * s, 'Cancel', 15 * s, 'F4F7F8')
    add_text(events, g.x + 30 * s, g.help_y,
        'Click controls or use Up/Down, Left/Right, Enter, Tab. Esc closes and keeps the draft.',
        11 * s, '9BAAB2')
    show_overlay(events, g)
    reset_menu_timeout()
end

local function save_selected()
    if state_save_blocked then
        show_status('Save disabled to protect the existing settings file.', 4)
        return
    end
    local ok, err = save_state()
    if not ok then
        show_status('Settings not saved: ' .. tostring(err), 4)
        return
    end
    -- Saving makes this gain the new Cancel baseline. A pending live request
    -- remains pending, but it is no longer an unsaved preview.
    gain_preview_touched = false
    gain_restore_owed = false
    gain_restore_value = nil
    local active = openjoc_track()
    if gain_pending then
        show_status('Saved for the next file; live gain is still queued and not yet confirmed.', 5)
    elseif gain_runtime_error or gain_cleanup_warning then
        show_status('Saved for the next file; live gain is not confirmed: '
            .. tostring(gain_runtime_error or gain_cleanup_warning), 5)
    elseif active and clamp_gain(active_gain_tenths_db)
        == clamp_gain(saved.options.output_gain_tenths_db or 0) then
        show_status('Saved for the next file; live preview remains at '
            .. gain_label(active_gain_tenths_db) .. '.', 4)
    elseif not active and not gain_preview_touched then
        show_status('Saved for the next file. Current playback is unchanged.', 4)
    else
        show_status('Saved for the next file; current live gain was not changed.', 4)
    end
end

local function cancel_draft()
    local had_gain_preview = gain_restore_owed or gain_preview_touched
    local restored, restore_error = true, nil
    if had_gain_preview then
        restored, restore_error = apply_gain_restore()
    end
    draft = copy_state(saved)
    dirty = false
    close_menu()
    if had_gain_preview and not restored then
        local message = gain_pending
            and 'Cancel queued restoration of the previous live gain; it will retry when playback resumes or the filter is ready.'
            or ('Cancel could not restore the previous live gain: ' .. tostring(restore_error))
        mp.osd_message(message, 5)
    end
end

local function resume_menu()
    if menu_open then return end
    sofa_input_open = false
    menu_open = true
    if install_menu_keys then install_menu_keys() end
    draw()
end

open_sofa_prompt = function()
    if sofa_input_open then return end
    sofa_input_open = true
    menu_open = false
    if menu_timeout then menu_timeout:kill(); menu_timeout = nil end
    if remove_menu_keys then remove_menu_keys() end
    overlay.data = ''
    overlay:update()
    local current_path = effective_value('sofa', '')
    input.get({
        prompt = 'Custom SOFA file path (local file): ',
        default_text = current_path,
        id = 'openjoc-sofa-path',
        closed = function()
            resume_menu()
        end,
        submit = function(path)
            if type(path) ~= 'string' or path:match('^%s*$') then
                resume_menu()
                show_status('No SOFA path entered.', 3)
                return
            end
            local expanded = mp.command_native({ 'expand-path', path })
            local normalized = expanded and mp.command_native({ 'normalize-path', expanded })
            local info = normalized and utils.file_info(normalized)
            resume_menu()
            if not info or not info.is_file then
                show_status('Choose an existing local SOFA file.', 3)
                return
            end
            draft.options.sofa = normalized
            mark_changed()
            show_status('Custom SOFA selected for the next file.', 3)
        end,
    })
end

local function point_inside(point_x, point_y, rect)
    return point_x >= rect.x and point_x <= rect.x + rect.w
        and point_y >= rect.y and point_y <= rect.y + rect.h
end

local function save_state_or_report()
    save_selected()
end

change_selected = function(direction, activate)
    local rows = main_rows()
    if row <= #rows then
        local item = rows[row]
        if activate and item.activate then
            item.activate()
        elseif item.change then
            item.change(direction)
        elseif activate and item.action then
            item.action()
        end
    elseif row == #rows + 1 then
        save_state_or_report()
    elseif row == #rows + 2 then
        apply_current_playback()
    elseif row == #rows + 3 then
        cancel_draft()
    end
    if menu_open then draw() end
end

local function move_focus(delta)
    local max_row = #main_rows() + 3
    row = ((row - 1 + delta) % max_row) + 1
    draw()
end

local function on_mouse_click()
    if not menu_open then return end
    local mouse_x, mouse_y = mp.get_mouse_pos()
    if type(mouse_x) ~= 'number' or type(mouse_y) ~= 'number' then return end
    local g = geometry()
    if not point_inside(mouse_x, mouse_y, { x = g.x, y = g.y, w = g.w, h = g.h }) then
        local had_draft = dirty
        close_menu()
        if had_draft then
            mp.osd_message('Draft kept. Reopen settings to save or cancel.', 4)
        end
        return
    end
    if point_inside(mouse_x, mouse_y, g.save) then
        row = #main_rows() + 1
        save_selected()
        if menu_open then draw() end
        return
    end
    if point_inside(mouse_x, mouse_y, g.apply) then
        row = #main_rows() + 2
        apply_current_playback()
        if menu_open then draw() end
        return
    end
    if point_inside(mouse_x, mouse_y, g.cancel) then
        cancel_draft()
        return
    end
    local rows = main_rows()
    for index, rect in ipairs(g.rows) do
        if point_inside(mouse_x, mouse_y, rect) then
            row = index
            local item = rows[index]
            if index == 4 and point_inside(mouse_x, mouse_y, rect.choose) then
                open_sofa_prompt()
                return
            elseif index == 6 and point_inside(mouse_x, mouse_y, rect.gain_reset) then
                change_selected(1, true)
            elseif item.change and mouse_x >= rect.control_x then
                local direction = mouse_x >= rect.control_right - rect.arrow_w and 1
                    or (mouse_x <= rect.control_x + rect.arrow_w and -1 or 1)
                change_selected(direction)
            else
                draw()
            end
            return
        end
    end
    reset_menu_timeout()
end

close_menu = function()
    menu_open = false
    if menu_timeout then menu_timeout:kill(); menu_timeout = nil end
    if status_timeout then status_timeout:kill(); status_timeout = nil end
    status_message = nil
    if remove_menu_keys then remove_menu_keys() end
    overlay.data = ''
    overlay:update()
end

remove_menu_keys = function()
    for _, name in ipairs({ 'up', 'down', 'left', 'right', 'enter', 'escape',
        'tab', 'shift-tab', 'mouse' }) do
        mp.remove_key_binding('openjoc-settings-' .. name)
    end
end

install_menu_keys = function()
    mp.add_forced_key_binding('UP', 'openjoc-settings-up', function() move_focus(-1) end,
        { repeatable = true })
    mp.add_forced_key_binding('DOWN', 'openjoc-settings-down', function() move_focus(1) end,
        { repeatable = true })
    mp.add_forced_key_binding('LEFT', 'openjoc-settings-left', function()
        local last = #main_rows()
        if row == last + 1 then row = last + 3; draw()
        elseif row == last + 2 then row = last + 1; draw()
        elseif row == last + 3 then row = last + 2; draw()
        else change_selected(-1) end
    end, { repeatable = true })
    mp.add_forced_key_binding('RIGHT', 'openjoc-settings-right', function()
        local last = #main_rows()
        if row == last + 1 then row = last + 2; draw()
        elseif row == last + 2 then row = last + 3; draw()
        elseif row == last + 3 then row = last + 1; draw()
        else change_selected(1) end
    end, { repeatable = true })
    mp.add_forced_key_binding('ENTER', 'openjoc-settings-enter', function()
        change_selected(1, true)
    end)
    mp.add_forced_key_binding('TAB', 'openjoc-settings-tab', function() move_focus(1) end)
    mp.add_forced_key_binding('Shift+TAB', 'openjoc-settings-shift-tab', function()
        move_focus(-1)
    end)
    mp.add_forced_key_binding('ESC', 'openjoc-settings-escape', function()
        local had_draft = dirty
        close_menu()
        if had_draft then mp.osd_message('Draft kept. Reopen settings to save or cancel.', 4) end
    end)
    mp.add_forced_key_binding('MBTN_LEFT', 'openjoc-settings-mouse', on_mouse_click)
end

local function toggle_menu()
    if sofa_input_open then return end
    if menu_open then
        local had_draft = dirty
        close_menu()
        if had_draft then
            mp.osd_message('Draft kept. Reopen settings to save or cancel.', 4)
        end
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
        if key ~= 'output_gain_tenths_db' then options[key] = value end
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
    'track-list',
    'path',
    'file-local-options/ad-lavc-o',
    'audio-params/hr-channels',
    'audio-out-params/hr-channels',
    'osd-dimensions',
    'af',
    'pause',
}) do
    local format = (property == 'osd-dimensions' or property == 'track-list'
        or property == 'file-local-options/ad-lavc-o' or property == 'af'
        or property == 'pause') and 'native' or 'string'
    mp.observe_property(property, format, function(_, value)
        if property == 'pause' then
            if value == false and gain_pending then
                schedule_gain_retry(gain_pending, GAIN_RETRY_DELAYS[1])
            end
            if value == false and pending_apply then
                kill_timer(apply_current_timer)
                apply_current_timer = mp.add_timeout(0.05, function()
                    check_apply_current(pending_apply)
                end)
            end
        elseif property == 'af' then
            local active = openjoc_track()
            if (gain_add_cleanup_pending or (gain_add_pending and not active))
                and not gain_cleanup_in_progress then
                gain_cleanup_in_progress = true
                if active then
                    gain_add_cleanup_pending = false
                    gain_add_cleanup_attempt = 0
                    clear_cleanup_warning()
                    reconcile_live_gain('af-visible-on-openjoc')
                else
                    gain_add_cleanup_pending = true
                    remove_gain_filter_for_non_openjoc()
                end
                gain_cleanup_in_progress = false
            elseif gain_pending and gain_add_pending and not gain_cleanup_in_progress then
                -- AF property changes are a readiness signal. Retry the latest
                -- queued value promptly rather than issuing another add.
                schedule_gain_retry(gain_pending, GAIN_RETRY_DELAYS[1])
            end
        elseif property == 'current-tracks/audio/decoder'
            or property == 'track-list' or property == 'path'
            or property == 'file-local-options/ad-lavc-o' then
            reconcile_live_gain(property)
        end
        if menu_open then draw() end
    end)
end
if type(mp.register_event) == 'function' then
    mp.register_event('end-file', function()
        pending_apply = nil
        runtime_apply_token = runtime_apply_token + 1
        kill_timer(apply_current_timer)
        apply_current_timer = nil
        kill_timer(runtime_reconcile_timer)
        runtime_reconcile_timer = nil
        runtime_signature = nil
        remove_gain_filter_for_non_openjoc()
    end)
end
reconcile_live_gain('startup')
mp.msg.info('OpenJOC settings menu loaded (Ctrl+Alt+J)')

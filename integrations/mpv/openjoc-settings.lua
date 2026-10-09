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

local function states_equal(left, right)
    for key, value in pairs(left.options) do
        if right.options[key] ~= value then return false end
    end
    for key, value in pairs(right.options) do
        if left.options[key] ~= value then return false end
    end
    return true
end

local function mark_changed()
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
    local base_h = compact and 470 or 560
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
    for index = 1, 5 do
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
    end
    local button_h = 42 * scale
    local button_w = 112 * scale
    local button_y = y + (compact and 390 or 491) * scale
    local button_gap = 12 * scale
    local cancel = { x = x + panel_w - pad - button_w, y = button_y, w = button_w, h = button_h }
    local save = { x = cancel.x - button_gap - button_w, y = button_y, w = button_w, h = button_h }
    return {
        width = screen_w, height = screen_h, x = x, y = y,
        w = panel_w, h = panel_h, scale = scale, compact = compact,
        rows = row_rects, save = save, cancel = cancel,
        help_y = y + (compact and 350 or 541) * scale,
        status_y = y + (compact and 286 or 451) * scale,
        live_title_y = y + (compact and 245 or 333) * scale,
        live_rows_y = y + 356 * scale,
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
    row = math.max(1, math.min(row, #rows + 2))

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
            and 'Draft only. Save stores it for the next file; current playback stays as-is.'
            or 'Current settings take effect when the next file opens; current playback stays as-is.')
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
        add_text(events, g.x + 30 * s, g.status_y,
            (status_message or state_load_error)
                and shorten(status_message or state_load_error, 56)
                or 'Best-effort E-AC-3 options; mpv audio routing is unchanged.',
            12 * s, (status_message or state_load_error) and 'F0B8A2' or '99AAB4')
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
        add_text(events, g.x + 30 * s, g.y + 423 * s,
            (status_message or state_load_error) and shorten(status_message or state_load_error, 96)
                or 'Live rows are mpv properties, not JOC diagnostics; LAV output gain is not exposed here.',
            12 * s, (status_message or state_load_error) and 'F0B8A2' or '99AAB4')
    end

    local save_focused, cancel_focused = row == #rows + 1, row == #rows + 2
    add_rect(events, g.save.x, g.save.y, g.save.w, g.save.h,
        state_save_blocked and '394047' or (save_focused and '427D67' or '355F51'), 0)
    add_rect(events, g.cancel.x, g.cancel.y, g.cancel.w, g.cancel.h,
        cancel_focused and '58636B' or '303942', 0)
    add_text(events, g.save.x + 22 * s, g.save.y + 12 * s,
        state_save_blocked and 'Save disabled' or 'Save', 15 * s,
        state_save_blocked and 'AAB4BA' or 'FFFFFF')
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
    show_status('Saved for the next file. Current playback is unchanged.', 4)
end

local function cancel_draft()
    draft = copy_state(saved)
    dirty = false
    close_menu()
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
        if item.change then
            item.change(direction)
        elseif activate and item.action then
            item.action()
        end
    elseif row == #rows + 1 then
        save_state_or_report()
    elseif row == #rows + 2 then
        cancel_draft()
    end
    if menu_open then draw() end
end

local function move_focus(delta)
    local max_row = #main_rows() + 2
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
        if row == #main_rows() + 2 then row = row - 1; draw()
        elseif row == #main_rows() + 1 then row = row + 1; draw()
        else change_selected(-1) end
    end, { repeatable = true })
    mp.add_forced_key_binding('RIGHT', 'openjoc-settings-right', function()
        if row == #main_rows() + 1 then row = row + 1; draw()
        elseif row == #main_rows() + 2 then row = row - 1; draw()
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
    'osd-dimensions',
}) do
    mp.observe_property(property, property == 'osd-dimensions' and 'native' or 'string', function()
        if menu_open then draw() end
    end)
end
mp.msg.info('OpenJOC settings menu loaded (Ctrl+Alt+J)')

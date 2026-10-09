-- SPDX-FileCopyrightText: 2026 OpenJOC contributors
-- SPDX-License-Identifier: Apache-2.0

-- Headless driver for the actual mpv-loaded settings script. The
-- qualification harness runs it with mpv.exe under a Unicode config path.
local ffi_ok, ffi = pcall(require, 'ffi')
if not ffi_ok or type(ffi.cdef) ~= 'function' then
    mp.msg.error('OPENJOC_SETTINGS_MPV_DRIVER_FAIL: LuaJIT FFI unavailable')
    mp.commandv('quit', 1)
    return
end

local actions = {
    'openjoc-settings-toggle',
    'openjoc-settings-right',
    'openjoc-settings-down',
    'openjoc-settings-down',
    'openjoc-settings-down',
    'openjoc-settings-down',
    'openjoc-settings-down',
    'openjoc-settings-enter',
}

local index = 0
local function dispatch_next()
    index = index + 1
    if index <= #actions then
        mp.commandv('script-binding', 'openjoc_settings/' .. actions[index])
        mp.add_timeout(0.15, dispatch_next)
        return
    end

    mp.add_timeout(0.3, function()
        mp.msg.info('OPENJOC_SETTINGS_MPV_DRIVER_DONE')
        mp.commandv('quit', 0)
    end)
end

mp.add_timeout(0.5, dispatch_next)

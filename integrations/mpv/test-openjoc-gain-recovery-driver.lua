-- SPDX-FileCopyrightText: 2026 OpenJOC contributors
-- SPDX-License-Identifier: Apache-2.0

-- Real-player regression for dedicated gain-context persistence.
-- Host AF commands touch only unrelated labels. DSP traces are checked by
-- scripts/verify-mpv-gain-recovery.py; the static AF graph is not live volume.
local utils = require 'mp.utils'
local options = { recovery_case = 'saved' }
require('mp.options').read_options(options, 'openjoc_gain_recovery')
local scenario = options.recovery_case
local initial_path
local initial_track
local playback_restarts = 0
local finished = false

mp.register_event('playback-restart', function() playback_restarts = playback_restarts + 1 end)
local function filters()
    local owned, upstream, downstream = 0, 0, 0
    for _, filter in ipairs(mp.get_property_native('af', {})) do
        if filter.label == 'openjoc_gain' then owned = owned + 1 end
        if filter.label == 'recovery_upstream' then upstream = upstream + 1 end
        if filter.label == 'recovery_fixed_ao' then downstream = downstream + 1 end
    end
    return owned, upstream, downstream
end
local function check_owned()
    assert(mp.get_property('path', '') == initial_path, 'gain context changed file')
    assert(mp.get_property('current-tracks/audio/decoder', '') == 'libopenjoc', 'OpenJOC decoder changed')
    assert(filters() == 1, 'dedicated gain filter is absent or duplicated')
    local selected
    for _, track in ipairs(mp.get_property_native('track-list', {})) do
        if track.type == 'audio' and track.selected then selected = track.id end
    end
    assert(selected == initial_track, 'gain context changed selected track')
end
local function mark(stage)
    mp.msg.info('GAIN_RECOVERY_STAGE ' .. stage .. ' af=' .. utils.format_json(mp.get_property_native('af', {})))
end
local function dispatch(actions, callback)
    local index = 1
    local function next_action()
        if index > #actions then callback(); return end
        assert(mp.commandv('script-binding', 'openjoc_settings/openjoc-settings-' .. actions[index]))
        index = index + 1
        mp.add_timeout(0.03, next_action)
    end
    next_action()
end
local function finish()
    check_owned()
    local owned, upstream, downstream = filters()
    assert(owned == 1 and upstream == 1 and downstream == 1, 'host filters changed or duplicated')
    assert(mp.get_property_number('audio-out-params/samplerate', 0) == 48000, 'fixed downstream AO changed')
    mark('DONE')
    finished = true
    mp.commandv('quit', 0)
end
local function change_upstream(rate, callback)
    local _, upstream = filters()
    mark('PRE_FORMAT_' .. rate)
    if upstream > 0 then assert(mp.commandv('af', 'remove', '@recovery_upstream')) end
    assert(mp.commandv('af', 'pre', '@recovery_upstream:lavfi=[aresample=' .. rate .. ']'))
    mp.add_timeout(0.5, function()
        check_owned()
        mark('POST_FORMAT_' .. rate)
        callback()
    end)
end
local function after_first_rebuild()
    if scenario == 'cancel' then
        dispatch({ 'down', 'down', 'down', 'enter' }, function()
            mark('CANCEL_01')
            change_upstream('32000', finish)
        end)
    elseif scenario == 'new-preview' then
        dispatch({ 'right' }, function()
            mark('PREVIEW_03')
            change_upstream('32000', finish)
        end)
    elseif scenario == 'reload' then
        dispatch({ 'up', 'right' }, function()
            mark('PREVIEW_03')
            mark('PRE_NEW_FILE')
            assert(mp.commandv('loadfile', initial_path, 'replace'))
            mp.add_timeout(1, function() mark('POST_NEW_FILE'); finish() end)
        end)
    elseif scenario == 'track' then
        dispatch({ 'up', 'right' }, function()
            mark('PREVIEW_03')
            mark('PRE_NON_OPENJOC_TRACK')
            assert(mp.set_property_native('aid', 2))
            mp.add_timeout(0.6, function()
                assert(mp.get_property('current-tracks/audio/decoder', ''):match('^pcm') ~= nil, 'ordinary track was not selected')
                assert(filters() == 0, 'OpenJOC gain leaked onto the ordinary track')
                mark('NON_OPENJOC_TRACK_NO_GAIN')
                assert(mp.set_property_native('aid', initial_track))
                mp.add_timeout(1, function() mark('POST_NEW_TRACK'); finish() end)
            end)
        end)
    elseif scenario == 'seek' then
        assert(mp.get_property_native('seekable') == true, 'seek input must be genuinely seekable')
        local before = mp.get_property_number('time-pos', -1)
        local restarts = playback_restarts
        mark('PRE_SEEK')
        assert(mp.commandv('seek', '8', 'absolute+exact'))
        mp.add_timeout(0.6, function()
            local after = mp.get_property_number('time-pos', -1)
            assert(playback_restarts > restarts and after >= 8 and after < 10 and after > before + 2,
                'seek command did not produce an actual playback discontinuity')
            mp.msg.info('GAIN_RECOVERY_SEEK before=' .. before .. ' after=' .. after .. ' seekable=true')
            mark('POST_SEEK')
            finish()
        end)
    else
        change_upstream('32000', function() change_upstream('44100', finish) end)
    end
end
local function change_host_format()
    check_owned()
    assert(mp.commandv('af', 'add', '@recovery_fixed_ao:lavfi=[aresample=48000]'))
    mp.add_timeout(0.35, function()
        if scenario == 'paused' then
            assert(mp.set_property_native('pause', true))
            mark('PAUSED_PRE_FORMAT')
            assert(mp.commandv('af', 'pre', '@recovery_upstream:lavfi=[aresample=44100]'))
            mp.add_timeout(0.6, function()
                mark('RESUME')
                assert(mp.set_property_native('pause', false))
                mp.add_timeout(0.6, function() mark('POST_RESUME'); after_first_rebuild() end)
            end)
        else
            change_upstream('44100', after_first_rebuild)
        end
    end)
end
mp.add_timeout(1, function()
    initial_path = mp.get_property('path', '')
    for _, track in ipairs(mp.get_property_native('track-list', {})) do
        if track.type == 'audio' and track.selected then initial_track = track.id end
    end
    check_owned()
    mark('START_01')
    dispatch({ 'toggle', 'down', 'down', 'down', 'down', 'down', 'right' }, function()
        mark('PREVIEW_02')
        if scenario == 'saved' or scenario == 'reload' or scenario == 'track' then
            dispatch({ 'down', 'enter' }, function() mark('SAVE_02'); change_host_format() end)
        else
            change_host_format()
        end
    end)
end)
mp.add_timeout(12, function()
    if not finished then mp.msg.error('GAIN_RECOVERY_TIMEOUT'); mp.commandv('quit', 2) end
end)

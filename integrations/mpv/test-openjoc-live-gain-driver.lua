-- Real mpv lifecycle qualification for live gain and Apply Current.
-- Run only against the freshly packaged candidate and synthetic JOC fixture.
local started_at = mp.get_time()
local initial_path
local initial_time = 0
local pre_apply_time = 0
local selected_track_id
local expected_factor = string.format('%.17g', math.pow(10.0, 1 / 200.0))
local failure = false

local function fail(message)
    if failure then return end
    failure = true
    mp.msg.error('OPENJOC_LIVE_GAIN_DRIVER_FAIL: ' .. message)
    mp.commandv('quit', 1)
end

local function selected_openjoc_track()
    local tracks = mp.get_property_native('track-list', {})
    if type(tracks) ~= 'table' then return nil end
    for _, track in ipairs(tracks) do
        if type(track) == 'table' and track.type == 'audio' and track.selected then
            return track
        end
    end
end

local function selected_track_is_stable()
    local track = selected_openjoc_track()
    if not track or mp.get_property('current-tracks/audio/decoder', '') ~= 'libopenjoc' then return false end
    if tostring(mp.get_property('path', '') or '') ~= initial_path then return false end
    if selected_track_id and tostring(track.id or '') ~= selected_track_id then return false end
    return track
end

local function find_gain_filter()
    local filters = mp.get_property_native('af', {})
    if type(filters) ~= 'table' then return nil end
    for _, filter in ipairs(filters) do
        if type(filter) == 'table' and filter.name == 'lavfi'
            and filter.label == 'openjoc_gain' then
            return filter
        end
    end
end

local function graph_factor(filter)
    local graph = type(filter) == 'table' and type(filter.params) == 'table'
        and filter.params.graph or nil
    if type(graph) ~= 'string' then return nil end
    return graph:match('volume@openjoc_gain=volume=([^:]+)')
end

local function factor_is(expected)
    local filter = find_gain_filter()
    return filter and graph_factor(filter) == expected
end

local function gain_filter_exists()
    return find_gain_filter() ~= nil
end

local function property_map(path)
    local raw = mp.get_property_native(path, {})
    local values = {}
    if type(raw) == 'table' then
        for key, value in pairs(raw) do
            if type(key) == 'string' then values[key] = tostring(value) end
        end
    elseif type(raw) == 'string' then
        for entry in raw:gmatch('[^,]+') do
            local key, value = entry:match('^%s*([^=]+)%s*=%s*(.-)%s*$')
            if key then values[key] = value end
        end
    end
    return values
end

local function channel_count()
    local params = mp.get_property_native('audio-params', {})
    if type(params) == 'table' then
        for _, key in ipairs({ 'channel-count', 'channels-count', 'num-channels' }) do
            local value = tonumber(params[key])
            if value then return value end
        end
        local channels = params.channels or params['hr-channels']
        if type(channels) == 'string' then
            if channels == '7.1' or channels == '7.1(wide)' then return 8 end
            if channels == '5.1' or channels == '5.1(side)' then return 6 end
        end
    end
    local count = tonumber(mp.get_property('audio-params/channel-count', ''))
    if count then return count end
    local channels = mp.get_property('audio-params/hr-channels',
        mp.get_property('audio-params/channels', ''))
    if channels == '7.1' or channels == '7.1(wide)' then return 8 end
    if channels == '5.1' or channels == '5.1(side)' then return 6 end
    return nil
end

local function dispatch(action, callback)
    mp.commandv('script-binding', 'openjoc_settings/' .. action)
    mp.add_timeout(0.15, callback)
end

local function repeat_binding(action, count, callback)
    if count == 0 then callback(); return end
    dispatch(action, function() repeat_binding(action, count - 1, callback) end)
end

local function poll_until(description, predicate, deadline, callback)
    local function poll()
        if failure then return end
        local ok, value = pcall(predicate)
        if ok and value then callback(value); return end
        if mp.get_time() >= deadline then
            fail('timed out waiting for ' .. description)
            return
        end
        mp.add_timeout(0.05, poll)
    end
    poll()
end

local function apply_phase()
    pre_apply_time = mp.get_property_number('time-pos', 0) or 0
    mp.msg.info('LIVE_GAIN_PHASE:PRE_APPLYCURRENT')
    dispatch('openjoc-settings-enter', function()
        local deadline = mp.get_time() + 12
        poll_until('file-local OpenJOC option transition to speaker_layout=7.1', function()
            local values = property_map('file-local-options/ad-lavc-o')
            return values.speaker_layout == '7.1' and values.render_mode == 'speaker'
        end, deadline, function()
            local audio_deadline = mp.get_time() + 5
            poll_until('Apply Current 7.1 output and restored +0.1 dB graph', function()
                local track = selected_track_is_stable()
                if not track then return false end
                local params = mp.get_property_native('audio-params', {})
                local rate = type(params) == 'table' and tonumber(params.samplerate)
                    or tonumber(mp.get_property('audio-params/samplerate', ''))
                if channel_count() ~= 8 or rate ~= 48000 or not factor_is(expected_factor) then
                    return false
                end
                return track
            end, audio_deadline, function()
                local current_time = mp.get_property_number('time-pos', 0) or 0
                if current_time <= initial_time or current_time + 0.25 < pre_apply_time then
                    fail('playback timestamp did not advance continuously across Apply Current')
                    return
                end
                mp.msg.info('APPLYCURRENT_REINIT:PASS output_channels=8 samplerate=48000 gain='
                    .. expected_factor .. ' timestamp=' .. string.format('%.3f', current_time))
                -- Allow the final log/debug events to flush before orderly quit.
                mp.add_timeout(0.5, function()
                    if not failure then mp.commandv('quit', 0) end
                end)
            end)
        end)
    end)
end

local function preview_gain()
    if not factor_is('1') then
        fail('startup live gain filter is not present at unity')
        return
    end
    mp.msg.info('LIVE_GAIN_PHASE:PRE_RUNTIME')
    repeat_binding('openjoc-settings-down', 5, function()
        dispatch('openjoc-settings-right', function()
            local deadline = mp.get_time() + 5
            poll_until('UI preview to retain the named gain filter while playback advances', function()
                -- af-command changes the live libavfilter volume AVOption.
                -- The mpv AF property retains the original graph string, so
                -- validate filter presence and decoder continuity here; the
                -- verifier checks FFmpeg's logged runtime value separately.
                return gain_filter_exists()
                    and selected_track_is_stable()
                    and (mp.get_property_number('time-pos', 0) or 0) > initial_time
            end, deadline, function()
                mp.msg.info('LIVE_GAIN_UI_PREVIEW:PASS factor=' .. expected_factor)
                repeat_binding('openjoc-settings-up', 5, function()
                    dispatch('openjoc-settings-right', function()
                        repeat_binding('openjoc-settings-down', 7, apply_phase)
                    end)
                end)
            end)
        end)
    end)
end

local function open_menu()
    local track = selected_openjoc_track()
    if not track or mp.get_property('current-tracks/audio/decoder', '') ~= 'libopenjoc' then
        fail('fixture did not select the libopenjoc decoder')
        return
    end
    initial_path = tostring(mp.get_property('path', '') or '')
    if initial_path == '' or not initial_path:match('joc%.live%-gain%.mp4$') then
        fail('unexpected live-gain fixture path')
        return
    end
    initial_time = mp.get_property_number('time-pos', 0) or 0
    selected_track_id = tostring(track.id or '')
    local deadline = mp.get_time() + 5
    poll_until('startup named gain filter at unity', function()
        return factor_is('1')
    end, deadline, function()
        dispatch('openjoc-settings-toggle', preview_gain)
    end)
end

mp.add_timeout(0.5, function()
    if mp.get_time() - started_at > 5 then
        fail('driver startup exceeded its bounded setup deadline')
        return
    end
    open_menu()
end)
mp.add_timeout(26, function()
    if not failure then fail('overall lifecycle driver deadline exceeded') end
end)

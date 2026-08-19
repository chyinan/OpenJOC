#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libavcodec/codec_id.h>
#include <libavcodec/avcodec.h>
#include <libavcodec/packet.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/channel_layout.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/samplefmt.h>

typedef struct OpenJocAvDemux {
    AVFormatContext *format;
    AVPacket *packet;
} OpenJocAvDemux;

typedef struct OpenJocAvPacketView {
    const uint8_t *data;
    size_t size;
    int64_t pts;
    int64_t dts;
    int64_t duration;
    int stream_index;
} OpenJocAvPacketView;

static void openjoc_av_error(int code, char *buffer, size_t capacity) {
    if (!buffer || capacity == 0)
        return;
    if (av_strerror(code, buffer, capacity) < 0)
        snprintf(buffer, capacity, "FFmpeg error %d", code);
}

unsigned openjoc_avutil_version(void) { return avutil_version(); }
unsigned openjoc_avcodec_version(void) { return avcodec_version(); }
unsigned openjoc_avformat_version(void) { return avformat_version(); }

OpenJocAvDemux *openjoc_av_demux_open(const char *path, char *error,
                                      size_t error_capacity) {
    OpenJocAvDemux *demux = calloc(1, sizeof(*demux));
    if (!demux) {
        openjoc_av_error(AVERROR(ENOMEM), error, error_capacity);
        return NULL;
    }
    int result = avformat_open_input(&demux->format, path, NULL, NULL);
    if (result < 0) {
        openjoc_av_error(result, error, error_capacity);
        free(demux);
        return NULL;
    }
    result = avformat_find_stream_info(demux->format, NULL);
    if (result < 0) {
        openjoc_av_error(result, error, error_capacity);
        avformat_close_input(&demux->format);
        free(demux);
        return NULL;
    }
    demux->packet = av_packet_alloc();
    if (!demux->packet) {
        openjoc_av_error(AVERROR(ENOMEM), error, error_capacity);
        avformat_close_input(&demux->format);
        free(demux);
        return NULL;
    }
    return demux;
}

void openjoc_av_demux_free(OpenJocAvDemux **demux) {
    if (!demux || !*demux)
        return;
    av_packet_free(&(*demux)->packet);
    avformat_close_input(&(*demux)->format);
    free(*demux);
    *demux = NULL;
}

int openjoc_av_demux_find_eac3(const OpenJocAvDemux *demux) {
    if (!demux || !demux->format)
        return AVERROR(EINVAL);
    for (unsigned index = 0; index < demux->format->nb_streams; ++index) {
        const AVStream *stream = demux->format->streams[index];
        if (stream->codecpar->codec_type == AVMEDIA_TYPE_AUDIO &&
            stream->codecpar->codec_id == AV_CODEC_ID_EAC3)
            return (int)index;
    }
    return AVERROR_STREAM_NOT_FOUND;
}

int openjoc_av_demux_time_base(const OpenJocAvDemux *demux, int stream_index,
                               int *numerator, int *denominator) {
    if (!demux || !demux->format || stream_index < 0 ||
        (unsigned)stream_index >= demux->format->nb_streams || !numerator ||
        !denominator)
        return AVERROR(EINVAL);
    const AVRational time_base = demux->format->streams[stream_index]->time_base;
    *numerator = time_base.num;
    *denominator = time_base.den;
    return 0;
}

int openjoc_av_demux_read(OpenJocAvDemux *demux, OpenJocAvPacketView *view,
                          char *error, size_t error_capacity) {
    if (!demux || !demux->packet || !view)
        return AVERROR(EINVAL);
    av_packet_unref(demux->packet);
    const int result = av_read_frame(demux->format, demux->packet);
    if (result == AVERROR_EOF)
        return 0;
    if (result < 0) {
        openjoc_av_error(result, error, error_capacity);
        return result;
    }
    if (demux->packet->size < 0)
        return AVERROR_INVALIDDATA;
    view->data = demux->packet->data;
    view->size = (size_t)demux->packet->size;
    view->pts = demux->packet->pts;
    view->dts = demux->packet->dts;
    view->duration = demux->packet->duration;
    view->stream_index = demux->packet->stream_index;
    return 1;
}

int openjoc_av_demux_seek(OpenJocAvDemux *demux, int stream_index,
                          int64_t timestamp, char *error,
                          size_t error_capacity) {
    if (!demux || !demux->format || stream_index < 0 ||
        (unsigned)stream_index >= demux->format->nb_streams)
        return AVERROR(EINVAL);
    av_packet_unref(demux->packet);
    const int result = av_seek_frame(demux->format, stream_index, timestamp,
                                     AVSEEK_FLAG_BACKWARD);
    if (result < 0) {
        openjoc_av_error(result, error, error_capacity);
        return result;
    }
    avformat_flush(demux->format);
    return 0;
}

int openjoc_av_channel_id(const char *name) {
    if (!name)
        return AV_CHAN_NONE;
    return av_channel_from_string(name);
}

AVFrame *openjoc_av_frame_create(const float *samples, size_t sample_len,
                                 int sample_rate, int nb_samples, int64_t pts,
                                 int has_pts, const int *channel_ids,
                                 int channel_count, const char *standard_layout,
                                 char *error, size_t error_capacity) {
    if (!samples || sample_rate <= 0 || nb_samples <= 0 || channel_count <= 0 ||
        sample_len != (size_t)nb_samples * (size_t)channel_count) {
        openjoc_av_error(AVERROR(EINVAL), error, error_capacity);
        return NULL;
    }
    AVFrame *frame = av_frame_alloc();
    if (!frame) {
        openjoc_av_error(AVERROR(ENOMEM), error, error_capacity);
        return NULL;
    }
    int result;
    if (standard_layout) {
        result = av_channel_layout_from_string(&frame->ch_layout, standard_layout);
    } else {
        result = av_channel_layout_custom_init(&frame->ch_layout, channel_count);
        if (result >= 0) {
            for (int index = 0; index < channel_count; ++index)
                frame->ch_layout.u.map[index].id = channel_ids[index];
        }
    }
    if (result < 0 || frame->ch_layout.nb_channels != channel_count ||
        !av_channel_layout_check(&frame->ch_layout)) {
        if (result >= 0)
            result = AVERROR(EINVAL);
        openjoc_av_error(result, error, error_capacity);
        av_frame_free(&frame);
        return NULL;
    }
    for (int index = 0; index < channel_count; ++index) {
        if (av_channel_layout_channel_from_index(&frame->ch_layout, (unsigned)index) !=
            channel_ids[index]) {
            openjoc_av_error(AVERROR(EINVAL), error, error_capacity);
            av_frame_free(&frame);
            return NULL;
        }
    }
    frame->format = AV_SAMPLE_FMT_FLT;
    frame->sample_rate = sample_rate;
    frame->nb_samples = nb_samples;
    frame->pts = has_pts ? pts : AV_NOPTS_VALUE;
    frame->duration = nb_samples;
    result = av_frame_get_buffer(frame, 0);
    if (result < 0) {
        openjoc_av_error(result, error, error_capacity);
        av_frame_free(&frame);
        return NULL;
    }
    result = av_frame_make_writable(frame);
    if (result < 0) {
        openjoc_av_error(result, error, error_capacity);
        av_frame_free(&frame);
        return NULL;
    }
    memcpy(frame->data[0], samples, sample_len * sizeof(*samples));
    return frame;
}

void openjoc_av_frame_free(AVFrame **frame) { av_frame_free(frame); }

const float *openjoc_av_frame_data(const AVFrame *frame, size_t *sample_len) {
    if (!frame || frame->format != AV_SAMPLE_FMT_FLT || !frame->data[0] ||
        frame->nb_samples < 0 || frame->ch_layout.nb_channels < 0)
        return NULL;
    if (sample_len)
        *sample_len = (size_t)frame->nb_samples *
                      (size_t)frame->ch_layout.nb_channels;
    return (const float *)frame->data[0];
}

int openjoc_av_frame_sample_rate(const AVFrame *frame) {
    return frame ? frame->sample_rate : 0;
}
int openjoc_av_frame_nb_samples(const AVFrame *frame) {
    return frame ? frame->nb_samples : 0;
}
int openjoc_av_frame_channel_count(const AVFrame *frame) {
    return frame ? frame->ch_layout.nb_channels : 0;
}
int64_t openjoc_av_frame_pts(const AVFrame *frame) {
    return frame ? frame->pts : AV_NOPTS_VALUE;
}
int64_t openjoc_av_frame_duration(const AVFrame *frame) {
    return frame ? frame->duration : 0;
}
int openjoc_av_frame_format(const AVFrame *frame) {
    return frame ? frame->format : AV_SAMPLE_FMT_NONE;
}
int openjoc_av_sample_format_flt(void) { return AV_SAMPLE_FMT_FLT; }

int openjoc_av_frame_channel(const AVFrame *frame, unsigned index) {
    if (!frame || index >= (unsigned)frame->ch_layout.nb_channels)
        return AV_CHAN_NONE;
    return av_channel_layout_channel_from_index(&frame->ch_layout, index);
}

int openjoc_av_frame_layout_description(const AVFrame *frame, char *buffer,
                                        size_t capacity) {
    if (!frame || !buffer || capacity == 0)
        return AVERROR(EINVAL);
    return av_channel_layout_describe(&frame->ch_layout, buffer, capacity);
}

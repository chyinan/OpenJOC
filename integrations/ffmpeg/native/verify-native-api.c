#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavutil/channel_layout.h>
#include <libavutil/error.h>
#include <libavutil/log.h>
#include <libavutil/opt.h>

#include <openjoc.h>

static int syncframe_size(const uint8_t *data, size_t remaining,
                          int *stream_type, int *substream_id)
{
    int words;

    if (remaining < 4 || data[0] != 0x0b || data[1] != 0x77)
        return AVERROR_INVALIDDATA;
    *stream_type = data[2] >> 6;
    *substream_id = (data[2] >> 3) & 7;
    words = ((data[2] & 7) << 8) | data[3];
    return (words + 1) * 2;
}

static int access_unit_size(const uint8_t *data, size_t remaining)
{
    int first_type, first_substream, first_size;
    int second_type, second_substream, second_size;

    first_size = syncframe_size(data, remaining, &first_type, &first_substream);
    if (first_size < 0 || first_type != 0 || first_substream != 0 ||
        (size_t)first_size > remaining)
        return AVERROR_INVALIDDATA;
    if ((size_t)first_size == remaining)
        return first_size;
    second_size = syncframe_size(data + first_size, remaining - first_size,
                                 &second_type, &second_substream);
    if (second_size < 0 || second_type == 0)
        return first_size;
    if (second_type != 1 || second_substream != 0 ||
        (size_t)(first_size + second_size) > remaining)
        return AVERROR_INVALIDDATA;
    return first_size + second_size;
}

static AVCodecContext *open_decoder(void)
{
    const AVCodec *codec = avcodec_find_decoder_by_name("libopenjoc");
    AVCodecContext *context;
    int ret;

    if (!codec)
        return NULL;
    context = avcodec_alloc_context3(codec);
    if (!context)
        return NULL;
    context->pkt_timebase = (AVRational){ 1, 48000 };
    context->strict_std_compliance = FF_COMPLIANCE_EXPERIMENTAL;
    ret = av_opt_set(context->priv_data, "speaker_layout", "2.0", 0);
    if (ret < 0 || avcodec_open2(context, codec, NULL) < 0) {
        avcodec_free_context(&context);
        return NULL;
    }
    return context;
}

static int send_bytes(AVCodecContext *context, const uint8_t *data, int size,
                      int64_t pts)
{
    AVPacket packet = {
        .data = (uint8_t *)data,
        .size = size,
        .pts = pts,
        .dts = AV_NOPTS_VALUE,
    };
    return avcodec_send_packet(context, &packet);
}

static int receive_available(AVCodecContext *context, int *frames,
                             int64_t *first_pts, int *first_samples)
{
    AVFrame *frame = av_frame_alloc();
    int ret;

    if (!frame)
        return AVERROR(ENOMEM);
    for (;;) {
        ret = avcodec_receive_frame(context, frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF)
            break;
        if (ret < 0)
            break;
        if (frame->format != AV_SAMPLE_FMT_FLT || frame->sample_rate != 48000 ||
            frame->ch_layout.nb_channels != 2 || !frame->buf[0]) {
            ret = AVERROR_INVALIDDATA;
            break;
        }
        if (*frames == 0) {
            *first_pts = frame->pts;
            *first_samples = frame->nb_samples;
        }
        (*frames)++;
        av_frame_unref(frame);
    }
    av_frame_free(&frame);
    return ret;
}

static int verify_context_shape(const AVCodecContext *context)
{
    const AVChannelLayout stereo = AV_CHANNEL_LAYOUT_STEREO;

    return context->sample_fmt == AV_SAMPLE_FMT_FLT &&
           context->sample_rate == 48000 && context->delay == 609 &&
           av_channel_layout_compare(&context->ch_layout, &stereo) == 0;
}

int main(int argc, char **argv)
{
    AVCodecContext *first = NULL, *second = NULL;
    AVFrame *eof_frame = NULL;
    uint8_t *data = NULL;
    long file_size;
    size_t offsets[5] = { 0 };
    int sizes[4] = { 0 };
    FILE *file = NULL;
    int frames = 0, first_samples = 0, ret = 0, receive_ret = 0, stage = 0;
    int64_t first_pts;
    openjoc_decoder_config bridge_config;
    openjoc_stream_decoder *bridge = NULL;
    openjoc_pcm_frame bridge_frame;
    openjoc_status bridge_first, bridge_second, bridge_third, bridge_receive;

    if (argc != 2) {
        fprintf(stderr, "usage: %s JOC.ec3\n", argv[0]);
        return 2;
    }
    av_log_set_level(AV_LOG_DEBUG);
    stage = 1;
    file = fopen(argv[1], "rb");
    if (!file || fseek(file, 0, SEEK_END) || (file_size = ftell(file)) < 0 ||
        fseek(file, 0, SEEK_SET))
        goto fail;
    data = malloc(file_size);
    if (!data || fread(data, 1, file_size, file) != (size_t)file_size)
        goto fail;
    fclose(file);
    file = NULL;

    stage = 2;
    for (int i = 0; i < 4; i++) {
        sizes[i] = access_unit_size(data + offsets[i], file_size - offsets[i]);
        if (sizes[i] < 0)
            goto fail;
        offsets[i + 1] = offsets[i] + sizes[i];
    }

    openjoc_decoder_config_init(&bridge_config);
    bridge_config.speaker_layout = "2.0";
    if (openjoc_stream_decoder_create(&bridge_config, &bridge) !=
        OPENJOC_STATUS_OK)
        goto fail;
    bridge_first = openjoc_stream_decoder_send_chunk(
        bridge, data, sizes[0] / 2, 0, 0);
    bridge_second = openjoc_stream_decoder_send_chunk(
        bridge, data + sizes[0] / 2, sizes[0] - sizes[0] / 2 + sizes[1],
        OPENJOC_NO_PTS, 0);
    bridge_third = openjoc_stream_decoder_send_chunk(
        bridge, data + offsets[2], sizes[2], 3072, 0);
    openjoc_pcm_frame_init(&bridge_frame);
    bridge_receive = openjoc_stream_decoder_receive_frame(bridge, &bridge_frame);
    if (bridge_first != OPENJOC_STATUS_NEED_MORE_INPUT ||
        bridge_second != OPENJOC_STATUS_NEED_MORE_INPUT ||
        bridge_third != OPENJOC_STATUS_FRAME_AVAILABLE ||
        bridge_receive != OPENJOC_STATUS_FRAME_AVAILABLE ||
        bridge_frame.sample_count != 1536 ||
        openjoc_stream_decoder_get_staged_bytes(bridge) != 4096)
        goto fail;
    openjoc_stream_decoder_destroy(bridge);
    bridge = NULL;

    stage = 3;
    first = open_decoder();
    second = open_decoder();
    if (!first || !second || !verify_context_shape(first) ||
        !verify_context_shape(second))
        goto fail;

    stage = 4;
    ret = send_bytes(first, data, sizes[0] / 2, 0);
    frames = 0;
    first_pts = AV_NOPTS_VALUE;
    first_samples = 0;
    receive_ret = receive_available(first, &frames, &first_pts, &first_samples);
    if (ret < 0 || receive_ret != AVERROR(EAGAIN) ||
        frames != 0)
        goto fail;

    stage = 5;
    ret = send_bytes(first, data + sizes[0] / 2,
                     sizes[0] - sizes[0] / 2 + sizes[1], AV_NOPTS_VALUE);
    receive_ret = receive_available(first, &frames, &first_pts, &first_samples);
    if (ret < 0 || receive_ret != AVERROR(EAGAIN) || frames != 0)
        goto fail;
    ret = send_bytes(first, data + offsets[2], sizes[2], 3072);
    receive_ret = receive_available(first, &frames, &first_pts, &first_samples);
    if (ret < 0 || receive_ret != AVERROR(EAGAIN) || frames != 1 ||
        first_pts != 609 || first_samples != 927)
        goto fail;

    stage = 6;
    avcodec_flush_buffers(first);
    frames = 0;
    first_pts = AV_NOPTS_VALUE;
    first_samples = 0;
    ret = send_bytes(first, data,
                     sizes[0] + sizes[1] + sizes[2] + sizes[3], 0);
    if (ret < 0 || receive_available(first, &frames, &first_pts,
                                     &first_samples) != AVERROR(EAGAIN) ||
        frames != 2 || first_pts != 0 || first_samples != 1536)
        goto fail;

    stage = 7;
    frames = 0;
    first_pts = AV_NOPTS_VALUE;
    first_samples = 0;
    ret = send_bytes(second, data, sizes[0] + sizes[1] + sizes[2], 0);
    if (ret < 0 || receive_available(second, &frames, &first_pts,
                                     &first_samples) != AVERROR(EAGAIN) ||
        frames != 1 || first_pts != 609)
        goto fail;

    stage = 8;
    avcodec_flush_buffers(first);
    ret = send_bytes(first, data + offsets[3], sizes[3] / 2, 0);
    frames = 0;
    if (ret < 0 || receive_available(first, &frames, &first_pts,
                                     &first_samples) != AVERROR(EAGAIN) ||
        frames != 0)
        goto fail;
    avcodec_flush_buffers(first);
    ret = send_bytes(first, data, sizes[0] + sizes[1] + sizes[2], 0);
    if (ret < 0 || receive_available(first, &frames, &first_pts,
                                     &first_samples) != AVERROR(EAGAIN) ||
        frames != 1 || first_pts != 0)
        goto fail;

    stage = 9;
    ret = avcodec_send_packet(first, NULL);
    if (ret < 0)
        goto fail;
    ret = receive_available(first, &frames, &first_pts, &first_samples);
    eof_frame = av_frame_alloc();
    if (ret != AVERROR_EOF || frames < 2 || !eof_frame ||
        avcodec_receive_frame(first, eof_frame) != AVERROR_EOF)
        goto fail;

    printf("fragmentation=PASS multiple_aus=PASS flush=PASS drain=PASS "
           "multi_instance=PASS delay=609 first_pts=609\n");
    avcodec_free_context(&first);
    avcodec_free_context(&second);
    av_frame_free(&eof_frame);
    free(data);
    openjoc_stream_decoder_destroy(bridge);
    return 0;

fail:
    fprintf(stderr,
            "native API verification failed stage=%d ret=%d frames=%d "
            "receive_ret=%d first_pts=%lld first_samples=%d sizes=%d,%d,%d,%d\n",
            stage, ret, frames, receive_ret, (long long)first_pts,
            first_samples, sizes[0], sizes[1], sizes[2], sizes[3]);
    if (file)
        fclose(file);
    avcodec_free_context(&first);
    avcodec_free_context(&second);
    av_frame_free(&eof_frame);
    free(data);
    openjoc_stream_decoder_destroy(bridge);
    return 1;
}

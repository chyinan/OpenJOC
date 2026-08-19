#include <stdint.h>
#include <stdio.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/error.h>
#include <libavutil/opt.h>

static int receive_one(AVCodecContext *decoder, AVFrame *frame)
{
    int ret = avcodec_receive_frame(decoder, frame);

    if (ret < 0)
        return ret;
    if (frame->format != AV_SAMPLE_FMT_FLT || frame->sample_rate != 48000 ||
        frame->ch_layout.nb_channels != 2 || !frame->buf[0])
        return AVERROR_INVALIDDATA;
    return 0;
}

static int read_decoded_frame(AVFormatContext *format, AVCodecContext *decoder,
                              int stream_index, AVFrame *frame)
{
    AVPacket *packet = av_packet_alloc();
    int ret;

    if (!packet)
        return AVERROR(ENOMEM);
    for (;;) {
        ret = receive_one(decoder, frame);
        if (ret >= 0 || ret != AVERROR(EAGAIN))
            break;

        ret = av_read_frame(format, packet);
        if (ret < 0) {
            ret = avcodec_send_packet(decoder, NULL);
            if (ret < 0 && ret != AVERROR_EOF)
                break;
            ret = receive_one(decoder, frame);
            break;
        }
        if (packet->stream_index != stream_index) {
            av_packet_unref(packet);
            continue;
        }
        ret = avcodec_send_packet(decoder, packet);
        av_packet_unref(packet);
        if (ret < 0)
            break;
    }
    av_packet_free(&packet);
    return ret;
}

int main(int argc, char **argv)
{
    const AVCodec *codec;
    AVFormatContext *format = NULL;
    AVCodecContext *decoder = NULL;
    AVFrame *frame = NULL;
    AVStream *stream;
    int stream_index, ret;
    int64_t first_pts = AV_NOPTS_VALUE, pre_seek_last_pts = AV_NOPTS_VALUE;
    int64_t post_seek_pts = AV_NOPTS_VALUE, target;

    if (argc != 2) {
        fprintf(stderr, "usage: %s JOC.mp4\n", argv[0]);
        return 2;
    }
    if (avformat_open_input(&format, argv[1], NULL, NULL) < 0 ||
        avformat_find_stream_info(format, NULL) < 0)
        goto fail;
    codec = avcodec_find_decoder_by_name("libopenjoc");
    stream_index = av_find_best_stream(format, AVMEDIA_TYPE_AUDIO, -1, -1,
                                       NULL, 0);
    if (!codec || stream_index < 0)
        goto fail;
    stream = format->streams[stream_index];
    decoder = avcodec_alloc_context3(codec);
    if (!decoder ||
        avcodec_parameters_to_context(decoder, stream->codecpar) < 0)
        goto fail;
    decoder->pkt_timebase = stream->time_base;
    decoder->strict_std_compliance = FF_COMPLIANCE_EXPERIMENTAL;
    if (av_opt_set(decoder->priv_data, "speaker_layout", "2.0", 0) < 0 ||
        avcodec_open2(decoder, codec, NULL) < 0)
        goto fail;
    frame = av_frame_alloc();
    if (!frame)
        goto fail;

    for (int i = 0; i < 4; i++) {
        ret = read_decoded_frame(format, decoder, stream_index, frame);
        if (ret < 0)
            goto fail;
        if (i == 0)
            first_pts = frame->pts;
        pre_seek_last_pts = frame->pts;
        av_frame_unref(frame);
    }

    target = av_rescale_q(2, (AVRational){ 1, 1 }, stream->time_base);
    if (av_seek_frame(format, stream_index, target, AVSEEK_FLAG_BACKWARD) < 0)
        goto fail;
    avcodec_flush_buffers(decoder);

    for (;;) {
        ret = read_decoded_frame(format, decoder, stream_index, frame);
        if (ret < 0)
            goto fail;
        post_seek_pts = frame->pts;
        if (post_seek_pts >= target)
            break;
        av_frame_unref(frame);
    }

    if (first_pts == AV_NOPTS_VALUE || pre_seek_last_pts == AV_NOPTS_VALUE ||
        post_seek_pts == AV_NOPTS_VALUE || post_seek_pts < target ||
        post_seek_pts <= pre_seek_last_pts)
        goto fail;

    printf("seek=PASS first_pts=%lld pre_seek_last_pts=%lld "
           "target=%lld post_seek_pts=%lld time_base=%d/%d\n",
           (long long)first_pts, (long long)pre_seek_last_pts,
           (long long)target, (long long)post_seek_pts,
           stream->time_base.num, stream->time_base.den);
    av_frame_free(&frame);
    avcodec_free_context(&decoder);
    avformat_close_input(&format);
    return 0;

fail:
    fprintf(stderr, "native seek verification failed\n");
    av_frame_free(&frame);
    avcodec_free_context(&decoder);
    avformat_close_input(&format);
    return 1;
}

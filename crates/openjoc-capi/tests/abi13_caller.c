#include "openjoc_abi13.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct guarded_config {
    openjoc_decoder_config config;
    uint64_t canary;
};

static int read_file(const char *path, uint8_t **data, size_t *size) {
    FILE *file = fopen(path, "rb");
    long length;
    if (file == NULL || fseek(file, 0, SEEK_END) != 0) {
        return 0;
    }
    length = ftell(file);
    if (length <= 0 || fseek(file, 0, SEEK_SET) != 0) {
        fclose(file);
        return 0;
    }
    *size = (size_t)length;
    *data = (uint8_t *)malloc(*size);
    if (*data == NULL || fread(*data, 1, *size, file) != *size) {
        free(*data);
        *data = NULL;
        fclose(file);
        return 0;
    }
    fclose(file);
    return 1;
}

static int receive_available(openjoc_decoder *decoder, size_t *frames) {
    openjoc_pcm_frame frame;
    openjoc_status status;
    if (openjoc_pcm_frame_init(&frame) != OPENJOC_STATUS_OK) {
        return 0;
    }
    while ((status = openjoc_decoder_receive_frame(decoder, &frame)) ==
           OPENJOC_STATUS_FRAME_AVAILABLE) {
        if (frame.channel_count != 6 || frame.sample_count == 0 ||
            frame.data == NULL || frame.data_len == 0) {
            return 0;
        }
        *frames += 1;
    }
    return status == OPENJOC_STATUS_NEED_MORE_INPUT ||
           status == OPENJOC_STATUS_END_OF_STREAM;
}

int main(int argc, char **argv) {
    struct guarded_config guarded;
    uint8_t *packet = NULL;
    size_t packet_size = 0;
    size_t frames = 0;
    openjoc_decoder *decoder = NULL;
    openjoc_status status;

    if (argc != 2 || !read_file(argv[1], &packet, &packet_size)) {
        return 2;
    }
    if (openjoc_get_abi_version() < ((OPENJOC_ABI_VERSION_MAJOR << 16) | 3u)) {
        free(packet);
        return 3;
    }
    memset(&guarded, 0, sizeof(guarded));
    guarded.canary = UINT64_C(0x4f50454e4a4f4331);
    if (openjoc_decoder_config_init(&guarded.config) != OPENJOC_STATUS_OK ||
        guarded.canary != UINT64_C(0x4f50454e4a4f4331)) {
        free(packet);
        return 4;
    }
    guarded.config.speaker_layout = "5.1";
    if (openjoc_decoder_create(&guarded.config, &decoder) != OPENJOC_STATUS_OK ||
        decoder == NULL) {
        free(packet);
        return 5;
    }
    status = openjoc_decoder_send_packet(decoder, packet, packet_size,
                                         OPENJOC_NO_PTS, 0);
    if (status != OPENJOC_STATUS_FRAME_AVAILABLE &&
        status != OPENJOC_STATUS_NEED_MORE_INPUT) {
        openjoc_decoder_destroy(decoder);
        free(packet);
        return 6;
    }
    if (!receive_available(decoder, &frames)) {
        openjoc_decoder_destroy(decoder);
        free(packet);
        return 7;
    }
    status = openjoc_decoder_drain(decoder);
    if (status != OPENJOC_STATUS_FRAME_AVAILABLE &&
        status != OPENJOC_STATUS_END_OF_STREAM) {
        openjoc_decoder_destroy(decoder);
        free(packet);
        return 8;
    }
    if (!receive_available(decoder, &frames) || frames == 0) {
        openjoc_decoder_destroy(decoder);
        free(packet);
        return 9;
    }
    if (openjoc_decoder_flush(decoder) != OPENJOC_STATUS_OK ||
        openjoc_decoder_reset(decoder) != OPENJOC_STATUS_OK) {
        openjoc_decoder_destroy(decoder);
        free(packet);
        return 10;
    }
    openjoc_decoder_destroy(decoder);
    free(packet);
    puts("ABI_1_3_CALLER_ON_1_4_LIBRARY=PASS");
    return 0;
}

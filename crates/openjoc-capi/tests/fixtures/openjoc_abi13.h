#ifndef OPENJOC_ABI13_H
#define OPENJOC_ABI13_H

/* Retained ABI 1.3 declarations copied from START_HEAD 33ef4bc. This header
 * deliberately has no ABI 1.4 custom_speaker_layout field. */
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define OPENJOC_ABI_VERSION_MAJOR 1u
#define OPENJOC_ABI_VERSION_MINOR 3u
#define OPENJOC_NO_PTS INT64_MIN

typedef struct openjoc_decoder openjoc_decoder;

typedef enum openjoc_status {
    OPENJOC_STATUS_OK = 0,
    OPENJOC_STATUS_NEED_MORE_INPUT = 1,
    OPENJOC_STATUS_FRAME_AVAILABLE = 2,
    OPENJOC_STATUS_END_OF_STREAM = 3,
    OPENJOC_STATUS_OUTPUT_PENDING = 4,
    OPENJOC_STATUS_UNSUPPORTED = 5,
    OPENJOC_STATUS_INVALID_ARGUMENT = 6,
    OPENJOC_STATUS_DECODE_ERROR = 7,
    OPENJOC_STATUS_RENDER_ERROR = 8,
    OPENJOC_STATUS_FORMAT_CHANGED = 9,
    OPENJOC_STATUS_REQUIRE_RESET = 10,
    OPENJOC_STATUS_NOT_JOC = 11,
    OPENJOC_STATUS_OUT_OF_MEMORY = 12,
    OPENJOC_STATUS_EXTERNAL_ERROR = 13
} openjoc_status;

typedef struct openjoc_decoder_config {
    uint32_t struct_size;
    uint32_t render_mode;
    const char *speaker_layout;
    uint32_t downmix;
    uint32_t drc;
    uint8_t drc_boost_percent;
    uint8_t drc_cut_percent;
    uint32_t validation_profile;
    const uint8_t *sofa_data;
    size_t sofa_size;
    const char *virtual_layout;
    uint32_t lfe_policy;
    uint32_t dialnorm_mode;
} openjoc_decoder_config;

typedef struct openjoc_pcm_frame {
    uint32_t struct_size;
    uint32_t sample_format;
    uint32_t sample_rate;
    uint32_t channel_count;
    size_t sample_count;
    int64_t pts_samples;
    const float *data;
    size_t data_len;
    const char *layout_name;
    const char *const *channel_labels;
    size_t channel_label_count;
} openjoc_pcm_frame;

uint32_t openjoc_get_abi_version(void);
openjoc_status openjoc_decoder_config_init(openjoc_decoder_config *config);
openjoc_status openjoc_decoder_create(const openjoc_decoder_config *config, openjoc_decoder **output);
void openjoc_decoder_destroy(openjoc_decoder *decoder);
openjoc_status openjoc_decoder_send_packet(openjoc_decoder *decoder, const uint8_t *data, size_t data_len, int64_t pts_samples, uint32_t flags);
openjoc_status openjoc_decoder_receive_frame(openjoc_decoder *decoder, openjoc_pcm_frame *output);
openjoc_status openjoc_decoder_drain(openjoc_decoder *decoder);
openjoc_status openjoc_decoder_flush(openjoc_decoder *decoder);
openjoc_status openjoc_decoder_reset(openjoc_decoder *decoder);
const char *openjoc_decoder_last_error(const openjoc_decoder *decoder);
openjoc_status openjoc_pcm_frame_init(openjoc_pcm_frame *output);

#ifdef __cplusplus
}
#endif

#endif

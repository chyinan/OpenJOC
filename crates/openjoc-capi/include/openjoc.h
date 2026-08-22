#ifndef OPENJOC_H
#define OPENJOC_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define OPENJOC_ABI_VERSION_MAJOR 1u
#define OPENJOC_ABI_VERSION_MINOR 4u
#define OPENJOC_NO_PTS INT64_MIN

typedef struct openjoc_decoder openjoc_decoder;
typedef struct openjoc_stream_decoder openjoc_stream_decoder;
typedef struct openjoc_classifier openjoc_classifier;

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

typedef enum openjoc_classification {
    OPENJOC_CLASSIFICATION_UNKNOWN = 0,
    OPENJOC_CLASSIFICATION_CONFIRMED_JOC = 1,
    OPENJOC_CLASSIFICATION_CONFIRMED_NON_JOC = 2,
    OPENJOC_CLASSIFICATION_INVALID_OR_UNSUPPORTED = 3
} openjoc_classification;

typedef enum openjoc_render_mode {
    OPENJOC_RENDER_SPEAKER = 0,
    OPENJOC_RENDER_STEREO = 1,
    OPENJOC_RENDER_BINAURAL = 2
} openjoc_render_mode;

typedef enum openjoc_downmix_policy {
    OPENJOC_DOWNMIX_AUTO = 0,
    OPENJOC_DOWNMIX_LORO = 1,
    OPENJOC_DOWNMIX_LTRT = 2
} openjoc_downmix_policy;

typedef enum openjoc_drc_mode {
    OPENJOC_DRC_DISABLED = 0,
    OPENJOC_DRC_LINE = 1,
    OPENJOC_DRC_RF = 2,
    OPENJOC_DRC_CUSTOM = 3
} openjoc_drc_mode;

typedef enum openjoc_dialnorm_mode {
    OPENJOC_DIALNORM_DEFAULT = 0,
    OPENJOC_DIALNORM_DIGITAL = 1,
    OPENJOC_DIALNORM_ANALOG = 2
} openjoc_dialnorm_mode;

typedef enum openjoc_validation_profile {
    OPENJOC_VALIDATION_AUTO = 0,
    OPENJOC_VALIDATION_ETSI_STRICT = 1,
    OPENJOC_VALIDATION_OBSERVED_VENDOR_COMPAT = 2
} openjoc_validation_profile;

typedef enum openjoc_lfe_policy {
    OPENJOC_LFE_EXCLUDE = 0,
    OPENJOC_LFE_EQUAL_POWER_DUAL_MONO = 1
} openjoc_lfe_policy;

typedef enum openjoc_speaker_role {
    OPENJOC_SPEAKER_FULL_RANGE = 0,
    OPENJOC_SPEAKER_LFE = 1
} openjoc_speaker_role;

#define OPENJOC_PACKET_FLAG_DISCONTINUITY 1u
#define OPENJOC_PACKET_FLAG_PREROLL 2u

typedef struct openjoc_custom_speaker {
    uint32_t struct_size;
    const char *name;
    double azimuth;
    double elevation;
    uint32_t role;
} openjoc_custom_speaker;

typedef struct openjoc_custom_speaker_layout {
    uint32_t struct_size;
    uint32_t version;
    const char *name;
    const openjoc_custom_speaker *speakers;
    size_t speaker_count;
} openjoc_custom_speaker_layout;

typedef struct openjoc_decoder_config {
    uint32_t struct_size;
    uint32_t render_mode;
    const char *speaker_layout;
    uint32_t downmix;
    uint32_t drc;
    uint8_t drc_boost_percent;
    uint8_t drc_cut_percent;
    uint32_t validation_profile;
    const uint8_t *sofa_data; /* NULL/0 selects the built-in generic HRTF. */
    size_t sofa_size;         /* Nonzero selects a strict caller-provided SOFA. */
    const char *virtual_layout;
    uint32_t lfe_policy;
    /* Appended in ABI minor 1; older struct_size callers use DEFAULT. */
    uint32_t dialnorm_mode;
    /* Appended in ABI minor 4; NULL retains preset-name behavior. */
    const openjoc_custom_speaker_layout *custom_speaker_layout;
} openjoc_decoder_config;

typedef struct openjoc_pcm_frame {
    uint32_t struct_size;
    uint32_t sample_format; /* 1 = interleaved IEEE-754 float32 */
    uint32_t sample_rate;
    uint32_t channel_count;
    size_t sample_count;
    int64_t pts_samples;
    const float *data;
    size_t data_len;
    const char *layout_name;
    const char *const *channel_labels; /* reserved; use get_channel_label in ABI 1.0 */
    size_t channel_label_count;
} openjoc_pcm_frame;

typedef struct openjoc_output_info {
    uint32_t struct_size;
    uint32_t sample_format;
    uint32_t sample_rate;
    uint32_t channel_count;
    size_t latency_samples;
    const char *layout_name;
    const char *const *channel_labels; /* reserved; use get_channel_label in ABI 1.0 */
    size_t channel_label_count;
} openjoc_output_info;

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
openjoc_status openjoc_output_info_init(openjoc_output_info *output);
openjoc_status openjoc_decoder_get_output_info(openjoc_decoder *decoder, openjoc_output_info *output);
const char *openjoc_decoder_get_channel_label(const openjoc_decoder *decoder, size_t index);

/* ABI 1.2 framework-neutral compressed-stream bridge. Input may contain a
 * partial access unit or multiple access units. The bridge owns bounded
 * staging, positively admits JOC before creating the render session, and
 * returns PCM in the semantic order advertised by its channel labels. */
openjoc_status openjoc_stream_decoder_create(const openjoc_decoder_config *config, openjoc_stream_decoder **output);
void openjoc_stream_decoder_destroy(openjoc_stream_decoder *decoder);
openjoc_status openjoc_stream_decoder_send_chunk(openjoc_stream_decoder *decoder, const uint8_t *data, size_t data_len, int64_t pts_samples, uint32_t flags);
openjoc_status openjoc_stream_decoder_receive_frame(openjoc_stream_decoder *decoder, openjoc_pcm_frame *output);
openjoc_status openjoc_stream_decoder_drain(openjoc_stream_decoder *decoder);
openjoc_status openjoc_stream_decoder_flush(openjoc_stream_decoder *decoder);
openjoc_status openjoc_stream_decoder_reset(openjoc_stream_decoder *decoder);
const char *openjoc_stream_decoder_last_error(const openjoc_stream_decoder *decoder);
openjoc_status openjoc_stream_decoder_get_output_info(openjoc_stream_decoder *decoder, openjoc_output_info *output);
const char *openjoc_stream_decoder_get_channel_label(const openjoc_stream_decoder *decoder, size_t index);
const char *openjoc_stream_decoder_get_config_descriptor(const openjoc_stream_decoder *decoder);
const char *openjoc_stream_decoder_get_config_fingerprint(const openjoc_stream_decoder *decoder);
size_t openjoc_stream_decoder_get_staged_bytes(const openjoc_stream_decoder *decoder);

/* ABI 1.3 decode-free compressed-stream classifier. It shares the bounded
 * access-unit parser and positive admission rules with the stream decoder,
 * but never creates a render session or emits PCM. */
openjoc_status openjoc_classifier_create(openjoc_classifier **output);
void openjoc_classifier_destroy(openjoc_classifier *classifier);
openjoc_status openjoc_classifier_send_chunk(openjoc_classifier *classifier, const uint8_t *data, size_t data_len, openjoc_classification *output);
openjoc_status openjoc_classifier_finish(openjoc_classifier *classifier, openjoc_classification *output);
openjoc_status openjoc_classifier_reset(openjoc_classifier *classifier);
const char *openjoc_classifier_last_error(const openjoc_classifier *classifier);
size_t openjoc_classifier_get_staged_bytes(const openjoc_classifier *classifier);
size_t openjoc_classifier_get_inspected_bytes(const openjoc_classifier *classifier);

#ifdef __cplusplus
}
#endif

#endif /* OPENJOC_H */

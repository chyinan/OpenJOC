#ifndef OPENJOC_H
#define OPENJOC_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define OPENJOC_ABI_VERSION_MAJOR 1u
#define OPENJOC_ABI_VERSION_MINOR 7u
#define OPENJOC_NO_PTS INT64_MIN
#define OPENJOC_NO_ORIENTATION_SEQUENCE UINT64_MAX

typedef struct openjoc_decoder openjoc_decoder;
typedef struct openjoc_stream_decoder openjoc_stream_decoder;
typedef struct openjoc_classifier openjoc_classifier;
typedef struct openjoc_listener_orientation_preparer openjoc_listener_orientation_preparer;
typedef struct openjoc_listener_orientation_update openjoc_listener_orientation_update;
typedef struct openjoc_listener_orientation_retired openjoc_listener_orientation_retired;

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

typedef enum openjoc_hrtf_preset {
    OPENJOC_HRTF_SADIE_D1_KU100 = 0,
    OPENJOC_HRTF_SADIE_D2_KEMAR = 1
} openjoc_hrtf_preset;

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
    /* Appended in ABI minor 6; 0 preserves the SADIE II D1 default. */
    uint32_t hrtf_preset;
    /* Reserves ABI 1.6 trailing alignment so the next field starts after the
     * old struct sizeof on both 32-bit and 64-bit targets. Initialize to 0. */
    uint32_t reserved_v1_6_padding;
    /* ABI 1.7: 0 disables pull mode; 1..=256 bounds samples per binaural
     * receive in the experimental listener-orientation mode. */
    uint32_t listener_orientation_pull_samples;
} openjoc_decoder_config;

typedef struct openjoc_pcm_frame {
    uint32_t struct_size;
    uint32_t sample_format; /* 1 = interleaved IEEE-754 float32 */
    uint32_t sample_rate;
    uint32_t channel_count;
    size_t sample_count;
    int64_t pts_samples;
    const float *data;
    size_t data_len; /* byte length of the interleaved float32 payload */
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

typedef struct openjoc_listener_orientation {
    uint32_t struct_size;
    uint32_t reserved;
    double x;
    double y;
    double z;
    double w;
} openjoc_listener_orientation;

typedef struct openjoc_listener_orientation_state {
    uint32_t struct_size;
    uint32_t has_last_applied;
    uint32_t has_pending_update;
    uint32_t reserved;
    uint64_t stream_epoch;
    uint64_t last_applied_sequence; /* UINT64_MAX when absent. */
    uint64_t last_applied_logical_start_sample;
    uint64_t pending_sequence;      /* UINT64_MAX when absent. */
    size_t pending_binaural_input_samples;
} openjoc_listener_orientation_state;

#define OPENJOC_LIVE_INSPECTION_SCHEMA_VERSION 1u
#define OPENJOC_LIVE_INSPECTION_TEXT_CAPACITY 512u
#define OPENJOC_LIVE_INSPECTION_SHORT_TEXT_CAPACITY 128u
#define OPENJOC_LIVE_INSPECTION_TINY_TEXT_CAPACITY 64u
#define OPENJOC_LIVE_INSPECTION_FORMAT_CAPACITY 32u

typedef struct openjoc_live_inspection_snapshot {
    uint32_t struct_size;
    uint32_t schema_version;
    uint64_t observation_epoch;
    uint8_t stream_present;
    uint8_t joc_present;
    /* 0 unknown, 1 no dynamic metadata observed, 2 dynamic metadata observed. */
    uint8_t dynamic_scene_observed;
    /* 0 unknown, 1 absent, 2 present. */
    uint8_t lfe_presence;
    uint8_t has_sample_rate;
    uint8_t has_timestamp;
    uint8_t has_object_count;
    uint8_t has_complexity;
    uint8_t has_first_change;
    uint8_t reserved[3];
    uint32_t sample_rate_hz;
    uint16_t object_count;
    uint16_t complexity;
    uint64_t observed_au_count;
    uint64_t malformed_observed_count;
    uint64_t current_decode_sequence;
    double current_timestamp_seconds;
    uint64_t first_change_au;
    uint64_t first_change_sample;
    double first_change_seconds;
    int32_t profile_index;
    char inspection_kind[OPENJOC_LIVE_INSPECTION_TINY_TEXT_CAPACITY];
    char observation_scope[OPENJOC_LIVE_INSPECTION_TINY_TEXT_CAPACITY];
    char coverage[OPENJOC_LIVE_INSPECTION_FORMAT_CAPACITY];
    char format[OPENJOC_LIVE_INSPECTION_FORMAT_CAPACITY];
    char profile_display_name[OPENJOC_LIVE_INSPECTION_SHORT_TEXT_CAPACITY];
    char reconstruction_carriers[OPENJOC_LIVE_INSPECTION_TEXT_CAPACITY];
    char programme_topology[OPENJOC_LIVE_INSPECTION_TEXT_CAPACITY];
    char dependent_ids[OPENJOC_LIVE_INSPECTION_SHORT_TEXT_CAPACITY];
    char block_partition[OPENJOC_LIVE_INSPECTION_SHORT_TEXT_CAPACITY];
    char lfe_owner[OPENJOC_LIVE_INSPECTION_TINY_TEXT_CAPACITY];
    char lfe_semantics[OPENJOC_LIVE_INSPECTION_SHORT_TEXT_CAPACITY];
    char joc_owner[OPENJOC_LIVE_INSPECTION_TINY_TEXT_CAPACITY];
    char carriage_locations[OPENJOC_LIVE_INSPECTION_TEXT_CAPACITY];
    char etsi_strict[OPENJOC_LIVE_INSPECTION_FORMAT_CAPACITY];
    char deployed_compatibility[OPENJOC_LIVE_INSPECTION_FORMAT_CAPACITY];
    char emdf_payloads[OPENJOC_LIVE_INSPECTION_SHORT_TEXT_CAPACITY];
    char last_error_summary[OPENJOC_LIVE_INSPECTION_TEXT_CAPACITY];
    char programme_layout[OPENJOC_LIVE_INSPECTION_SHORT_TEXT_CAPACITY];
} openjoc_live_inspection_snapshot;

uint32_t openjoc_get_abi_version(void);
/* Legacy-safe ABI 1.3 prefix initializer. Use v1_4 for custom_speaker_layout,
 * v1_6 for the exact v1.6 prefix/hrtf_preset, and v1_7 for the full current
 * struct including listener_orientation_pull_samples. */
openjoc_status openjoc_decoder_config_init(openjoc_decoder_config *config);
openjoc_status openjoc_decoder_config_init_v1_4(openjoc_decoder_config *config);
openjoc_status openjoc_decoder_config_init_v1_6(openjoc_decoder_config *config);
openjoc_status openjoc_decoder_config_init_v1_7(openjoc_decoder_config *config);
openjoc_status openjoc_decoder_create(const openjoc_decoder_config *config, openjoc_decoder **output);
void openjoc_decoder_destroy(openjoc_decoder *decoder);
/* PTS is the first sample of this complete AU; OPENJOC_NO_PTS omits it.
 * The first provided PTS anchors the segment, subtracting previously decoded
 * samples. Earlier returned timestamps stay unchanged. Later PTS must match
 * sample-count continuation; an unrepresentable origin/expected PTS is rejected.
 * Reset/flush/discontinuity clears the anchor. Stream-decoder rules differ. */
openjoc_status openjoc_decoder_send_packet(openjoc_decoder *decoder, const uint8_t *data, size_t data_len, int64_t pts_samples, uint32_t flags);
/* An actual output PTS equal to OPENJOC_NO_PTS is unrepresentable in C:
 * receive returns RENDER_ERROR without writing output and requires reset/flush. */
openjoc_status openjoc_decoder_receive_frame(openjoc_decoder *decoder, openjoc_pcm_frame *output);
openjoc_status openjoc_decoder_drain(openjoc_decoder *decoder);
openjoc_status openjoc_decoder_flush(openjoc_decoder *decoder);
openjoc_status openjoc_decoder_reset(openjoc_decoder *decoder);
const char *openjoc_decoder_last_error(const openjoc_decoder *decoder);
openjoc_status openjoc_pcm_frame_init(openjoc_pcm_frame *output);
openjoc_status openjoc_output_info_init(openjoc_output_info *output);
openjoc_status openjoc_decoder_get_output_info(openjoc_decoder *decoder, openjoc_output_info *output);
const char *openjoc_decoder_get_channel_label(const openjoc_decoder *decoder, size_t index);

/* Experimental orientation pull API. Create a decoder with nonzero
 * listener_orientation_pull_samples, obtain its immutable preparer before
 * handing the decoder to the render thread, then prepare updates on a worker
 * using the state stream_epoch. Different preparer handles may be used in
 * parallel; serialize prepare and last_error calls made through the same
 * handle. Pose submission is block-scheduled and carries no sensor/device
 * timestamp. A failed validation/lifecycle apply leaves *update usable. On
 * success *update becomes NULL and *retired receives the same allocation,
 * possibly with zero kernels, to release on a control thread. If a panic is
 * contained, the decoder is poisoned and the update handle may be empty; reset
 * or recreate the decoder and destroy that handle rather than retrying it. */
openjoc_status openjoc_listener_orientation_state_init(openjoc_listener_orientation_state *output);
openjoc_status openjoc_listener_orientation_init(openjoc_listener_orientation *output);
openjoc_status openjoc_decoder_get_listener_orientation_state(openjoc_decoder *decoder, openjoc_listener_orientation_state *output);
openjoc_status openjoc_stream_decoder_get_listener_orientation_state(openjoc_stream_decoder *decoder, openjoc_listener_orientation_state *output);
openjoc_status openjoc_decoder_get_listener_orientation_preparer(openjoc_decoder *decoder, openjoc_listener_orientation_preparer **output);
openjoc_status openjoc_stream_decoder_get_listener_orientation_preparer(openjoc_stream_decoder *decoder, openjoc_listener_orientation_preparer **output);
openjoc_status openjoc_listener_orientation_prepare(const openjoc_listener_orientation_preparer *preparer, const openjoc_listener_orientation *orientation, uint64_t stream_epoch, uint64_t sequence, openjoc_listener_orientation_update **output);
const char *openjoc_listener_orientation_preparer_last_error(const openjoc_listener_orientation_preparer *preparer);
void openjoc_listener_orientation_preparer_destroy(openjoc_listener_orientation_preparer *preparer);
openjoc_status openjoc_decoder_apply_listener_orientation(openjoc_decoder *decoder, openjoc_listener_orientation_update **update, uint64_t *accepted_sequence, uint64_t *superseded_sequence, openjoc_listener_orientation_retired **retired);
openjoc_status openjoc_stream_decoder_apply_listener_orientation(openjoc_stream_decoder *decoder, openjoc_listener_orientation_update **update, uint64_t *accepted_sequence, uint64_t *superseded_sequence, openjoc_listener_orientation_retired **retired);
void openjoc_listener_orientation_update_destroy(openjoc_listener_orientation_update *update);
size_t openjoc_listener_orientation_retired_count(const openjoc_listener_orientation_retired *retired);
void openjoc_listener_orientation_retired_destroy(openjoc_listener_orientation_retired *retired);

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
openjoc_status openjoc_live_inspection_snapshot_init(openjoc_live_inspection_snapshot *output);
openjoc_status openjoc_stream_decoder_get_live_inspection_snapshot(const openjoc_stream_decoder *decoder, openjoc_live_inspection_snapshot *output);
openjoc_status openjoc_stream_decoder_copy_live_inspection_json(const openjoc_stream_decoder *decoder, char *output, size_t output_capacity, size_t *required_size);

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

#include "openjoc.h"
#include <assert.h>
#include <math.h>
#include <stdint.h>

int main(void) {
    for (uint32_t hrtf = OPENJOC_HRTF_SADIE_D1_KU100;
         hrtf <= OPENJOC_HRTF_SADIE_D2_KEMAR; ++hrtf) {
        openjoc_decoder_config config;
        assert(openjoc_decoder_config_init_v1_7(&config) == OPENJOC_STATUS_OK);
        config.render_mode = OPENJOC_RENDER_BINAURAL;
        config.speaker_layout = "22.2";
        config.hrtf_preset = hrtf;
        config.listener_orientation_pull_samples = 128;
        openjoc_decoder *decoder = NULL;
        assert(openjoc_decoder_create(&config, &decoder) == OPENJOC_STATUS_OK);
        openjoc_listener_orientation_preparer *preparer = NULL;
        assert(openjoc_decoder_get_listener_orientation_preparer(decoder, &preparer) == OPENJOC_STATUS_OK);
        openjoc_listener_orientation orientation;
        assert(openjoc_listener_orientation_init(&orientation) == OPENJOC_STATUS_OK);
        for (uint64_t sequence = 1; sequence <= 2; ++sequence) {
            if (sequence == 2) {
                const double half_yaw = 9.0 * 3.14159265358979323846 / 360.0;
                orientation.z = sin(half_yaw);
                orientation.w = cos(half_yaw);
            }
            openjoc_listener_orientation_update *update = NULL;
            assert(openjoc_listener_orientation_prepare(preparer, &orientation, 0, sequence, &update) == OPENJOC_STATUS_OK);
            uint64_t accepted = 0, superseded = 0;
            openjoc_listener_orientation_retired *retired = NULL;
            assert(openjoc_decoder_apply_listener_orientation(decoder, &update, &accepted, &superseded, &retired) == OPENJOC_STATUS_OK);
            assert(update == NULL && accepted == sequence);
            assert(openjoc_listener_orientation_retired_count(retired) == (sequence == 1 ? 0 : 22));
            openjoc_listener_orientation_retired_destroy(retired);
        }
        openjoc_listener_orientation_preparer_destroy(preparer);
        openjoc_decoder_destroy(decoder);
    }
    return 0;
}

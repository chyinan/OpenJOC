/*
 * Minimal C ABI lifecycle smoke example.
 *
 * Build after `cargo build -p openjoc-capi`:
 *   cc -Icrates/openjoc-capi/include examples/c_api_example.c \
 *      target/debug/libopenjoc_capi.a -ldl -lm -o /tmp/openjoc-c-api-example
 */
#include "openjoc.h"

#include <assert.h>
#include <stdio.h>

int main(void) {
    openjoc_decoder_config config;
    assert(openjoc_decoder_config_init_v1_4(&config) == OPENJOC_STATUS_OK);

    openjoc_decoder *decoder = NULL;
    assert(openjoc_decoder_create(&config, &decoder) == OPENJOC_STATUS_OK);
    assert(decoder != NULL);

    openjoc_output_info info = {0};
    assert(openjoc_output_info_init(&info) == OPENJOC_STATUS_OK);
    assert(openjoc_decoder_get_output_info(decoder, &info) == OPENJOC_STATUS_OK);
    printf("ABI 0x%08x, layout %s, channels %u, latency %zu\n",
           openjoc_get_abi_version(), info.layout_name, info.channel_count,
           info.latency_samples);

    /* This intentionally malformed packet exercises numeric error delivery. */
    const unsigned char malformed[] = {0x0b, 0x77};
    openjoc_status send = openjoc_decoder_send_packet(
        decoder, malformed, sizeof(malformed), OPENJOC_NO_PTS, 0);
    assert(send == OPENJOC_STATUS_DECODE_ERROR ||
           send == OPENJOC_STATUS_INVALID_ARGUMENT);
    printf("malformed packet: %s\n", openjoc_decoder_last_error(decoder));

    assert(openjoc_decoder_flush(decoder) == OPENJOC_STATUS_OK);
    assert(openjoc_decoder_drain(decoder) == OPENJOC_STATUS_END_OF_STREAM);
    openjoc_pcm_frame frame;
    assert(openjoc_pcm_frame_init(&frame) == OPENJOC_STATUS_OK);
    assert(openjoc_decoder_receive_frame(decoder, &frame) ==
           OPENJOC_STATUS_END_OF_STREAM);
    openjoc_decoder_destroy(decoder);
    return 0;
}

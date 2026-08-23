#include "openjoc.h"

int main() {
    openjoc_decoder_config config{};
    return openjoc_decoder_config_init_v1_4(&config) == OPENJOC_STATUS_OK ? 0 : 1;
}

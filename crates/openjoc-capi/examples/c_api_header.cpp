#include "openjoc.h"

int main() {
    openjoc_decoder_config config{};
    return openjoc_decoder_config_init(&config) == OPENJOC_STATUS_OK ? 0 : 1;
}
